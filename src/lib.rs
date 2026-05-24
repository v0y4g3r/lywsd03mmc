use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::Manager;
use std::collections::BTreeMap;

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

fn format_device(device: &BleDevice) -> String {
    format!(
        "id={} address={} type={} name={} rssi={} tx_power={} class={} services=[{}] manufacturer_data=[{}] service_data=[{}]",
        device.id,
        device.address,
        device.address_type.as_deref().unwrap_or("unknown"),
        device.name.as_deref().unwrap_or("(unknown)"),
        device
            .rssi
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        device
            .tx_power_level
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        device
            .class
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        device.services.join(", "),
        device.manufacturer_data.join(", "),
        device.service_data.join(", "),
    )
}

fn build_device(id: String, properties: btleplug::api::PeripheralProperties) -> BleDevice {
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

pub async fn find_lywsd03mmc(scan_time: Duration) -> btleplug::Result<Vec<BleDevice>> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let adapter = adapters
        .into_iter()
        .next()
        .ok_or_else(|| btleplug::Error::RuntimeError("no Bluetooth adapters found".to_string()))?;

    adapter.start_scan(ScanFilter::default()).await?;
    let mut elapsed = Duration::ZERO;
    let mut found_devices = BTreeMap::new();

    while elapsed < scan_time {
        tokio::time::sleep(Duration::from_secs(1)).await;
        elapsed += Duration::from_secs(1);

        for peripheral in adapter.peripherals().await? {
            if let Some(properties) = peripheral.properties().await? {
                let id = peripheral.id().to_string();
                let device = build_device(id.clone(), properties);
                if device.name.as_deref() == Some("LYWSD03MMC")
                    && found_devices.insert(id, device.clone()).is_none()
                {
                    println!("{}", format_device(&device));
                }
            }
        }
    }

    let _ = adapter.stop_scan().await;

    Ok(found_devices.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::find_lywsd03mmc;
    use std::time::Duration;

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires a Bluetooth adapter, OS BLE permissions, and nearby devices"]
    async fn finds_lywsd03mmc_devices() {
        let devices = find_lywsd03mmc(Duration::from_secs(500))
            .await
            .expect("LYWSD03MMC scan should complete successfully");

        assert!(
            !devices.is_empty(),
            "expected to discover at least one device named LYWSD03MMC during the scan window"
        );
    }
}
