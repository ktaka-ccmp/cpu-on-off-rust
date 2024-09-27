# CPU Core Management Program

## Description

This Rust program is designed to dynamically manage CPU cores based on system load. It aims to optimize power consumption by offlining cores when the system load is low and onlining them when the load increases.

## Features

- Automatically detects and maps the CPU topology of the system
- Continuously monitors CPU frequencies and utilization
- Dynamically offlines and onlines CPU cores based on system load
- Respects CPU0 and always keeps it online
- Handles thread siblings (e.g., hyperthreading) together
- Provides real-time feedback on CPU states and actions taken

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

The program will start managing CPU cores automatically. It will print information about its actions and the current state of the CPUs.

## How it Works

1. On startup, the program reads the CPU topology from the sysfs filesystem.
2. It enters a loop where it continuously:
   - Updates CPU frequency information
   - Calculates average and maximum CPU frequencies
   - Decides whether to online or offline cores based on the current load:
     - If avg_freq > 85% of max_freq: Online all cores
     - If avg_freq < 50% of max_freq: Offline cores, starting with the highest-numbered
   - Applies the decided actions
   - Waits for 1 second before the next iteration

## Configuration

The program uses hardcoded thresholds for determining high and low loads. If you want to adjust these, modify the following lines in `main()`:

```rust
if avg_mhz > max_mhz * 0.85 {  // High load threshold
    // ...
} else if avg_mhz < max_mhz * 0.5 {  // Low load threshold
    // ...
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
