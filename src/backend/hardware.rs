use crate::models::{DeviceCategory, DriverStatus, HardwareDevice};
use anyhow::Result;
use regex::Regex;
use std::process::Command;

pub struct HardwareScanner;

/// PCI device IDs (vendor 14e4) supported by the proprietary `wl` driver
/// (broadcom-wl): BCM4311/4312/4313, BCM4321/4322, BCM43224/43225/43227/
/// 43228, BCM43142, BCM4331, BCM4352, BCM4360. Anything else (notably
/// modern brcmfmac chips) must not get a broadcom-wl recommendation.
const BROADCOM_WL_DEVICE_IDS: &[&str] = &[
    "0576", "4311", "4312", "4313", "4315", "4328", "4329", "432a", "432b", "432c", "432d",
    "4331", "4353", "4357", "4358", "4359", "4360", "4365", "43a0", "43b1", "4727",
];

/// Outcome of a full hardware scan: devices from every backend that
/// succeeded, plus the failure of each backend that did not. One backend
/// failing (e.g. `lspci` missing) must not discard the other's results.
#[derive(Debug, Default)]
pub struct ScanReport {
    pub devices: Vec<HardwareDevice>,
    pub pci_error: Option<String>,
    pub usb_error: Option<String>,
    /// Format-mismatch warnings: a backend printed output none of our
    /// patterns matched (localized/older tool), so its results are
    /// Unknown rather than Optimal.
    pub warnings: Vec<String>,
}

impl ScanReport {
    /// All backend errors, for toasts and log lines.
    pub fn errors(&self) -> Vec<String> {
        [&self.pci_error, &self.usb_error]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    pub fn all_backends_failed(&self) -> bool {
        self.pci_error.is_some() && self.usb_error.is_some()
    }
}

/// Parsed backend output: the recognized devices plus a format-mismatch
/// warning when the tool printed content none of our patterns matched.
#[derive(Debug)]
pub struct ParseOutcome {
    pub devices: Vec<HardwareDevice>,
    pub warning: Option<String>,
}

impl HardwareScanner {
    /// Scan every backend independently, collecting each backend's devices
    /// or error without letting one failure abort the others.
    pub fn scan_all_report() -> ScanReport {
        let pci = Self::scan_pci();
        let usb = Self::scan_usb();
        let mut warnings = Vec::new();
        for parsed in [&pci, &usb].into_iter().flatten() {
            warnings.extend(parsed.warning.clone());
        }
        for warning in &warnings {
            log::warn!("{warning}");
        }
        let mut report = Self::combine_scan_results(
            pci.map(|parsed| parsed.devices),
            usb.map(|parsed| parsed.devices),
        );
        report.warnings = warnings;
        report
    }

    fn combine_scan_results(
        pci: Result<Vec<HardwareDevice>>,
        usb: Result<Vec<HardwareDevice>>,
    ) -> ScanReport {
        let mut report = ScanReport::default();
        match pci {
            Ok(mut devs) => report.devices.append(&mut devs),
            Err(e) => report.pci_error = Some(format!("PCI scan failed: {e:#}")),
        }
        match usb {
            Ok(mut devs) => report.devices.append(&mut devs),
            Err(e) => report.usb_error = Some(format!("USB scan failed: {e:#}")),
        }
        report
    }

    /// Non-empty tool output with zero recognized device lines means our
    /// patterns do not cover this format (localized/older/newer tool):
    /// warn so callers treat the backend as Unknown instead of Optimal.
    /// Genuine-empty output (and recognized-but-skipped lines such as USB
    /// root hubs) produce no warning.
    fn format_mismatch_warning(tool: &str, stdout: &str, recognized: usize) -> Option<String> {
        let has_content = stdout.lines().any(|line| !line.trim().is_empty());
        if recognized == 0 && has_content {
            Some(format!(
                "{tool} output matched no known device lines; format may be localized or unsupported (treating as unknown)"
            ))
        } else {
            None
        }
    }

    pub fn scan_pci() -> Result<ParseOutcome> {
        let output = Command::new("lspci").arg("-nnk").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_pci_output(&stdout)
    }

    fn parse_pci_output(stdout: &str) -> Result<ParseOutcome> {
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

        // Every `dev_re` match above flushes exactly one device, so the device
        // count doubles as the recognized-line count here.
        let warning = Self::format_mismatch_warning("lspci", stdout, devices.len());
        Ok(ParseOutcome { devices, warning })
    }

    pub fn scan_usb() -> Result<ParseOutcome> {
        let output = Command::new("lsusb").output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_usb_output(&stdout)
    }

    fn parse_usb_output(stdout: &str) -> Result<ParseOutcome> {
        // Case-insensitive: some lsusb builds/locales emit uppercase hex.
        let usb_re = Regex::new(r"(?i)^Bus\s+(\d+)\s+Device\s+(\d+):\s+ID\s+([0-9a-f]{4}):([0-9a-f]{4})\s*(.*)$")?;

        let mut devices = Vec::new();
        let mut recognized = 0usize;

        for line in stdout.lines() {
            if let Some(caps) = usb_re.captures(line) {
                recognized += 1;
                let bus = &caps[1];
                let dev = &caps[2];
                let v_id = &caps[3];
                let d_id = &caps[4];
                let name = caps[5].trim();

                // Skip root hubs (case-insensitive: IDs may be uppercase hex).
                if v_id.eq_ignore_ascii_case("1d6b") {
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

                let vendor = Self::usb_vendor_from_name(name).to_string();

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

        // Recognized-but-skipped root hubs count as recognized: hub-only
        // output is genuine-empty, not a format mismatch.
        let warning = Self::format_mismatch_warning("lsusb", stdout, recognized);
        Ok(ParseOutcome { devices, warning })
    }

    /// First whitespace-delimited token of an lsusb product string, minus
    /// trailing punctuation (`"Logitech, Inc. ..."` -> `"Logitech"`).
    fn usb_vendor_from_name(name: &str) -> &str {
        name.split_whitespace()
            .next()
            .map(|token| {
                token.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
            })
            .filter(|s| !s.is_empty())
            .unwrap_or("USB Device")
    }

    fn categorize_pci(class_code: &str, name: &str) -> DeviceCategory {
        // PCI class codes decide the category. Name substring matching used
        // to override them, so a 0604 bridge named "PCI Express Graphics
        // Port" showed up as a GPU and every 0c03 USB controller was labeled
        // Thunderbolt. Only Thunderbolt keeps a name fallback: its PCI
        // functions genuinely span classes (NHI system peripherals, bridges),
        // while 0c03 hosts stay plain USB controllers per their class code.
        if class_code.starts_with("03") {
            return DeviceCategory::Gpu;
        }
        if class_code.starts_with("02") {
            return DeviceCategory::Network;
        }
        if class_code.starts_with("04") {
            return DeviceCategory::Audio;
        }
        if class_code.starts_with("01") {
            return DeviceCategory::Storage;
        }
        if class_code.starts_with("0c03") {
            return DeviceCategory::Other;
        }

        if name.to_lowercase().contains("thunderbolt") {
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

    /// True when the card is a Broadcom chip on the `wl` support list.
    fn broadcom_wl_supported(v_id: &str, d_id: &str) -> bool {
        v_id.eq_ignore_ascii_case("14e4")
            && BROADCOM_WL_DEVICE_IDS
                .iter()
                .any(|known| d_id.eq_ignore_ascii_case(known))
    }

    fn evaluate_pci_status(
        category: &DeviceCategory,
        v_id: &str,
        d_id: &str,
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
                } else if Self::broadcom_wl_supported(v_id, d_id) {
                    // Broadcom chip on the wl support list.
                    (
                        DriverStatus::Missing,
                        Some("Broadcom proprietary driver recommended (broadcom-wl)".to_string()),
                        Some("broadcom-wl".to_string()),
                    )
                } else if v_id.eq_ignore_ascii_case("14e4") {
                    // Modern/unsupported Broadcom chip: wl neither supports
                    // it nor builds on current kernels. Point at the
                    // in-kernel drivers instead of a harmful recommendation.
                    (
                        DriverStatus::Missing,
                        Some(
                            "Broadcom adapter not supported by broadcom-wl; check in-kernel b43/brcmfmac support"
                                .to_string(),
                        ),
                        None,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim `lspci -nnk` lines captured on this machine (Lenovo Raptor Lake).
    const LSPCI_FIXTURE: &str = "00:06.0 PCI bridge [0604]: Intel Corporation Raptor Lake PCI Express 4.0 Graphics Port [8086:a74d]\n\tSubsystem: Lenovo Device [17aa:383d]\n\tKernel driver in use: pcieport\n\tKernel modules: shpchp\n00:0d.0 USB controller [0c03]: Intel Corporation Raptor Lake-P Thunderbolt 4 USB Controller [8086:a71e]\n\tSubsystem: Intel Corporation Device [8086:7270]\n\tKernel driver in use: xhci_hcd\n\tKernel modules: xhci_pci\n00:14.0 USB controller [0c03]: Intel Corporation Alder Lake PCH USB 3.2 xHCI Host Controller [8086:51ed] (rev 01)\n\tSubsystem: Lenovo Device [17aa:386b]\n\tKernel driver in use: xhci_hcd\n\tKernel modules: xhci_pci\n00:02.0 VGA compatible controller [0300]: Intel Corporation Raptor Lake-P [UHD Graphics] [8086:a7a8] (rev 04)\n\tSubsystem: Lenovo Device [17aa:3812]\n\tKernel driver in use: i915\n\tKernel modules: i915, xe\n";

    fn category_of(fixture: &str, slot: &str) -> DeviceCategory {
        HardwareScanner::parse_pci_output(fixture)
            .expect("fixture must parse")
            .devices
            .into_iter()
            .find(|d| d.id == slot)
            .unwrap_or_else(|| panic!("slot {slot} missing from fixture"))
            .category
    }

    #[test]
    fn graphics_named_bridge_is_not_a_gpu() {
        assert_eq!(
            HardwareScanner::categorize_pci(
                "0604",
                "Intel Corporation Raptor Lake PCI Express 4.0 Graphics Port"
            ),
            DeviceCategory::Other
        );
        assert_eq!(category_of(LSPCI_FIXTURE, "00:06.0"), DeviceCategory::Other);
    }

    #[test]
    fn plain_usb_controller_is_not_thunderbolt() {
        assert_eq!(
            HardwareScanner::categorize_pci(
                "0c03",
                "Intel Corporation Alder Lake PCH USB 3.2 xHCI Host Controller"
            ),
            DeviceCategory::Other
        );
        assert_eq!(category_of(LSPCI_FIXTURE, "00:14.0"), DeviceCategory::Other);
    }

    #[test]
    fn usb4_thunderbolt_port_follows_class_code() {
        // Integrated Thunderbolt 4 USB controller: class 0c03 wins over the
        // "Thunderbolt" name substring.
        assert_eq!(
            HardwareScanner::categorize_pci(
                "0c03",
                "Intel Corporation Raptor Lake-P Thunderbolt 4 USB Controller"
            ),
            DeviceCategory::Other
        );
        assert_eq!(category_of(LSPCI_FIXTURE, "00:0d.0"), DeviceCategory::Other);
    }

    #[test]
    fn real_gpu_stays_gpu() {
        assert_eq!(
            HardwareScanner::categorize_pci("0300", "Intel Corporation Raptor Lake-P [UHD Graphics]"),
            DeviceCategory::Gpu
        );
        assert_eq!(
            HardwareScanner::categorize_pci("0302", "NVIDIA Corporation GA107M [GeForce RTX 3050 Mobile]"),
            DeviceCategory::Gpu
        );
        assert_eq!(category_of(LSPCI_FIXTURE, "00:02.0"), DeviceCategory::Gpu);
    }

    #[test]
    fn discrete_thunderbolt_nhi_stays_thunderbolt() {
        // Discrete Thunderbolt NHI is a System peripheral (0880), so the
        // Thunderbolt name fallback still applies there.
        assert_eq!(
            HardwareScanner::categorize_pci("0880", "Intel Corporation Thunderbolt 4 NHI"),
            DeviceCategory::Thunderbolt
        );
    }

    #[test]
    fn known_classes_keep_their_categories() {
        assert_eq!(
            HardwareScanner::categorize_pci("0280", "Intel Corporation Raptor Lake PCH CNVi WiFi"),
            DeviceCategory::Network
        );
        assert_eq!(
            HardwareScanner::categorize_pci("0401", "Intel Corporation Raptor Lake-P/U/H cAVS"),
            DeviceCategory::Audio
        );
        assert_eq!(
            HardwareScanner::categorize_pci("0108", "Micron Technology Inc 2500 NVMe SSD (DRAM-less)"),
            DeviceCategory::Storage
        );
    }

    fn test_device(id: &str) -> HardwareDevice {
        HardwareDevice {
            id: id.to_string(),
            name: format!("Test device {id}"),
            vendor: "Test Vendor".to_string(),
            vendor_id: "1234".to_string(),
            device_id: "5678".to_string(),
            subsystem: None,
            category: DeviceCategory::Other,
            current_driver: None,
            available_modules: Vec::new(),
            status: DriverStatus::Unknown,
            recommendation: None,
            package_candidate: None,
        }
    }

    #[test]
    fn pci_failure_still_yields_usb_results() {
        let report = HardwareScanner::combine_scan_results(
            Err(anyhow::anyhow!("lspci not found")),
            Ok(vec![test_device("usb:1:2")]),
        );
        assert_eq!(report.devices.len(), 1);
        assert_eq!(report.devices[0].id, "usb:1:2");
        assert!(report.pci_error.is_some());
        assert!(report.usb_error.is_none());
        assert!(!report.all_backends_failed());
    }

    #[test]
    fn usb_failure_still_yields_pci_results() {
        let report = HardwareScanner::combine_scan_results(
            Ok(vec![test_device("00:02.0")]),
            Err(anyhow::anyhow!("lsusb not found")),
        );
        assert_eq!(report.devices.len(), 1);
        assert_eq!(report.devices[0].id, "00:02.0");
        assert!(report.pci_error.is_none());
        assert!(report.usb_error.is_some());
        assert!(!report.all_backends_failed());
    }

    #[test]
    fn both_failures_yield_errors_and_no_devices() {
        let report = HardwareScanner::combine_scan_results(
            Err(anyhow::anyhow!("lspci not found")),
            Err(anyhow::anyhow!("lsusb not found")),
        );
        assert!(report.devices.is_empty());
        assert!(report.pci_error.is_some());
        assert!(report.usb_error.is_some());
        assert!(report.all_backends_failed());
    }

    #[test]
    fn successful_scans_combine() {
        let report = HardwareScanner::combine_scan_results(
            Ok(vec![test_device("00:02.0")]),
            Ok(vec![test_device("usb:1:2")]),
        );
        assert_eq!(report.devices.len(), 2);
        assert!(report.pci_error.is_none());
        assert!(report.usb_error.is_none());
        assert!(!report.all_backends_failed());
    }

    #[test]
    fn broadcom_wl_recommended_for_supported_ids() {
        // BCM4352 (14e4:43b1): on the wl support list.
        let (status, _, pkg) =
            HardwareScanner::evaluate_pci_status(&DeviceCategory::Network, "14e4", "43b1", &None, &[]);
        assert_eq!(status, DriverStatus::Missing);
        assert_eq!(pkg.as_deref(), Some("broadcom-wl"));
    }

    #[test]
    fn broadcom_wl_withheld_for_modern_chips() {
        // BCM43602 (14e4:43ba): brcmfmac territory, wl does not support it.
        let (status, rec, pkg) =
            HardwareScanner::evaluate_pci_status(&DeviceCategory::Network, "14e4", "43ba", &None, &[]);
        assert_eq!(status, DriverStatus::Missing);
        assert_eq!(pkg, None);
        assert!(rec.expect("status must stay informative").contains("not supported by broadcom-wl"));
    }

    #[test]
    fn broadcom_id_match_is_case_insensitive() {
        let (_, _, pkg) = HardwareScanner::evaluate_pci_status(
            &DeviceCategory::Network,
            "14E4",
            "43B1",
            &None,
            &[],
        );
        assert_eq!(pkg.as_deref(), Some("broadcom-wl"));
    }

    #[test]
    fn broadcom_with_driver_loaded_needs_nothing() {
        let (status, _, pkg) = HardwareScanner::evaluate_pci_status(
            &DeviceCategory::Network,
            "14e4",
            "43b1",
            &Some("wl".to_string()),
            &[],
        );
        assert_eq!(status, DriverStatus::Optimal);
        assert_eq!(pkg, None);
    }

    /// `lspci` output in a shape our patterns do not cover (no `[vvvv:dddd]`
    /// IDs, as with localized or very old versions): non-empty yet zero
    /// matched lines must raise a format warning, not parse as "no hardware".
    const LSPCI_UNRECOGNIZED_FIXTURE: &str = "Peripherique VGA compatible : Intel Corporation UHD Graphics (rev 04)\nPont PCI : Intel Corporation Raptor Lake PCI Express Graphics Port\n";

    const LSUSB_UPPERCASE_FIXTURE: &str = "Bus 001 Device 002: ID 046D:C52B Logitech USB Receiver\nBus 001 Device 001: ID 1D6B:0002 Linux Foundation 2.0 root hub\n";

    const LSUSB_UNRECOGNIZED_FIXTURE: &str = "Peripheriques USB :\nquelque chose d'inattendu\n";

    #[test]
    fn unrecognized_lspci_output_yields_format_warning() {
        let outcome =
            HardwareScanner::parse_pci_output(LSPCI_UNRECOGNIZED_FIXTURE).expect("parse must not fail");
        assert!(outcome.devices.is_empty());
        assert!(outcome.warning.is_some(), "expected a format-mismatch warning");
    }

    #[test]
    fn empty_pci_output_yields_no_warning() {
        for input in ["", "  \n\n"] {
            let outcome = HardwareScanner::parse_pci_output(input).expect("parse must not fail");
            assert!(outcome.devices.is_empty());
            assert!(outcome.warning.is_none(), "empty input is genuine-empty, not a mismatch");
        }
    }

    #[test]
    fn normal_pci_output_yields_no_warning() {
        let outcome =
            HardwareScanner::parse_pci_output(LSPCI_FIXTURE).expect("fixture must parse");
        assert!(!outcome.devices.is_empty());
        assert!(outcome.warning.is_none());
    }

    #[test]
    fn usb_ids_match_case_insensitively() {
        let outcome =
            HardwareScanner::parse_usb_output(LSUSB_UPPERCASE_FIXTURE).expect("fixture must parse");
        assert!(outcome.warning.is_none());
        assert_eq!(outcome.devices.len(), 1, "uppercase root hub must still be skipped");
        assert_eq!(outcome.devices[0].vendor_id, "046D");
        assert_eq!(outcome.devices[0].device_id, "C52B");
    }

    #[test]
    fn root_hub_only_usb_output_yields_no_warning() {
        let outcome = HardwareScanner::parse_usb_output(
            "Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub\n",
        )
        .expect("parse must not fail");
        assert!(outcome.devices.is_empty());
        assert!(outcome.warning.is_none(), "recognized hubs are genuine-empty");
    }

    #[test]
    fn unrecognized_usb_output_yields_warning() {
        let outcome =
            HardwareScanner::parse_usb_output(LSUSB_UNRECOGNIZED_FIXTURE).expect("parse must not fail");
        assert!(outcome.devices.is_empty());
        assert!(outcome.warning.is_some(), "expected a format-mismatch warning");
    }

    #[test]
    fn usb_vendor_strips_trailing_punctuation() {
        assert_eq!(
            HardwareScanner::usb_vendor_from_name("Logitech, Inc. Unifying Receiver"),
            "Logitech"
        );
        assert_eq!(HardwareScanner::usb_vendor_from_name("Logitech USB Receiver"), "Logitech");
        assert_eq!(HardwareScanner::usb_vendor_from_name("Acme... Widget"), "Acme");
        assert_eq!(HardwareScanner::usb_vendor_from_name(""), "USB Device");
        assert_eq!(HardwareScanner::usb_vendor_from_name(",,,"), "USB Device");
    }

    const LSUSB_VENDOR_PUNCT_FIXTURE: &str =
        "Bus 001 Device 003: ID 046d:c52b Logitech, Inc. Unifying Receiver\n";

    #[test]
    fn parsed_usb_vendor_has_no_trailing_comma() {
        let outcome = HardwareScanner::parse_usb_output(LSUSB_VENDOR_PUNCT_FIXTURE)
            .expect("fixture must parse");
        assert_eq!(outcome.devices.len(), 1);
        assert_eq!(outcome.devices[0].vendor, "Logitech");
        assert_eq!(outcome.devices[0].name, "Logitech, Inc. Unifying Receiver");
    }
}
