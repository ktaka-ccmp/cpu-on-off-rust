# Dynamic C-state CPU Core Management Program

## Description

This Rust program dynamically manages CPU cores based on system load, optimizing power consumption by offlining cores during low load periods and onlining them when the load increases. It's designed to work with various x86_64 systems, adapts to different CPU configurations, and dynamically detects available C-states for each CPU.

## Features

- Automatically detects and maps the CPU topology of the system
- Dynamically identifies available C-states for each CPU
- Continuously monitors CPU utilization based on C-state residency
- Dynamically offlines and onlines CPU cores based on system load
- Respects CPU0 and always keeps it online
- Handles thread siblings (e.g., hyperthreading) together
- Provides real-time feedback on CPU states and actions taken
- Onlines all available CPUs at startup for consistent initial state
- Uses adaptive thresholds based on C0 (active state) residency
- Allows customization of high and low load thresholds via command-line arguments

## Requirements

- Rust programming language (latest stable version recommended)
- Linux operating system with sysfs CPU management capabilities
- Root privileges to modify CPU states

## Installation

1. Clone this repository:
   ```
   git clone https://github.com/ktaka-ccmp/cpu-clk-rust.git
   cd cpu-clk-rust
   ```

2. Build the project:
   ```
   cargo build --release
   ```

## Usage

Run the program with root privileges:

```
sudo ./target/release/cpu-clk-rust [OPTIONS]
```

Options:
- `-u, --upper-threshold <VALUE>`: Set the upper load threshold percentage (default: 85)
- `-l, --lower-threshold <VALUE>`: Set the lower load threshold percentage (default: 50)

Example:
```
sudo ./target/release/cpu-clk-rust -u 80 -l 40
```

The program will start by onlining all available CPUs, then begin managing CPU cores automatically based on the specified thresholds. It will print information about its actions and the current state of the CPUs.

## How it Works

1. On startup, the program parses command-line arguments for custom thresholds.
2. It then onlines all available CPUs and reads the CPU topology from the sysfs filesystem.
3. The program detects available C-states for each CPU.
4. It enters a loop where it continuously:
   - Updates C-state residency information for each CPU
   - Calculates the average C0 (active state) percentage across all online CPUs
   - Decides whether to online or offline cores based on the current load and specified thresholds
   - Applies the decided actions
   - Waits for 1 second before the next iteration

## Configuration

The upper and lower load thresholds can be configured via command-line arguments. If not specified, the program uses default values of 85% for the upper threshold and 50% for the lower threshold.

## Caution

This program modifies CPU states and can affect system performance. Use with care, especially on production systems. Always test thoroughly in a safe environment before deploying.

## Contributing

Contributions, issues, and feature requests are welcome. Feel free to check [issues page](https://github.com/ktaka-ccmp/cpu-clk-rust/issues) if you want to contribute.

## License

[MIT](https://choosealicense.com/licenses/mit/)

## Author

[Kimitoshi Takahashi]

## Acknowledgments

- Inspired by CPU management techniques in modern operating systems
- Thanks to the Rust community for providing excellent documentation and libraries
- Uses the `clap` crate for parsing command-line arguments
