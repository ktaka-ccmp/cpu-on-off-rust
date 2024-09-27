use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct CpuInfo {
    id: usize,
    core_id: usize,
    socket_id: usize,
    thread_siblings: Vec<usize>,
    mhz: f64,
    max_mhz: f64,
    min_mhz: f64,
    online: bool,
}

struct SystemTopology {
    cpus: HashMap<usize, CpuInfo>,
    sockets: HashMap<usize, Vec<usize>>,
    cpu0_socket: usize,
}

impl SystemTopology {
    fn new() -> io::Result<Self> {
        let mut cpus = HashMap::new();
        let mut sockets = HashMap::new();
        let mut cpu0_socket = 0;

        let cpu_dir = Path::new("/sys/devices/system/cpu");
        for entry in fs::read_dir(cpu_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(cpu_name) = path.file_name().and_then(|n| n.to_str()) {
                if cpu_name.starts_with("cpu") && cpu_name[3..].parse::<usize>().is_ok() {
                    let id = cpu_name[3..].parse().unwrap();
                    let core_id = fs::read_to_string(path.join("topology/core_id"))?
                        .trim()
                        .parse()
                        .unwrap();
                    let socket_id = fs::read_to_string(path.join("topology/physical_package_id"))?
                        .trim()
                        .parse()
                        .unwrap();
                    let thread_siblings = Self::read_thread_siblings(&path)?;

                    let (mhz, max_mhz, min_mhz) = Self::read_cpu_freq(&path)?;
                    let online = Self::is_cpu_online(&path)?;

                    if id == 0 {
                        cpu0_socket = socket_id;
                    }

                    let cpu_info = CpuInfo {
                        id,
                        core_id,
                        socket_id,
                        thread_siblings,
                        mhz,
                        max_mhz,
                        min_mhz,
                        online,
                    };
                    cpus.insert(id, cpu_info);

                    sockets.entry(socket_id).or_insert_with(Vec::new).push(id);
                }
            }
        }

        Ok(SystemTopology {
            cpus,
            sockets,
            cpu0_socket,
        })
    }

    fn read_thread_siblings(cpu_path: &Path) -> io::Result<Vec<usize>> {
        let siblings_path = cpu_path.join("topology/thread_siblings_list");
        let siblings_str = fs::read_to_string(siblings_path)?;
        Ok(siblings_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect())
    }

    fn read_cpu_freq(cpu_path: &Path) -> io::Result<(f64, f64, f64)> {
        let freq_dir = cpu_path.join("cpufreq");
        let cur_freq = fs::read_to_string(freq_dir.join("scaling_cur_freq"))
            .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0)
            / 1000.0;
        let max_freq = fs::read_to_string(freq_dir.join("scaling_max_freq"))
            .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0)
            / 1000.0;
        let min_freq = fs::read_to_string(freq_dir.join("scaling_min_freq"))
            .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
            .unwrap_or(0.0)
            / 1000.0;
        Ok((cur_freq, max_freq, min_freq))
    }

    fn is_cpu_online(cpu_path: &Path) -> io::Result<bool> {
        let online_path = cpu_path.join("online");
        if online_path.exists() {
            let content = fs::read_to_string(online_path)?;
            Ok(content.trim() == "1")
        } else {
            // If the 'online' file doesn't exist, the CPU is always online
            Ok(true)
        }
    }

    fn select_cpu_to_offline(&self) -> Option<Vec<usize>> {
        // First, try to select a core from a non-CPU0 socket
        for (&socket_id, cpus) in &self.sockets {
            if socket_id != self.cpu0_socket {
                if let Some(core) = self.select_core_from_socket(socket_id) {
                    return Some(core);
                }
            }
        }

        // If all other sockets are offline, select from CPU0 socket, but never offline CPU0 itself
        self.select_core_from_socket(self.cpu0_socket)
    }

    fn select_core_from_socket(&self, socket_id: usize) -> Option<Vec<usize>> {
        self.sockets.get(&socket_id).and_then(|cpus| {
            cpus.iter()
                .filter(|&&cpu_id| cpu_id != 0 && self.cpus[&cpu_id].online)
                .max_by_key(|&&cpu_id| cpu_id)
                .map(|&cpu_id| self.cpus[&cpu_id].thread_siblings.clone())
        })
    }

    fn offline_cpu_group(&mut self, cpu_ids: &[usize]) -> io::Result<()> {
        for &id in cpu_ids {
            if id == 0 {
                continue;
            } // Never offline CPU0
            let path = Path::new("/sys/devices/system/cpu")
                .join(format!("cpu{}", id))
                .join("online");
            if path.exists() {
                let current_state = fs::read_to_string(&path)?.trim().to_string();
                if current_state == "1" {
                    fs::write(&path, "0")?;
                    if let Some(cpu) = self.cpus.get_mut(&id) {
                        cpu.online = false;
                    }
                    println!("Offlined CPU {}", id);
                }
            }
        }
        Ok(())
    }

    fn online_cpu_group(&mut self, cpu_ids: &[usize]) -> io::Result<()> {
        for &id in cpu_ids {
            let path = Path::new("/sys/devices/system/cpu")
                .join(format!("cpu{}", id))
                .join("online");
            if path.exists() {
                fs::write(&path, "1")?;
                if let Some(cpu) = self.cpus.get_mut(&id) {
                    cpu.online = true;
                }
                println!("Onlined CPU {}", id);
            }
        }
        Ok(())
    }

    fn update_cpu_frequencies(&mut self) -> io::Result<()> {
        for cpu in self.cpus.values_mut() {
            let cpu_path = Path::new("/sys/devices/system/cpu").join(format!("cpu{}", cpu.id));
            let (mhz, max_mhz, min_mhz) = Self::read_cpu_freq(&cpu_path)?;
            cpu.mhz = mhz;
            cpu.max_mhz = max_mhz;
            cpu.min_mhz = min_mhz;
        }
        Ok(())
    }

    fn print_summary(&self) {
        println!("System Topology Summary:");
        println!("Total CPUs: {}", self.cpus.len());
        println!("Total Sockets: {}", self.sockets.len());
        println!("CPU0 Socket: {}", self.cpu0_socket);

        for (&socket_id, cpus) in &self.sockets {
            println!("Socket {}: {} CPUs", socket_id, cpus.len());
            let online_cpus = cpus
                .iter()
                .filter(|&&cpu_id| self.cpus[&cpu_id].online)
                .count();
            println!("  Online CPUs: {}", online_cpus);
        }
    }
}

fn main() -> io::Result<()> {
    let mut topology = SystemTopology::new()?;
    topology.print_summary();

    loop {
        topology.update_cpu_frequencies()?;

        let total_mhz: f64 = topology
            .cpus
            .values()
            .filter(|cpu| cpu.online)
            .map(|cpu| cpu.mhz)
            .sum();
        let online_count = topology.cpus.values().filter(|cpu| cpu.online).count();
        let avg_mhz = total_mhz / online_count as f64;
        let max_mhz = topology
            .cpus
            .values()
            .filter(|cpu| cpu.online)
            .map(|cpu| cpu.max_mhz)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        println!(
            "Average CPU MHz: {:.2}, Max CPU MHz: {:.2}",
            avg_mhz, max_mhz
        );

        if avg_mhz > max_mhz * 0.85 {
            println!("High load detected, onlining all CPUs");
            let all_cpus: Vec<usize> = topology.cpus.keys().cloned().collect();
            topology.online_cpu_group(&all_cpus)?;
        } else if avg_mhz < max_mhz * 0.5 {
            let online_cores: HashSet<_> = topology
                .cpus
                .values()
                .filter(|cpu| cpu.online)
                .map(|cpu| (cpu.socket_id, cpu.core_id))
                .collect();

            if online_cores.len() > 1 {
                // Always keep at least one core online
                if let Some(core_to_offline) = topology.select_cpu_to_offline() {
                    println!("Low load detected, offlining core {:?}", core_to_offline);
                    topology.offline_cpu_group(&core_to_offline)?;
                }
            } else {
                println!("Cannot offline more CPUs, already at minimum");
            }
        } else {
            println!("Load is optimal, no action needed");
        }

        thread::sleep(Duration::from_secs(1));
    }
}
