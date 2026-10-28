use crate::models::{DnfTransaction, FirmwareItem};
use anyhow::Result;
use std::process::Command;

pub struct SystemBackend;

impl SystemBackend {
    pub fn get_secure_boot_state() -> String {
        let output = Command::new("mokutil").arg("--sb-state").output();
        if let Ok(out) = output {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.contains("SecureBoot enabled") {
                return "Secure Boot: Enabled".to_string();
            } else if s.contains("SecureBoot disabled") {
                return "Secure Boot: Disabled".to_string();
            }
        }
        "Secure Boot: Unknown".to_string()
    }

    pub fn get_laptop_model() -> String {
        let vendor = std::fs::read_to_string("/sys/devices/virtual/dmi/id/sys_vendor")
            .unwrap_or_else(|_| "Generic".to_string());
        let product = std::fs::read_to_string("/sys/devices/virtual/dmi/id/product_version")
            .unwrap_or_else(|_| "Laptop".to_string());
        format!("{} {}", vendor.trim(), product.trim())
    }

    pub fn get_firmware_devices() -> Result<Vec<FirmwareItem>> {
        let output = Command::new("fwupdmgr")
            .args(["get-devices", "--json", "--no-unreported-check"])
            .output()?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let val: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let mut items = Vec::new();

        if let Some(devices) = val.get("Devices").and_then(|d| d.as_array()) {
            for dev in devices {
                let name = dev.get("Name").and_then(|n| n.as_str()).unwrap_or("Unknown Device");
                let vendor = dev.get("Vendor").and_then(|v| v.as_str()).unwrap_or("Unknown Vendor");
                let version = dev.get("Version").and_then(|v| v.as_str()).unwrap_or("N/A");
                let summary = dev.get("Summary").and_then(|s| s.as_str()).map(|s| s.to_string());

                let mut updatable = false;
                if let Some(flags) = dev.get("Flags").and_then(|f| f.as_array()) {
                    for flag in flags {
                        if flag.as_str() == Some("updatable") {
                            updatable = true;
                            break;
                        }
                    }
                }

                items.push(FirmwareItem {
                    name: name.to_string(),
                    vendor: vendor.to_string(),
                    version: version.to_string(),
                    updatable,
                    summary,
                });
            }
        }

        Ok(items)
    }

    pub fn get_dnf_history(limit: usize) -> Result<Vec<DnfTransaction>> {
        let output = Command::new("dnf5")
            .args(["history", "list"])
            .output()?;

        let mut list = Vec::new();
        if !output.status.success() {
            return Ok(list);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1).take(limit) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            if let Ok(id) = parts[0].parse::<u64>() {
                list.push(DnfTransaction {
                    id,
                    command_line: parts[1..].join(" "),
                    date: "Recent".to_string(),
                    action: if parts.len() > 1 { parts[1].to_string() } else { "Transaction".to_string() },
                });
            }
        }

        Ok(list)
    }

    pub fn execute_pkexec(args: &[&str]) -> Result<String> {
        let output = Command::new("pkexec")
            .args(args)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!("Command failed: {}", stderr);
        }

        Ok(stdout)
    }
}
