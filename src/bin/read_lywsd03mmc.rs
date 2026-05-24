use std::env;
use std::process::ExitCode;
use std::time::Duration;

use lywsd03mmc::Scanner;
use log::{error, info, warn};

fn print_usage(program: &str) {
    eprintln!(
        "Usage: {program} [--timeout SECONDS] [--duration SECONDS] [--id ID_OR_MAC]\n\
         \n\
         Options:\n\
           --timeout SECONDS   Scan timeout in seconds\n\
           --duration SECONDS  Alias for --timeout\n\
           --id ID_OR_MAC      Match a specific device id or MAC address\n\
           -h, --help          Show this help text"
    );
}

fn parse_args() -> Result<(Option<Duration>, Option<String>), String> {
    let mut timeout = None;
    let mut id_filter = None;
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "read_lywsd03mmc".to_string());
    let mut rest = args;

    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage(&program);
                std::process::exit(0);
            }
            "--timeout" | "--duration" => {
                let value = rest
                    .next()
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                let seconds: u64 = value
                    .parse()
                    .map_err(|_| format!("invalid integer value for {arg}: {value}"))?;
                timeout = Some(Duration::from_secs(seconds));
            }
            "--id" => {
                let value = rest
                    .next()
                    .ok_or_else(|| "missing value for --id".to_string())?;
                id_filter = Some(value);
            }
            _ => {
                return Err(format!("unknown argument: {arg}"));
            }
        }
    }

    Ok((timeout, id_filter))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    env_logger::init();
    let program = env::args()
        .next()
        .unwrap_or_else(|| "read_lywsd03mmc".to_string());

    let (timeout, id_filter) = match parse_args() {
        Ok(values) => values,
        Err(error) => {
            eprintln!("{error}");
            print_usage(&program);
            return ExitCode::FAILURE;
        }
    };

    let mut scanner = Scanner::new();
    if let Some(timeout) = timeout {
        scanner = scanner.with_timeout(timeout);
    }
    if let Some(id_filter) = id_filter {
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
    for device in devices {
        match device.read_data().await {
            Ok(reading) => {
                println!(
                    "{} temperature_celsius={:.2} humidity_percent={} battery_voltage={:.3} battery_percent={}",
                    device,
                    reading.temperature_celsius,
                    reading.humidity_percent,
                    reading.battery_voltage,
                    reading.battery_percent,
                );
            }
            Err(error) => {
                error!("read failed for {}: {error}", device.id);
            }
        }
    }

    ExitCode::SUCCESS
}
