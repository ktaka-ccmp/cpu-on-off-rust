//! This program manages CPU states (online/offline) based on system load thresholds.
//! It uses the `clap` crate for command-line argument parsing and `tokio` for asynchronous operations.
//!
//! # Command-line Arguments
//! - `-u, --upper-threshold`: Upper load threshold percentage (default: 85)
//! - `-l, --lower-threshold`: Lower load threshold percentage (default: 50)
//!
//! # Structures
//! - `Args`: Holds the command-line arguments.
//! - `CpuInfo`: Represents information about a single CPU.
//! - `SystemTopology`: Represents the system's CPU topology and provides methods to manage CPU states.
//!
//! # Methods
//! - `SystemTopology::new()`: Initializes the system topology by reading CPU information.
//! - `SystemTopology::process_cpu()`: Processes individual CPU entries.
//! - `SystemTopology::read_thread_siblings()`: Reads thread siblings for a CPU.
//! - `SystemTopology::is_cpu_online()`: Checks if a CPU is online.
//! - `SystemTopology::get_idle_states()`: Retrieves idle states for a CPU.
//! - `SystemTopology::update_c0_percentages()`: Updates the C0 state percentages for all CPUs.
//! - `SystemTopology::update_c0_single()`: Updates the C0 state percentage for a single CPU.
//! - `SystemTopology::select_cpu_to_offline()`: Selects CPUs to offline based on load.
//! - `SystemTopology::select_cpu_to_online()`: Selects CPUs to online based on load.
//! - `SystemTopology::offline_cpu_group()`: Offlines a group of CPUs.
//! - `SystemTopology::online_cpu_group()`: Onlines a group of CPUs.
//! - `SystemTopology::print_summary()`: Prints a summary of the system topology.
//!
//! # Functions
//! - `online_all_cpus()`: Onlines all CPUs.
//! - `signal_handler()`: Handles UNIX signals (SIGINT, SIGTERM, SIGHUP).
//! - `cpu_manager()`: Manages CPU states based on load thresholds and signals.
//!
//! # Main Function
//! - Initializes the program, parses command-line arguments, onlines all CPUs, initializes the system topology, and starts the CPU manager and signal handler tasks.
use clap::Parser;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;

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

    /// Asynchronously processes a CPU entry, extracting relevant information and updating the provided data structures.
    ///
    /// This function performs the following steps:
    /// 1. Retrieves the path of the CPU entry.
    /// 2. Extracts the CPU name from the path and checks if it starts with "cpu" and is followed by a valid number.
    /// 3. Parses the CPU ID from the CPU name.
    /// 4. Reads the core ID and socket ID from the respective files in the CPU's topology directory.
    /// 5. Reads the thread siblings, online status, and idle states of the CPU.
    /// 6. If the CPU ID is 0, updates the `cpu0_socket` with the socket ID.
    /// 7. Creates a `CpuInfo` struct with the extracted information and inserts it into the `cpus` HashMap.
    /// 8. Updates the `sockets` HashMap with the CPU ID if the socket ID is present.
    ///
    /// # Arguments
    /// * `entry` - The directory entry representing the CPU.
    /// * `cpu0_socket` - A mutable reference to an Option containing the socket ID of CPU0.
    /// * `cpus` - A mutable reference to a HashMap storing information about all CPUs.
    /// * `sockets` - A mutable reference to a HashMap storing the CPUs associated with each socket.
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

    /// Asynchronously updates the C0 state percentages (non-idle time) for all online CPUs.
    ///
    /// This function performs the following steps:
    /// 1. Records the current time as `now`.
    /// 2. Calculates the actual interval since the last update by subtracting `self.last_update` from `now`.
    /// 3. Updates `self.last_update` to the current time.
    /// 4. Iterates over all CPUs in the `self.cpus` HashMap.
    /// 5. For each online CPU, calls the `update_c0_single` method to update its C0 percentage based on the actual interval.
    ///
    /// # Returns
    /// * `io::Result<()>` - Returns an `Ok(())` if successful, or an `io::Error` if an error occurs.
    async fn update_c0_percentages(&mut self) -> io::Result<()> {
        let now = Instant::now();
        let actual_interval = now.duration_since(self.last_update);
        self.last_update = now;

        for cpu in self.cpus.values_mut() {
            if cpu.online {
                Self::update_c0_single(cpu, actual_interval).await?;
            }
        }
        Ok(())
    }

    /// Asynchronously updates the C0 state percentage (non-idle time) for a single CPU based on the actual interval.
    ///
    /// This function performs the following steps:
    /// 1. Constructs the path to the CPU's cpuidle directory.
    /// 2. Initializes the total idle time to zero.
    /// 3. Iterates over the CPU's idle states and reads the idle time for each state from the respective file.
    /// 4. Sums up the idle times to get the total idle time.
    /// 5. Calculates the delta of idle time since the last update.
    /// 6. Updates the CPU's last total idle time with the current total idle time.
    /// 7. Calculates the C0 percentage as the proportion of non-idle time over the actual interval.
    /// 8. Clamps the C0 percentage to the range [0.0, 100.0].
    ///
    /// # Arguments
    /// * `cpu` - A mutable reference to the `CpuInfo` struct representing the CPU.
    /// * `actual_interval` - The duration since the last update.
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

    /// Selects a group of CPUs to be offlined based on their current state and topology.
    ///
    /// This function performs the following steps:
    /// 1. Filters the CPUs to get a list of online CPUs excluding CPU0.
    /// 2. If there is only one or no online CPU (excluding CPU0), returns `None` to avoid offlining.
    /// 3. Finds the CPU with the highest ID among the online CPUs.
    /// 4. Collects the thread siblings of the selected CPU that are also online.
    /// 5. Returns the list of online thread siblings to be offlined.
    ///
    /// # Returns
    /// * `Option<Vec<usize>>` - A vector of CPU IDs to be offlined, or `None` if no CPUs can be offlined.
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

    /// Selects a group of CPUs to be onlined based on their current state and topology.
    ///
    /// This function performs the following steps:
    /// 1. Filters the CPUs to get a list of offline CPUs excluding CPU0.
    /// 2. If all CPUs are already online, returns `None` to avoid onlining.
    /// 3. Finds the CPU with the lowest ID among the offline CPUs.
    /// 4. Collects the thread siblings of the selected CPU that are also offline.
    /// 5. Returns the list of offline thread siblings to be onlined.
    ///
    /// # Returns
    /// * `Option<Vec<usize>>` - A vector of CPU IDs to be onlined, or `None` if no CPUs can be onlined.
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

/// Handles UNIX signals (SIGINT, SIGTERM, SIGHUP) asynchronously.
///
/// This function performs the following steps:
/// 1. Sets up signal handlers for SIGINT, SIGTERM, and SIGHUP using `tokio::signal::unix::signal`.
/// 2. Initializes a flag to `false`.
/// 3. Enters an infinite loop where it waits for any of the signals to be received using `tokio::select!`.
/// 4. If SIGINT is received, it prints a message, calls `online_all_cpus` to online all CPUs, and breaks the loop.
/// 5. If SIGTERM is received, it prints a message, calls `online_all_cpus` to online all CPUs, and breaks the loop.
/// 6. If SIGHUP is received, it toggles the flag, sends the flag's value through the provided `watch::Sender`, and continues the loop.
/// 7. After breaking the loop, it prints a shutdown message and performs any necessary cleanup.
///
/// # Arguments
/// * `tx` - A `watch::Sender<bool>` used to send the flag's value when SIGHUP is received.
async fn signal_handler(tx: watch::Sender<bool>) {
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    let mut sighup = signal(SignalKind::hangup()).unwrap();

    let mut flag = false;

    loop {
        tokio::select! {
            _ = sigint.recv() => {
                println!("Received SIGINT");
                online_all_cpus().await.unwrap();
                break;
            }
            _ = sigterm.recv() => {
                println!("Received SIGTERM");
                online_all_cpus().await.unwrap();
                break;
            }
            _ = sighup.recv() => {
                println!("Received SIGHUP");
                flag = !flag;
                tx.send(flag).unwrap();
            }
        }
    }

    // Cleanup code here
    println!("Shutting down...");
}

/// Manages CPU states based on load thresholds and signals asynchronously.
///
/// This function performs the following steps:
/// 1. Enters an infinite loop to continuously monitor and manage CPU states.
/// 2. Checks if a HUP signal has been received using the `rx` receiver:
///    - If a HUP signal is received, it prints a message, calls `online_all_cpus` to online all CPUs,
///      and waits until the HUP signal is cleared.
/// 3. Calls `update_c0_percentages` to update the C0 state percentages for all CPUs.
/// 4. Calculates the total and average C0 state percentage for all online CPUs.
/// 5. Prints the average C0 state percentage and the number of online CPUs.
/// 6. Compares the average C0 state percentage with the upper and lower thresholds:
///    - If the average C0 state percentage is above the upper threshold, it attempts to online more CPUs.
///    - If the average C0 state percentage is below the lower threshold, it attempts to offline some CPUs.
///    - If the average C0 state percentage is within the thresholds, it prints a message indicating no action is needed.
/// 7. Sleeps for 1 second before repeating the loop.
///
/// # Arguments
/// * `args` - A reference to the `Args` struct containing the command-line arguments.
/// * `topology` - A mutable reference to the `SystemTopology` struct representing the system's CPU topology.
/// * `rx` - A `watch::Receiver<bool>` used to receive signals indicating a HUP signal.
async fn cpu_manager(
    args: &Args,
    topology: &mut SystemTopology,
    rx: watch::Receiver<bool>,
) -> io::Result<()> {
    loop {
        if *rx.borrow() {
            println!("Received HUP signal...");
            online_all_cpus().await?;
            println!("Send Hup signal to restart...");
            loop {
                if !*rx.borrow() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
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

/// The main entry point for the CPU manager program.
///
/// This function performs the following steps:
/// 1. Parses command-line arguments using the `clap` crate.
/// 2. Prints the starting message and the upper and lower load thresholds.
/// 3. Calls `online_all_cpus` to ensure all CPUs are online at the start.
/// 4. Initializes the system topology by creating a new `SystemTopology` instance and prints a summary of the system topology.
/// 5. Creates a `watch` channel for signal handling.
/// 6. Spawns two asynchronous tasks:
///    - `main_task`: Runs the `cpu_manager` function to manage CPU states based on load thresholds.
///    - `signal_task`: Runs the `signal_handler` function to handle UNIX signals.
/// 7. Uses `tokio::select!` to wait for either the `main_task` or `signal_task` to complete.
/// 8. Prints a message indicating which task completed and returns `Ok(())`.
///
/// # Returns
/// * `Result<(), Box<dyn std::error::Error>>` - Returns `Ok(())` if successful, or an error if an error occurs.
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

    let (tx, rx) = watch::channel(false);

    let main_task = tokio::spawn(async move { cpu_manager(&args, &mut topology, rx).await });

    let signal_task = tokio::spawn(signal_handler(tx));

    tokio::select! {
        _ = main_task => println!("Main task completed"),
        _ = signal_task => println!("Received shutdown signal"),
    }

    Ok(())
}
