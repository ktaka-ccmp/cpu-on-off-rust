# CPU Core Management Program Using C0 State Residency

## Description

This Rust program dynamically manages CPU cores based on system load, optimizing power consumption by offlining cores during low load periods and onlining them when the load increases. It uses the percentage of time CPUs spend in the C0 state (active state) as a measure of system activity.

## Features

- Detects available C-states for each CPU
- Calculates C0 state percentage to measure CPU utilization
- Continuously monitors system load based on average C0 state percentage
- Dynamically offlines and onlines CPU cores based on the calculated load
- Respects CPU0 and always keeps it online
- Handles thread siblings (e.g., hyperthreading) together
- Provides real-time feedback on CPU states and actions taken
- Allows customization of high and low load thresholds via command-line arguments
- Supports graceful shutdown and restart via signal handling (SIGINT, SIGTERM, SIGHUP)

## Requirements

- Rust programming language (latest stable version recommended)
- Linux operating system with sysfs CPU management capabilities
- Root privileges to modify CPU states

## Installation

1. Clone this repository:
   ```
   git clone https://github.com/ktaka-ccmp/cpu-on-off-rust.git
   cd cpu-on-off-rust
   ```

2. Build the project:
   ```
   cargo build --release
   ```

## Usage

Run the program with root privileges:

```
sudo ./target/release/cpu-on-off-rust [OPTIONS]
```

Options:
- `-u, --upper-threshold <VALUE>`: Set the upper C0 percentage threshold (default: 85)
- `-l, --lower-threshold <VALUE>`: Set the lower C0 percentage threshold (default: 50)

Example:
```
sudo ./target/release/cpu-on-off-rust -u 80 -l 40
```

## Signal Handling

The program supports the following signals:

- SIGINT (Ctrl+C) and SIGTERM: Gracefully shut down the program, onlining all CPUs before exiting.
- SIGHUP: Restart the CPU management process. This will:
  1. Online all CPUs
  2. Reset the CPU topology information
  3. Restart the CPU management loop with the same threshold settings

This is useful for:

- Resetting the CPU states to a known configuration.
- Temporarily enabling all CPUs for a short period, without stopping the management program. After the SIGHUP, the program will resume the normal operation.

To send a SIGHUP signal to the running program:

1. Find the process ID (PID) of the running program:
   ```
   ps aux | grep cpu-on-off-rust
   ```

2. Send the SIGHUP signal:
   ```
   kill -SIGHUP <PID>
   ```

## How it Works

1. The program detects CPUs and their available C-states.
2. It then enters a loop where it:
   - Calculates the C0 state percentage for each CPU.
   - Computes the average C0 percentage across all online CPUs.
   - Based on this average and the set thresholds, it may online or offline cores.
   - Waits for a short interval before the next iteration.

3. The C0 percentage is calculated as:
   ```
   C0_percentage = 100 * (1 - (idle_time / total_time))
   ```
   Where `idle_time` is the sum of time spent in all C-states except C0, and `total_time` is the total elapsed time.

## Caution

This program modifies CPU states and can affect system performance. Use with care, especially on production systems. Always test thoroughly in a safe environment before deploying.

## Contributing

Contributions, issues, and feature requests are welcome. Feel free to check [issues page](https://github.com/ktaka-ccmp/cpu-on-off-rust/issues) if you want to contribute.

## License

[MIT](https://choosealicense.com/licenses/mit/)

## Author

[Kimitoshi Takahashi](https://github.com/ktaka-ccmp)

## Acknowledgments

- Inspired by various CPU management techniques
- Thanks to the Rust community for their libraries and documentation
- Uses the `clap` crate for parsing command-line arguments
- Uses the `tokio` runtime for asynchronous operations and signal handling
