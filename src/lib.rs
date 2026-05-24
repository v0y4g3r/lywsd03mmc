use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter};
use btleplug::platform::{Manager, Peripheral};
use log::{debug, info};
use uuid::Uuid;

const LYWSD03MMC_NAME: &str = "LYWSD03MMC";
const LYWSD03MMC_DATA_UUID: &str = "EBE0CCC1-7A0A-4B0C-8A1A-6FF2997DA3A6";
const DEFAULT_SCAN_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub address: String,
    pub address_type: Option<String>,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    pub tx_power_level: Option<i16>,
    pub services: Vec<String>,
    pub manufacturer_data: Vec<String>,
    pub service_data: Vec<String>,
    pub class: Option<u32>,
    pub(crate) peripheral: Option<Peripheral>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub temperature_celsius: f32,
    pub humidity_percent: u8,
    pub battery_voltage: f32,
    pub battery_percent: u8,
}

impl TryFrom<&[u8]> for Reading {
    type Error = btleplug::Error;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        if data.len() < 5 {
            return Err(btleplug::Error::RuntimeError(format!(
                "expected 5 bytes of sensor data, got {}",
                data.len()
            )));
        }

        let temperature_raw = i16::from_le_bytes([data[0], data[1]]);
        let humidity_percent = data[2];
        let battery_raw = i16::from_le_bytes([data[3], data[4]]);

        let temperature_celsius = temperature_raw as f32 / 100.0;
        let battery_voltage = battery_raw as f32 / 1000.0;
        let battery_percent = battery_percent_from_voltage(battery_voltage);

        Ok(Self {
            temperature_celsius,
            humidity_percent,
            battery_voltage,
            battery_percent,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Scanner {
    timeout: Option<Duration>,
    id_filter: Option<String>,
}

impl Scanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_id_filter(mut self, id_filter: impl Into<String>) -> Self {
        self.id_filter = Some(id_filter.into());
        self
    }

    pub async fn scan(&self) -> btleplug::Result<Vec<Device>> {
        let manager = Manager::new().await?;
        let adapters = manager.adapters().await?;
        let adapter = adapters.into_iter().next().ok_or_else(|| {
            btleplug::Error::RuntimeError("no Bluetooth adapters found".to_string())
        })?;

        let timeout = self.timeout.unwrap_or(DEFAULT_SCAN_TIMEOUT);
        let id_filter = self
            .id_filter
            .as_deref()
            .map(|value| value.to_ascii_lowercase());

        info!(
            "starting LYWSD03MMC scan with timeout={}s filter={:?}",
            timeout.as_secs(),
            self.id_filter
        );

        adapter.start_scan(ScanFilter::default()).await?;
        let mut elapsed = Duration::ZERO;
        let mut found_devices = BTreeMap::new();

        while elapsed < timeout {
            tokio::time::sleep(Duration::from_secs(1)).await;
            elapsed += Duration::from_secs(1);

            for peripheral in adapter.peripherals().await? {
                if let Some(properties) = peripheral.properties().await? {
                    let id = peripheral.id().to_string();
                    let device = build_device(id.clone(), properties, Some(peripheral.clone()));
                    if device.name.as_deref() != Some(LYWSD03MMC_NAME) {
                        continue;
                    }

                    let matches_filter = id_filter.as_ref().is_some_and(|filter| {
                        id.eq_ignore_ascii_case(filter)
                            || device.address.eq_ignore_ascii_case(filter)
                    });
                    let is_new = !found_devices.contains_key(&id);
                    found_devices.insert(id, device.clone());
                    if is_new {
                        info!("discovered {device}");
                    }
                    if matches_filter {
                        info!("matched filter for {}", device.id);
                        let _ = adapter.stop_scan().await;
                        return Ok(found_devices.into_values().collect());
                    }
                }
            }
        }

        let _ = adapter.stop_scan().await;
        info!("scan finished with {} device(s)", found_devices.len());
        Ok(found_devices.into_values().collect())
    }
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.address == other.address
            && self.address_type == other.address_type
            && self.name == other.name
            && self.rssi == other.rssi
            && self.tx_power_level == other.tx_power_level
            && self.services == other.services
            && self.manufacturer_data == other.manufacturer_data
            && self.service_data == other.service_data
            && self.class == other.class
    }
}

impl Eq for Device {}

impl Display for Device {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "id={} address={} type={} name={} rssi={} tx_power={} class={} services=[{}] manufacturer_data=[{}] service_data=[{}]",
            self.id,
            self.address,
            self.address_type.as_deref().unwrap_or("unknown"),
            self.name.as_deref().unwrap_or("(unknown)"),
            self.rssi
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            self.tx_power_level
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            self.class
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            self.services.join(", "),
            self.manufacturer_data.join(", "),
            self.service_data.join(", "),
        )
    }
}

impl Device {
    pub async fn read_data(&self) -> btleplug::Result<Reading> {
        info!("reading data from {}", self.id);
        let peripheral = self.peripheral.clone().ok_or_else(|| {
            btleplug::Error::RuntimeError(format!(
                "device {} does not carry an attached peripheral handle",
                self.id
            ))
        })?;
        let data_uuid = Uuid::parse_str(LYWSD03MMC_DATA_UUID)
            .expect("LYWSD03MMC data UUID should be a valid UUID literal");

        let mut connected_here = false;
        if !peripheral.is_connected().await? {
            debug!("connecting to {}", self.id);
            peripheral.connect().await?;
            connected_here = true;
        }

        let read_result = async {
            debug!("discovering services for {}", self.id);
            peripheral.discover_services().await?;
            let characteristic = peripheral
                .characteristics()
                .into_iter()
                .find(|characteristic| characteristic.uuid == data_uuid)
                .ok_or_else(|| {
                    btleplug::Error::RuntimeError(format!(
                        "LYWSD03MMC data characteristic {LYWSD03MMC_DATA_UUID} not found for {}",
                        self.id
                    ))
                })?;
            debug!("reading characteristic {} for {}", LYWSD03MMC_DATA_UUID, self.id);
            let raw = peripheral.read(&characteristic).await?;
            Reading::try_from(raw.as_slice())
        }
        .await;

        if connected_here {
            debug!("disconnecting from {}", self.id);
            let _ = peripheral.disconnect().await;
        }

        read_result
    }
}

fn build_device(
    id: String,
    properties: PeripheralProperties,
    peripheral: Option<Peripheral>,
) -> Device {
    Device {
        id,
        address: properties.address.to_string(),
        address_type: properties.address_type.map(|value| format!("{value:?}")),
        name: properties.local_name,
        rssi: properties.rssi,
        tx_power_level: properties.tx_power_level,
        services: properties
            .services
            .into_iter()
            .map(|uuid| uuid.to_string())
            .collect(),
        manufacturer_data: properties
            .manufacturer_data
            .into_iter()
            .map(|(id, data)| format!("{id:#06x}:{}", hex::encode(data)))
            .collect(),
        service_data: properties
            .service_data
            .into_iter()
            .map(|(uuid, data)| format!("{uuid}:{}", hex::encode(data)))
            .collect(),
        class: properties.class,
        peripheral,
    }
}

fn battery_percent_from_voltage(voltage: f32) -> u8 {
    let scaled = ((voltage - 2.1) * 100.0).clamp(0.0, 100.0);
    scaled.round() as u8
}

#[cfg(test)]
mod tests {
    use super::{Reading, Scanner, battery_percent_from_voltage};
    use std::time::Duration;

    #[test]
    fn parses_lywsd03mmc_sensor_payload() {
        let reading = Reading::try_from(&[0xA8, 0x08, 0x17, 0xB9, 0x0B][..]).unwrap();

        assert_eq!(reading.temperature_celsius, 22.16);
        assert_eq!(reading.humidity_percent, 23);
        assert_eq!(reading.battery_voltage, 3.001);
        assert_eq!(reading.battery_percent, 90);
    }

    #[test]
    fn clamps_battery_percent_to_valid_range() {
        assert_eq!(battery_percent_from_voltage(1.5), 0);
        assert_eq!(battery_percent_from_voltage(2.1), 0);
        assert_eq!(battery_percent_from_voltage(3.1), 100);
        assert_eq!(battery_percent_from_voltage(3.5), 100);
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires a Bluetooth adapter, OS BLE permissions, and nearby LYWSD03MMC devices"]
    async fn scanner_reads_lywsd03mmc_data() {
        let devices = Scanner::new()
            .with_timeout(Duration::from_secs(15))
            .with_id_filter("2b56c5ee-1288-a2f1-d82f-ad70b2fd8c69")
            .scan()
            .await
            .expect("LYWSD03MMC scan should complete successfully");

        assert!(
            !devices.is_empty(),
            "expected to discover at least one LYWSD03MMC device during the scan window"
        );

        let reading = devices[0]
            .read_data()
            .await
            .expect("LYWSD03MMC read should complete successfully");
        println!("{reading:?}");
    }
}
