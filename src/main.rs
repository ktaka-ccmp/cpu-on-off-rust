use clap::Parser;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// High load threshold percentage (default: 85)
    #[arg(short, long, default_value_t = 85)]
    upper_threshold: u8,

    /// Low load threshold percentage (default: 50)
    #[arg(short, long, default_value_t = 50)]
    lower_threshold: u8,
}

#[allow(dead_code)]
struct CpuInfo {
    id: usize,
    core_id: Option<usize>,
    socket_id: Option<usize>,
    thread_siblings: Vec<usize>,
    mhz: f64,
    max_mhz: f64,
    min_mhz: f64,
    online: bool,
}

struct SystemTopology {
    cpus: HashMap<usize, CpuInfo>,
    sockets: HashMap<usize, Vec<usize>>,
    cpu0_socket: Option<usize>,
}

impl SystemTopology {
    fn new() -> io::Result<Self> {
        let mut cpus = HashMap::new();
        let mut sockets = HashMap::new();
        let mut cpu0_socket = None;

        let cpu_dir = Path::new("/sys/devices/system/cpu");
        println!("Reading CPU information from: {:?}", cpu_dir);
        for entry in fs::read_dir(cpu_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(cpu_name) = path.file_name().and_then(|n| n.to_str()) {
                if cpu_name.starts_with("cpu") && cpu_name[3..].parse::<usize>().is_ok() {
                    let id = cpu_name[3..].parse().unwrap();
                    println!("Processing CPU {}", id);

                    let core_id = fs::read_to_string(path.join("topology/core_id"))
                        .ok()
                        .and_then(|s| s.trim().parse().ok());

                    let socket_id = fs::read_to_string(path.join("topology/physical_package_id"))
                        .ok()
                        .and_then(|s| s.trim().parse().ok());

                    let thread_siblings = Self::read_thread_siblings(&path);

                    let (mhz, max_mhz, min_mhz) = Self::read_cpu_freq(&path);

                    let online = Self::is_cpu_online(&path);

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

                    if let Some(socket_id) = socket_id {
                        sockets.entry(socket_id).or_insert_with(Vec::new).push(id);
                    }
                }
            }
        }

        println!("Finished reading CPU information");
        println!("Found {} CPUs across {} sockets", cpus.len(), sockets.len());

        Ok(SystemTopology {
            cpus,
            sockets,
            cpu0_socket,
        })
    }

    fn read_thread_siblings(cpu_path: &Path) -> Vec<usize> {
        let siblings_path = cpu_path.join("topology/thread_siblings_list");
        fs::read_to_string(&siblings_path)
            .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_default()
    }

    fn read_cpu_freq(cpu_path: &Path) -> (f64, f64, f64) {
        let freq_dir = cpu_path.join("cpufreq");
        let cur_freq = fs::read_to_string(freq_dir.join("scaling_cur_freq"))
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
            / 1000.0;
        let max_freq = fs::read_to_string(freq_dir.join("scaling_max_freq"))
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
            / 1000.0;
        let min_freq = fs::read_to_string(freq_dir.join("scaling_min_freq"))
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
            / 1000.0;
        (cur_freq, max_freq, min_freq)
    }

    fn is_cpu_online(cpu_path: &Path) -> bool {
        let online_path = cpu_path.join("online");
        if online_path.exists() {
            fs::read_to_string(online_path)
                .map(|content| content.trim() == "1")
                .unwrap_or(false)
        } else {
            true // CPU is always online if the 'online' file doesn't exist
        }
    }

    fn select_cpu_to_offline(&self) -> Option<Vec<usize>> {
        let online_cpus: Vec<_> = self
            .cpus
            .values()
            .filter(|cpu| cpu.online && cpu.id != 0) // Exclude CPU0
            .collect();

        if online_cpus.len() <= 1 {
            return None; // Don't offline if only CPU0 or one other CPU is online
        }

        online_cpus.into_iter().max_by_key(|cpu| cpu.id).map(|cpu| {
            let siblings = &cpu.thread_siblings;
            siblings
                .iter()
                .filter(|&&sibling_id| {
                    self.cpus
                        .get(&sibling_id)
                        .map_or(false, |sibling| sibling.online)
                })
                .copied()
                .collect()
        })
    }

    fn select_cpu_to_online(&self) -> Option<Vec<usize>> {
        let offline_cpus: Vec<_> = self
            .cpus
            .values()
            .filter(|cpu| !cpu.online && cpu.id != 0) // Exclude CPU0
            .collect();

        if offline_cpus.is_empty() {
            return None; // Don't online if all CPUs are already online
        }

        offline_cpus
            .into_iter()
            .min_by_key(|cpu| cpu.id)
            .map(|cpu| {
                let siblings = &cpu.thread_siblings;
                siblings
                    .iter()
                    .filter(|&&sibling_id| {
                        self.cpus
                            .get(&sibling_id)
                            .map_or(false, |sibling| !sibling.online)
                    })
                    .copied()
                    .collect()
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
                fs::write(&path, "0")?;
                if let Some(cpu) = self.cpus.get_mut(&id) {
                    cpu.online = false;
                }
                println!("Offlined CPU {}", id);
            } else {
                println!("Cannot offline CPU {}: 'online' file does not exist", id);
            }
        }
        Ok(())
    }

    fn online_cpu_group(&mut self, cpu_ids: &[usize]) -> io::Result<()> {
        for &id in cpu_ids {
            if id == 0 {
                continue;
            } // CPU0 is always online
            let path = Path::new("/sys/devices/system/cpu")
                .join(format!("cpu{}", id))
                .join("online");
            if path.exists() {
                fs::write(&path, "1")?;
                if let Some(cpu) = self.cpus.get_mut(&id) {
                    cpu.online = true;
                }
                println!("Onlined CPU {}", id);
            } else {
                println!("Cannot online CPU {}: 'online' file does not exist", id);
            }
        }
        Ok(())
    }

    fn update_cpu_frequencies(&mut self) {
        for cpu in self.cpus.values_mut() {
            if cpu.online {
                let cpu_path = Path::new("/sys/devices/system/cpu").join(format!("cpu{}", cpu.id));
                let (mhz, max_mhz, min_mhz) = Self::read_cpu_freq(&cpu_path);
                cpu.mhz = mhz;
                cpu.max_mhz = max_mhz;
                cpu.min_mhz = min_mhz;
            }
        }
    }

    fn print_summary(&self) {
        println!("System Topology Summary:");
        println!("Total CPUs: {}", self.cpus.len());
        println!("Total Sockets: {}", self.sockets.len());
        println!("CPU0 Socket: {:?}", self.cpu0_socket);

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

fn online_all_cpus() -> io::Result<()> {
    let cpu_dir = Path::new("/sys/devices/system/cpu");
    for entry in fs::read_dir(cpu_dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(cpu_name) = path.file_name().and_then(|n| n.to_str()) {
            if cpu_name.starts_with("cpu") && cpu_name[3..].parse::<usize>().is_ok() {
                let id: usize = cpu_name[3..].parse().unwrap();
                let online_path = path.join("online");
                if id == 0 {
                    continue;
                } // Skip CPU0 since it's always online
                if online_path.exists() {
                    fs::write(&online_path, "1")?;
                    println!("Onlined CPU {}", id);
                } else {
                    println!("Cannot online CPU {}: 'online' file does not exist", id);
                }
            }
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    println!("Starting CPU manager");
    println!("Uppper load threshold: {}%", args.upper_threshold);
    println!("Lower load threshold: {}%", args.lower_threshold);
    println!("Onlining all CPUs");
    online_all_cpus()?;

    let mut topology = SystemTopology::new()?;
    topology.print_summary();

    let (max_mhz, min_mhz) = topology
        .cpus
        .values()
        .filter(|cpu| cpu.id == 0)
        .map(|cpu| (cpu.max_mhz, cpu.min_mhz))
        .next()
        .unwrap_or((0.0, 0.0));

    println!("CPU0 Max/Min MHz: {:.2}/{:.2}", max_mhz, min_mhz);

    let upper_threshold = (args.upper_threshold as f64 / 100.0) * (max_mhz - min_mhz) + min_mhz;
    let lower_threshold = (args.lower_threshold as f64 / 100.0) * (max_mhz - min_mhz) + min_mhz;

    loop {
        topology.update_cpu_frequencies();

        let total_mhz: f64 = topology
            .cpus
            .values()
            .filter(|cpu| cpu.online)
            .map(|cpu| cpu.mhz)
            .sum();
        let online_count = topology.cpus.values().filter(|cpu| cpu.online).count();
        let avg_mhz = if online_count > 0 {
            total_mhz / online_count as f64
        } else {
            0.0
        };

        println!(
            "Min-Average-Max CPU MHz: {:.2}-{:.2}-{:.2}, Online CPUs: {}",
            min_mhz, avg_mhz, max_mhz, online_count
        );

        if avg_mhz > upper_threshold {
            if let Some(core_to_online) = topology.select_cpu_to_online() {
                println!("High load detected, onlining core {:?}", core_to_online);
                let _ = topology.online_cpu_group(&core_to_online);
            } else {
                println!("Cannot online more CPUs, already at maximum");
            }
        } else if avg_mhz < lower_threshold {
            if let Some(core_to_offline) = topology.select_cpu_to_offline() {
                println!("Low load detected, offlining core {:?}", core_to_offline);
                let _ = topology.offline_cpu_group(&core_to_offline);
            } else {
                println!("Cannot offline more CPUs, already at minimum");
            }
        } else {
            println!("Load is optimal, no action needed");
        }

        thread::sleep(Duration::from_secs(1));
    }
}
