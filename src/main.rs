use clap::Parser;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::task::JoinSet;

static CPU_DIR: &str = "/sys/devices/system/cpu";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Upper load threshold percentage (default: 85)
    #[arg(short = 'u', long, default_value_t = 85)]
    upper_threshold: u8,

    /// Lower load threshold percentage (default: 50)
    #[arg(short = 'l', long, default_value_t = 50)]
    lower_threshold: u8,
}

#[allow(dead_code)]
#[derive(Clone)]
struct CpuInfo {
    id: usize,
    core_id: Option<usize>,
    socket_id: Option<usize>,
    thread_siblings: Vec<usize>,
    c0_percentage: f64,
    online: bool,
    last_total_idle_time: u64,
    idle_states: Vec<String>,
}

struct SystemTopology {
    cpus: HashMap<usize, CpuInfo>,
    sockets: HashMap<usize, Vec<usize>>,
    cpu0_socket: Option<usize>,
    last_update: Instant,
}

impl SystemTopology {
    async fn new() -> io::Result<Self> {
        let mut cpus = HashMap::new();
        let mut sockets = HashMap::new();
        let mut cpu0_socket = None;

        let cpu_dir = Path::new(CPU_DIR);
        println!("Reading CPU information from: {:?}", cpu_dir);

        let mut read_dir = fs::read_dir(cpu_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            Self::process_cpu(entry, &mut cpu0_socket, &mut cpus, &mut sockets).await;
        }

        println!("Finished reading CPU information");
        println!("Found {} CPUs across {} sockets", cpus.len(), sockets.len());

        Ok(SystemTopology {
            cpus,
            sockets,
            cpu0_socket,
            last_update: Instant::now(),
        })
    }

    async fn process_cpu(
        entry: fs::DirEntry,
        cpu0_socket: &mut Option<usize>,
        cpus: &mut HashMap<usize, CpuInfo>,
        sockets: &mut HashMap<usize, Vec<usize>>,
    ) {
        let path = entry.path();
        if let Some(cpu_name) = path.file_name().and_then(|n| n.to_str()) {
            if cpu_name.starts_with("cpu") && cpu_name[3..].parse::<usize>().is_ok() {
                let id = cpu_name[3..].parse().unwrap();
                println!("Processing CPU {}", id);

                let core_id = fs::read_to_string(path.join("topology/core_id"))
                    .await
                    .ok()
                    .and_then(|s| s.trim().parse().ok());

                let socket_id = fs::read_to_string(path.join("topology/physical_package_id"))
                    .await
                    .ok()
                    .and_then(|s| s.trim().parse().ok());

                let thread_siblings = Self::read_thread_siblings(&path).await;

                let online = Self::is_cpu_online(&path).await;

                let idle_states = Self::get_idle_states(&path).await;

                if id == 0 {
                    *cpu0_socket = socket_id;
                }

                let cpu_info = CpuInfo {
                    id,
                    core_id,
                    socket_id,
                    thread_siblings,
                    c0_percentage: 0.0,
                    online,
                    last_total_idle_time: 0,
                    idle_states,
                };
                cpus.insert(id, cpu_info);

                if let Some(socket_id) = socket_id {
                    sockets.entry(socket_id).or_default().push(id);
                    // sockets.entry(socket_id).or_insert_with(Vec::new).push(id);
                }
            }
        }
    }

    async fn read_thread_siblings(cpu_path: &Path) -> Vec<usize> {
        let siblings_path = cpu_path.join("topology/thread_siblings_list");
        fs::read_to_string(&siblings_path)
            .await
            .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_default()
    }

    async fn is_cpu_online(cpu_path: &Path) -> bool {
        let online_path = cpu_path.join("online");
        if online_path.exists() {
            fs::read_to_string(online_path)
                .await
                .map(|content| content.trim() == "1")
                .unwrap_or(false)
        } else {
            true // CPU is always online if the 'online' file doesn't exist
        }
    }

    async fn get_idle_states(cpu_path: &Path) -> Vec<String> {
        let cpuidle_path = cpu_path.join("cpuidle");
        let mut states = Vec::new();
        if let Ok(mut entries) = fs::read_dir(cpuidle_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("state") {
                        states.push(name.to_string());
                    }
                }
            }
        }
        states.sort();
        states
    }

    async fn update_c0_percentages(&mut self) -> io::Result<()> {
        let now = Instant::now();
        let actual_interval = now.duration_since(self.last_update);
        self.last_update = now;

        let mut set = JoinSet::new();

        for cpu in self.cpus.values().cloned().collect::<Vec<CpuInfo>>() {
            if cpu.online {
                set.spawn(async move {
                    Self::calc_c0_single(&cpu, actual_interval)
                        .await
                        .map(|res| (cpu.id, res))
                });
            }
        }

        while let Some(res) = set.join_next().await {
            let (cpu_id, result) = res??;
            if let Some(cpu) = self.cpus.get_mut(&cpu_id) {
                cpu.c0_percentage = result.1;
                cpu.last_total_idle_time = result.2;
            }
        }

        Ok(())
    }

    async fn calc_c0_single(
        cpu: &CpuInfo,
        actual_interval: Duration,
    ) -> io::Result<(usize, f64, u64)> {
        let cpuidle_path = Path::new(CPU_DIR)
            .join(format!("cpu{}", cpu.id))
            .join("cpuidle");
        let mut total_idle_time = 0;
        for state in &cpu.idle_states {
            let state_path = cpuidle_path.join(state);
            if state_path.exists() {
                let time = fs::read_to_string(state_path.join("time"))
                    .await?
                    .trim()
                    .parse::<u64>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                total_idle_time += time;
            }
        }
        let idle_time_delta = total_idle_time.saturating_sub(cpu.last_total_idle_time);
        let c0_percentage = 100.0
            * (1.0 - (idle_time_delta as f64 / actual_interval.as_micros() as f64))
                .clamp(0.0, 100.0);
        Ok((cpu.id, c0_percentage, total_idle_time))
    }

    async fn update_c0_single(
        cpu: &mut CpuInfo,
        actual_interval: Duration,
    ) -> Result<(), io::Error> {
        let cpuidle_path = Path::new(CPU_DIR)
            .join(format!("cpu{}", cpu.id))
            .join("cpuidle");
        let mut total_idle_time = 0;
        for state in &cpu.idle_states {
            let state_path = cpuidle_path.join(state);
            if state_path.exists() {
                let time = fs::read_to_string(state_path.join("time"))
                    .await?
                    .trim()
                    .parse::<u64>()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                total_idle_time += time;
            }
        }
        let idle_time_delta = total_idle_time.saturating_sub(cpu.last_total_idle_time);
        cpu.last_total_idle_time = total_idle_time;
        cpu.c0_percentage =
            100.0 * (1.0 - (idle_time_delta as f64 / actual_interval.as_micros() as f64));
        cpu.c0_percentage = cpu.c0_percentage.clamp(0.0, 100.0);
        Ok(())
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

    async fn offline_cpu_group(&mut self, cpu_ids: &[usize]) -> io::Result<()> {
        for &id in cpu_ids {
            if id == 0 {
                continue;
            } // Never offline CPU0
            let path = Path::new(CPU_DIR).join(format!("cpu{}", id)).join("online");
            if path.exists() {
                fs::write(&path, "0").await?;
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

    async fn online_cpu_group(&mut self, cpu_ids: &[usize]) -> io::Result<()> {
        for &id in cpu_ids {
            if id == 0 {
                continue;
            } // CPU0 is always online
            let path = Path::new(CPU_DIR).join(format!("cpu{}", id)).join("online");
            if path.exists() {
                fs::write(&path, "1").await?;
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

        // Print idle states for CPU0 as an example
        if let Some(cpu0) = self.cpus.get(&0) {
            println!("Idle states for CPU0: {:?}", cpu0.idle_states);
        }
    }
}

async fn online_all_cpus() -> io::Result<()> {
    let cpu_dir = Path::new(CPU_DIR);
    let mut read_dir = fs::read_dir(cpu_dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if let Some(cpu_name) = path.file_name().and_then(|n| n.to_str()) {
            if cpu_name.starts_with("cpu") && cpu_name[3..].parse::<usize>().is_ok() {
                let id: usize = cpu_name[3..].parse().unwrap();
                let online_path = path.join("online");
                if id == 0 {
                    continue;
                } // Skip CPU0 since it's always online
                if online_path.exists() {
                    fs::write(&online_path, "1").await?;
                    println!("Onlined CPU {}", id);
                } else {
                    println!("Cannot online CPU {}: 'online' file does not exist", id);
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("Starting CPU manager");
    println!("Upper load threshold: {}%", args.upper_threshold);
    println!("Lower load threshold: {}%", args.lower_threshold);
    println!("Onlining all CPUs");
    online_all_cpus().await?;

    let mut topology = SystemTopology::new().await?;
    topology.print_summary();

    let (tx, rx) = mpsc::channel();

    ctrlc::set_handler(move || tx.send("hello").expect("Could not send signal on channel."))?;

    loop {
        if let Ok(msg) = rx.try_recv() {
            // if rx.try_recv().is_ok() {
            println!("Received shutdown signal. Cleaning up...{}", msg);
            online_all_cpus().await?;
            println!("All CPUs onlined. Shutting down.");
            return Ok(());
        }

        topology.update_c0_percentages().await?;

        let total_c0: f64 = topology
            .cpus
            .values()
            .filter(|cpu| cpu.online)
            .map(|cpu| cpu.c0_percentage)
            .sum();
        let online_count = topology.cpus.values().filter(|cpu| cpu.online).count();
        let avg_c0 = if online_count > 0 {
            total_c0 / online_count as f64
        } else {
            0.0
        };

        println!(
            "Average C0 state percentage: {:.2}%, Online CPUs: {}",
            avg_c0, online_count
        );

        if avg_c0 > args.upper_threshold as f64 {
            if let Some(core_to_online) = topology.select_cpu_to_online() {
                println!("High load detected, onlining core {:?}", core_to_online);
                let _ = topology.online_cpu_group(&core_to_online).await;
            } else {
                println!("Cannot online more CPUs, already at maximum");
            }
        } else if avg_c0 < args.lower_threshold as f64 {
            if let Some(core_to_offline) = topology.select_cpu_to_offline() {
                println!("Low load detected, offlining core {:?}", core_to_offline);
                let _ = topology.offline_cpu_group(&core_to_offline).await;
            } else {
                println!("Cannot offline more CPUs, already at minimum");
            }
        } else {
            println!("Load is optimal, no action needed");
        }

        let _ = tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
