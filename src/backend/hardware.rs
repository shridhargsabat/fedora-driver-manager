use crate::models::{DeviceCategory, DriverStatus, HardwareDevice};
use anyhow::Result;
use regex::Regex;
use std::process::Command;

pub struct HardwareScanner;

impl HardwareScanner {
    pub fn scan_all() -> Result<Vec<HardwareDevice>> {
        let mut devices = Vec::new();
        devices.extend(Self::scan_pci()?);
        devices.extend(Self::scan_usb()?);
        Ok(devices)
    }

    pub fn scan_pci() -> Result<Vec<HardwareDevice>> {
        let output = Command::new("lspci").arg("-nnk").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let dev_re = Regex::new(
            r"^([0-9a-f:.]+)\s+([^\[]+?)\s+\[([0-9a-f]{4})\]:\s+(.+?)\s+\[([0-9a-f]{4}):([0-9a-f]{4})\]",
        )?;
        let driver_re = Regex::new(r"^\s+Kernel driver in use:\s+(.+)")?;
        let modules_re = Regex::new(r"^\s+Kernel modules:\s+(.+)")?;
        let subsys_re = Regex::new(r"^\s+Subsystem:\s+(.+)")?;

        let mut devices = Vec::new();
        let mut current_slot = String::new();
        let mut current_class = String::new();
        let mut current_name = String::new();
        let mut current_vendor_id = String::new();
        let mut current_dev_id = String::new();
        let mut current_driver = None;
        let mut current_modules = Vec::new();
        let mut current_subsys = None;

        let flush = |slot: &str,
                     class_str: &str,
                     name_str: &str,
                     v_id: &str,
                     d_id: &str,
                     driver: Option<String>,
                     modules: Vec<String>,
                     subsys: Option<String>,
                     list: &mut Vec<HardwareDevice>| {
            if slot.is_empty() {
                return;
            }

            let category = Self::categorize_pci(class_str, name_str);
            let (status, recommendation, pkg) =
                Self::evaluate_pci_status(&category, v_id, d_id, &driver, &modules);

            let vendor = Self::extract_vendor(name_str, v_id);

            list.push(HardwareDevice {
                id: slot.to_string(),
                name: name_str.trim().to_string(),
                vendor,
                vendor_id: v_id.to_string(),
                device_id: d_id.to_string(),
                subsystem: subsys,
                category,
                current_driver: driver,
                available_modules: modules,
                status,
                recommendation,
                package_candidate: pkg,
            });
        };

        for line in stdout.lines() {
            if let Some(caps) = dev_re.captures(line) {
                flush(
                    &current_slot,
                    &current_class,
                    &current_name,
                    &current_vendor_id,
                    &current_dev_id,
                    current_driver.take(),
                    std::mem::take(&mut current_modules),
                    current_subsys.take(),
                    &mut devices,
                );

                current_slot = caps[1].to_string();
                current_class = caps[3].to_string();
                current_name = caps[4].to_string();
                current_vendor_id = caps[5].to_string();
                current_dev_id = caps[6].to_string();
            } else if let Some(caps) = driver_re.captures(line) {
                current_driver = Some(caps[1].trim().to_string());
            } else if let Some(caps) = modules_re.captures(line) {
                current_modules = caps[1]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
            } else if let Some(caps) = subsys_re.captures(line) {
                current_subsys = Some(caps[1].trim().to_string());
            }
        }

        flush(
            &current_slot,
            &current_class,
            &current_name,
            &current_vendor_id,
            &current_dev_id,
            current_driver.take(),
            std::mem::take(&mut current_modules),
            current_subsys.take(),
            &mut devices,
        );

        Ok(devices)
    }

    pub fn scan_usb() -> Result<Vec<HardwareDevice>> {
        let output = Command::new("lsusb").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let usb_re = Regex::new(r"^Bus\s+(\d+)\s+Device\s+(\d+):\s+ID\s+([0-9a-f]{4}):([0-9a-f]{4})\s*(.*)$")?;

        let mut devices = Vec::new();

        for line in stdout.lines() {
            if let Some(caps) = usb_re.captures(line) {
                let bus = &caps[1];
                let dev = &caps[2];
                let v_id = &caps[3];
                let d_id = &caps[4];
                let name = caps[5].trim();

                // Skip root hubs
                if v_id == "1d6b" {
                    continue;
                }

                let category = if name.to_lowercase().contains("camera") || name.to_lowercase().contains("webcam") {
                    DeviceCategory::Camera
                } else if name.to_lowercase().contains("bluetooth") {
                    DeviceCategory::Network
                } else if name.to_lowercase().contains("mouse") || name.to_lowercase().contains("keyboard") {
                    DeviceCategory::Input
                } else {
                    DeviceCategory::Other
                };

                let vendor = name.split_whitespace().next().unwrap_or("USB Device").to_string();

                devices.push(HardwareDevice {
                    id: format!("usb:{}:{}", bus, dev),
                    name: name.to_string(),
                    vendor,
                    vendor_id: v_id.to_string(),
                    device_id: d_id.to_string(),
                    subsystem: None,
                    category,
                    current_driver: None,
                    available_modules: Vec::new(),
                    status: DriverStatus::Optimal,
                    recommendation: Some("Operated via kernel USB subsystem".to_string()),
                    package_candidate: None,
                });
            }
        }

        Ok(devices)
    }

    fn categorize_pci(class_code: &str, name: &str) -> DeviceCategory {
        let lower = name.to_lowercase();
        if class_code.starts_with("03") || lower.contains("vga") || lower.contains("graphics") || lower.contains("display") {
            DeviceCategory::Gpu
        } else if class_code.starts_with("02") || lower.contains("network") || lower.contains("wifi") || lower.contains("wireless") || lower.contains("ethernet") {
            DeviceCategory::Network
        } else if class_code.starts_with("04") || lower.contains("audio") || lower.contains("sound") {
            DeviceCategory::Audio
        } else if class_code.starts_with("01") || lower.contains("nvme") || lower.contains("storage") || lower.contains("sata") {
            DeviceCategory::Storage
        } else if class_code.starts_with("0c03") || lower.contains("thunderbolt") {
            DeviceCategory::Thunderbolt
        } else {
            DeviceCategory::Other
        }
    }

    fn extract_vendor(name: &str, vendor_id: &str) -> String {
        if vendor_id == "8086" {
            "Intel Corporation".to_string()
        } else if vendor_id == "10de" {
            "NVIDIA Corporation".to_string()
        } else if vendor_id == "1002" {
            "AMD / ATI".to_string()
        } else if vendor_id == "10ec" {
            "Realtek".to_string()
        } else {
            name.split_whitespace().next().unwrap_or("Unknown").to_string()
        }
    }

    fn evaluate_pci_status(
        category: &DeviceCategory,
        v_id: &str,
        _d_id: &str,
        driver: &Option<String>,
        _modules: &[String],
    ) -> (DriverStatus, Option<String>, Option<String>) {
        match category {
            DeviceCategory::Gpu => {
                if v_id == "10de" {
                    // NVIDIA
                    if let Some(drv) = driver {
                        if drv == "nvidia" {
                            (DriverStatus::Optimal, Some("NVIDIA proprietary driver active".to_string()), None)
                        } else {
                            (
                                DriverStatus::Upgradable,
                                Some("RPM Fusion akmod-nvidia recommended for full performance".to_string()),
                                Some("akmod-nvidia".to_string()),
                            )
                        }
                    } else {
                        (
                            DriverStatus::Missing,
                            Some("No driver loaded for NVIDIA GPU".to_string()),
                            Some("akmod-nvidia".to_string()),
                        )
                    }
                } else if v_id == "8086" {
                    // Intel GPU
                    (
                        DriverStatus::Optimal,
                        Some("Intel open-source driver (i915/xe) active with VA-API support".to_string()),
                        Some("libva-intel-media-driver".to_string()),
                    )
                } else {
                    (DriverStatus::Optimal, Some("Open source driver loaded".to_string()), None)
                }
            }
            DeviceCategory::Network => {
                if driver.is_some() {
                    (DriverStatus::Optimal, Some("In-kernel wireless/network driver active".to_string()), None)
                } else if v_id == "14e4" {
                    // Broadcom
                    (
                        DriverStatus::Missing,
                        Some("Broadcom proprietary driver recommended (broadcom-wl)".to_string()),
                        Some("broadcom-wl".to_string()),
                    )
                } else {
                    (DriverStatus::Missing, Some("No driver loaded for this network adapter".to_string()), None)
                }
            }
            _ => {
                if driver.is_some() {
                    (DriverStatus::Optimal, Some("Driver actively in use".to_string()), None)
                } else {
                    (DriverStatus::Unknown, Some("Standard kernel subsystem management".to_string()), None)
                }
            }
        }
    }
}
