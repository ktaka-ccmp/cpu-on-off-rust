# CPU Core Management Program

## Description

This Rust program dynamically manages CPU cores based on system load, optimizing power consumption by offlining cores during low load periods and onlining them when the load increases. It's designed to work with various x86_64 systems and adapts to different CPU configurations.

## Features

- Automatically detects and maps the CPU topology of the system
- Continuously monitors CPU frequencies and utilization
- Dynamically offlines and onlines CPU cores based on system load
- Respects CPU0 and always keeps it online
- Handles thread siblings (e.g., hyperthreading) together
- Provides real-time feedback on CPU states and actions taken
- Onlines all available CPUs at startup for consistent initial state
- Uses adaptive thresholds based on min and max CPU frequencies

## Requirements

- Rust programming language (latest stable version recommended)
- Linux operating system with sysfs CPU management capabilities
- Root privileges to modify CPU states

## Installation

1. Clone this repository:
   ```
   git clone https://github.com/yourusername/cpu-clk-rust.git
   cd cpu-clk-rust
   ```

2. Build the project:
   ```
   cargo build --release
   ```

## Usage

Run the program with root privileges:

```
sudo ./target/release/cpu-clk-rust
```

The program will start by onlining all available CPUs, then begin managing CPU cores automatically. It will print information about its actions and the current state of the CPUs.

## How it Works

1. On startup, the program onlines all available CPUs.
2. It then reads the CPU topology from the sysfs filesystem.
3. The program enters a loop where it continuously:
   - Updates CPU frequency information
   - Calculates average, minimum, and maximum CPU frequencies
   - Decides whether to online or offline cores based on the current load:
     - If avg_mhz - min_mhz > 0.85 * (max_mhz - min_mhz): Online one core
     - If avg_mhz - min_mhz < 0.5 * (max_mhz - min_mhz): Offline one core
   - Applies the decided actions
   - Waits for 1 second before the next iteration

## Configuration

The program uses adaptive thresholds for determining high and low loads based on the minimum and maximum CPU frequencies. If you want to adjust these, modify the following lines in `main()`:

```rust
if avg_mhz > max_mhz * 0.85 + min_mhz * 0.15 {  // High load threshold
            // avg_mhz - min_mhz > 0.85 * (max_mhz - min_mhz)
} else if avg_mhz < max_mhz * 0.5 + min_mhz * 0.5 {  // Low load threshold
            // avg_mhz - min_mhz < 0.5 * (max_mhz - min_mhz)
}
```

## Caution

This program modifies CPU states and can affect system performance. Use with care, especially on production systems. Always test thoroughly in a safe environment before deploying.

## Contributing

Contributions, issues, and feature requests are welcome. Feel free to check [issues page](https://github.com/yourusername/cpu-clk-rust/issues) if you want to contribute.

## License

[MIT](https://choosealicense.com/licenses/mit/)

## Author

[Kimitoshi Takahashi]

## Acknowledgments

- Inspired by CPU management techniques in modern operating systems
- Thanks to the Rust community for providing excellent documentation and libraries
