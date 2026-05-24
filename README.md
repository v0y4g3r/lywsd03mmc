# lywsd03mmc

Rust support for reading Xiaomi Mijia `LYWSD03MMC` Bluetooth temperature and humidity sensors.

This project currently provides:

- a library API built around `Scanner`, `Device`, and `Reading`
- a CLI executable target: `read_lywsd03mmc`

The current data read is based on the `LYWSD03MMC` characteristic `EBE0CCC1-7A0A-4B0C-8A1A-6FF2997DA3A6`.

## Requirements

- Rust toolchain compatible with this project
- Bluetooth permissions enabled for your OS
- A machine with BLE support

This crate uses `btleplug`, so behavior depends on the platform BLE backend. On macOS, discovered devices often report the Bluetooth address as `00:00:00:00:00:00`; use the device `id` instead.

## Build

```bash
cargo build
```

Build the CLI target directly:

```bash
cargo build --bin read_lywsd03mmc
```

## CLI

Run the scanner and read current temperature, humidity, battery voltage, and battery percentage:

```bash
cargo run --bin read_lywsd03mmc -- --timeout 15
```

Filter to a specific device `id` or MAC address:

```bash
cargo run --bin read_lywsd03mmc -- --timeout 15 --id 2b56c5ee-1288-a2f1-d82f-ad70b2fd8c69
```

Supported arguments:

- `--timeout SECONDS`: scan timeout in seconds
- `--duration SECONDS`: alias for `--timeout`
- `--id ID_OR_MAC`: match a specific device id or MAC address

## Logging

The library and CLI use `log`, and the CLI initializes `env_logger`.

Examples:

```bash
RUST_LOG=info cargo run --bin read_lywsd03mmc -- --timeout 15
RUST_LOG=debug cargo run --bin read_lywsd03mmc -- --id 2b56c5ee-1288-a2f1-d82f-ad70b2fd8c69
```

## Library usage

Scan for matching devices:

```rust
use std::time::Duration;

use lywsd03mmc::Scanner;

# async fn demo() -> Result<(), btleplug::Error> {
let devices = Scanner::new()
    .with_timeout(Duration::from_secs(15))
    .scan()
    .await?;

for device in devices {
    let reading = device.read_data().await?;
    println!(
        "{} temp={:.2}C humidity={} battery={:.3}V",
        device,
        reading.temperature_celsius,
        reading.humidity_percent,
        reading.battery_voltage
    );
}
# Ok(())
# }
```

Filter by device id or MAC:

```rust
use std::time::Duration;

use lywsd03mmc::Scanner;

# async fn demo() -> Result<(), btleplug::Error> {
let devices = Scanner::new()
    .with_timeout(Duration::from_secs(15))
    .with_id_filter("2b56c5ee-1288-a2f1-d82f-ad70b2fd8c69")
    .scan()
    .await?;
# Ok(())
# }
```

## Testing

The crate includes:

- parser tests that do not require hardware
- an ignored hardware integration test for live BLE reads

Run default tests:

```bash
cargo test
```

Run the hardware test with output:

```bash
cargo test scanner_reads_lywsd03mmc_data -- --ignored --nocapture
```
