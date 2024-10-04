# CPU Core Management Program Using C0 State Residency

## Description

This Rust program dynamically manages CPU cores based on system load, aiming to optimize power consumption by offlining cores during low load periods and onlining them when the load increases. It uses the percentage of time CPUs spend in the C0 state (active state) as one way to estimate system activity.

## Key Concept: C0 State and CPU Activity

The program uses the C0 state percentage as an indicator of system activity. Here's a brief explanation:

- C0 State: This is the CPU's active state where it's executing instructions.
- Other C-states (C1, C2, C3, etc.): These are various sleep states where the CPU consumes less power.
- C0 State Percentage: The proportion of time a CPU spends in the C0 state.
  - Higher C0 percentage generally indicates more CPU activity
  - Lower C0 percentage generally indicates less CPU activity

Using C0 state percentage is one approach to estimating CPU utilization. It may offer insights into CPU activity that complement other metrics like clock speed or traditional load averages.

## Features

- Detects available C-states for each CPU
- Calculates C0 state percentage
- Monitors system activity based on average C0 state percentage
- Dynamically offlines and onlines CPU cores based on the calculated metric
- Keeps CPU0 always online
- Handles thread siblings (e.g., hyperthreading) together
- Provides feedback on CPU states and actions taken
- Allows customization of thresholds via command-line arguments

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

[Your Name]

## Acknowledgments

- Inspired by various CPU management techniques
- Thanks to the Rust community for their libraries and documentation
- Uses the `clap` crate for parsing command-line arguments
