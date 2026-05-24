use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter};
use btleplug::platform::{Manager, Peripheral};
use uuid::Uuid;

const LYWSD03MMC_NAME: &str = "LYWSD03MMC";
const LYWSD03MMC_DATA_UUID: &str = "EBE0CCC1-7A0A-4B0C-8A1A-6FF2997DA3A6";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BleDevice {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lywsd03mmcReading {
    pub device: BleDevice,
    pub temperature_celsius: f32,
    pub humidity_percent: u8,
    pub battery_voltage: f32,
    pub battery_percent: u8,
}

impl Display for BleDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
        "id={} address={} type={} name={} rssi={} tx_power={} class={} services=[{}] manufacturer_data=[{}] service_data=[{}]",
        self.id,
        self.address,
        self.address_type.as_deref().unwrap_or("unknown"),
        self.name.as_deref().unwrap_or("(unknown)"),
        self
            .rssi
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        self
            .tx_power_level
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        self
            .class
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        self.services.join(", "),
        self.manufacturer_data.join(", "),
        self.service_data.join(", "),
    )
    }
}

fn format_reading(reading: &Lywsd03mmcReading) -> String {
    format!(
        "{} temperature_celsius={:.2} humidity_percent={} battery_voltage={:.3} battery_percent={}",
        reading.device,
        reading.temperature_celsius,
        reading.humidity_percent,
        reading.battery_voltage,
        reading.battery_percent,
    )
}

fn build_device(id: String, properties: PeripheralProperties) -> BleDevice {
    BleDevice {
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
    }
}

fn battery_percent_from_voltage(voltage: f32) -> u8 {
    let scaled = ((voltage - 2.1) * 100.0).clamp(0.0, 100.0);
    scaled.round() as u8
}

fn parse_sensor_data(device: BleDevice, data: &[u8]) -> btleplug::Result<Lywsd03mmcReading> {
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

    Ok(Lywsd03mmcReading {
        device,
        temperature_celsius,
        humidity_percent,
        battery_voltage,
        battery_percent,
    })
}

async fn scan_lywsd03mmc_peripherals(
    scan_time: Duration,
    id_filter: Option<&str>,
) -> btleplug::Result<BTreeMap<String, (BleDevice, Peripheral)>> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let adapter = adapters
        .into_iter()
        .next()
        .ok_or_else(|| btleplug::Error::RuntimeError("no Bluetooth adapters found".to_string()))?;

    adapter.start_scan(ScanFilter::default()).await?;
    let mut elapsed = Duration::ZERO;
    let mut found_devices = BTreeMap::new();
    let id_filter = id_filter.map(|value| value.to_ascii_lowercase());

    while elapsed < scan_time {
        tokio::time::sleep(Duration::from_secs(1)).await;
        elapsed += Duration::from_secs(1);

        for peripheral in adapter.peripherals().await? {
            if let Some(properties) = peripheral.properties().await? {
                let id = peripheral.id().to_string();
                let device = build_device(id.clone(), properties);
                if device.name.as_deref() == Some(LYWSD03MMC_NAME) {
                    let matches_filter = id_filter.as_ref().is_some_and(|filter| {
                        id.eq_ignore_ascii_case(filter)
                            || device.address.eq_ignore_ascii_case(filter)
                    });
                    let is_new = !found_devices.contains_key(&id);
                    found_devices.insert(id, (device.clone(), peripheral.clone()));
                    if is_new {
                        println!("{device}");
                    }
                    if matches_filter {
                        let _ = adapter.stop_scan().await;
                        return Ok(found_devices);
                    }
                }
            }
        }
    }

    let _ = adapter.stop_scan().await;
    Ok(found_devices)
}

pub async fn find_lywsd03mmc(scan_time: Duration) -> btleplug::Result<Vec<BleDevice>> {
    Ok(scan_lywsd03mmc_peripherals(scan_time, None)
        .await?
        .into_values()
        .map(|(device, _)| device)
        .collect())
}

pub async fn read_lywsd03mmc(
    scan_time: Duration,
    id_filter: Option<&str>,
) -> btleplug::Result<Vec<Lywsd03mmcReading>> {
    let data_uuid = Uuid::parse_str(LYWSD03MMC_DATA_UUID)
        .expect("LYWSD03MMC data UUID should be a valid UUID literal");
    let peripherals = scan_lywsd03mmc_peripherals(scan_time, id_filter).await?;
    let mut readings = Vec::with_capacity(peripherals.len());

    for (_, (device, peripheral)) in peripherals {
        let reading = read_lywsd03mmc_from_peripheral(device, peripheral, data_uuid).await?;
        println!("{}", format_reading(&reading));
        readings.push(reading);
    }

    Ok(readings)
}

async fn read_lywsd03mmc_from_peripheral(
    device: BleDevice,
    peripheral: Peripheral,
    data_uuid: Uuid,
) -> btleplug::Result<Lywsd03mmcReading> {
    let mut connected_here = false;

    if !peripheral.is_connected().await? {
        peripheral.connect().await?;
        connected_here = true;
    }

    let read_result = async {
        peripheral.discover_services().await?;
        let characteristic = peripheral
            .characteristics()
            .into_iter()
            .find(|characteristic| characteristic.uuid == data_uuid)
            .ok_or_else(|| {
                btleplug::Error::RuntimeError(format!(
                    "LYWSD03MMC data characteristic {LYWSD03MMC_DATA_UUID} not found for {}",
                    device.id
                ))
            })?;
        let raw = peripheral.read(&characteristic).await?;
        parse_sensor_data(device, &raw)
    }
    .await;

    if connected_here {
        let _ = peripheral.disconnect().await;
    }

    read_result
}

#[cfg(test)]
mod tests {
    use super::{BleDevice, battery_percent_from_voltage, parse_sensor_data, read_lywsd03mmc};
    use std::time::Duration;

    #[test]
    fn parses_lywsd03mmc_sensor_payload() {
        let device = BleDevice {
            id: "test-device".to_string(),
            address: "00:00:00:00:00:00".to_string(),
            address_type: None,
            name: Some("LYWSD03MMC".to_string()),
            rssi: None,
            tx_power_level: None,
            services: Vec::new(),
            manufacturer_data: Vec::new(),
            service_data: Vec::new(),
            class: None,
        };

        let reading = parse_sensor_data(device, &[0xA8, 0x08, 0x17, 0xB9, 0x0B]).unwrap();

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
    async fn reads_lywsd03mmc_data() {
        let readings = read_lywsd03mmc(Duration::from_secs(15), Some("2b56c5ee-1288-a2f1-d82f-ad70b2fd8c69"))
            .await
            .expect("LYWSD03MMC read should complete successfully");

        assert!(
            !readings.is_empty(),
            "expected to read at least one LYWSD03MMC device during the scan window"
        );

        for r in readings{
            println!("Read LYWSD03MMC data: {:?}", r);
        }
    }
}
