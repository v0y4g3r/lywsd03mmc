use std::ffi::OsString;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use log::{error, info, warn};
use lywsd03mmc::Scanner;
use serde_json::json;

#[derive(Debug, Parser)]
#[command(name = "read_lywsd03mmc")]
#[command(about = "Scan for LYWSD03MMC devices and read temperature, humidity, and battery data")]
struct Args {
    #[arg(long, value_name = "SECONDS", conflicts_with = "duration")]
    timeout: Option<u64>,

    #[arg(long, value_name = "SECONDS", conflicts_with = "timeout")]
    duration: Option<u64>,

    #[arg(long, value_name = "ID_OR_MAC")]
    id: Option<String>,

    #[arg(long)]
    json: bool,
}

impl Args {
    fn timeout_duration(&self) -> Option<Duration> {
        self.timeout.or(self.duration).map(Duration::from_secs)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    env_logger::init();
    let args = Args::parse_from(std::env::args_os().collect::<Vec<OsString>>());

    let mut scanner = Scanner::new();
    if let Some(timeout) = args.timeout_duration() {
        scanner = scanner.with_timeout(timeout);
    }
    if let Some(id_filter) = args.id {
        scanner = scanner.with_id_filter(id_filter);
    }

    let devices = match scanner.scan().await {
        Ok(devices) => devices,
        Err(error) => {
            error!("scan failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    if devices.is_empty() {
        warn!("no LYWSD03MMC devices found");
        return ExitCode::FAILURE;
    }

    info!("reading {} device(s)", devices.len());
    let mut json_readings = Vec::new();
    for device in devices {
        match device.read_data().await {
            Ok(reading) => {
                if args.json {
                    json_readings.push(json!({
                        "id": device.id,
                        "address": device.address,
                        "address_type": device.address_type,
                        "name": device.name,
                        "rssi": device.rssi,
                        "tx_power_level": device.tx_power_level,
                        "services": device.services,
                        "manufacturer_data": device.manufacturer_data,
                        "service_data": device.service_data,
                        "class": device.class,
                        "temperature_celsius": reading.temperature_celsius,
                        "humidity_percent": reading.humidity_percent,
                        "battery_voltage": reading.battery_voltage,
                        "battery_percent": reading.battery_percent,
                    }));
                } else {
                    println!(
                        "{} temperature_celsius={:.2} humidity_percent={} battery_voltage={:.3} battery_percent={}",
                        device,
                        reading.temperature_celsius,
                        reading.humidity_percent,
                        reading.battery_voltage,
                        reading.battery_percent,
                    );
                }
            }
            Err(error) => {
                error!("read failed for {}: {error}", device.id);
            }
        }
    }

    if args.json {
        match serde_json::to_string(&json_readings) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                error!("failed to encode JSON output: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}
