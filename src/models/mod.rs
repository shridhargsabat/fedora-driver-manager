use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceCategory {
    Gpu,
    Network,
    Audio,
    Storage,
    Camera,
    Input,
    Thunderbolt,
    Other,
}

impl DeviceCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Gpu => "Graphics and Display",
            Self::Network => "Network and Wireless",
            Self::Audio => "Audio and Sound",
            Self::Storage => "Storage and NVMe",
            Self::Camera => "Camera and Imaging",
            Self::Input => "Input Devices",
            Self::Thunderbolt => "Thunderbolt and USB",
            Self::Other => "Other System Devices",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Gpu => "video-display-symbolic",
            Self::Network => "network-wireless-symbolic",
            Self::Audio => "audio-card-symbolic",
            Self::Storage => "drive-harddisk-symbolic",
            Self::Camera => "camera-web-symbolic",
            Self::Input => "input-touchpad-symbolic",
            Self::Thunderbolt => "thunderbolt-symbolic",
            Self::Other => "preferences-system-devices-symbolic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverStatus {
    Optimal,
    Upgradable,
    Missing,
    FirmwareUpdate,
    GenericFallback,
    Unknown,
}

impl DriverStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Optimal => "Working (Optimal)",
            Self::Upgradable => "Update / Alternative Available",
            Self::Missing => "Missing Driver",
            Self::FirmwareUpdate => "Firmware Update Available",
            Self::GenericFallback => "Using Generic Driver",
            Self::Unknown => "Status Unknown",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Optimal => "emblem-ok-symbolic",
            Self::Upgradable => "software-update-available-symbolic",
            Self::Missing => "dialog-error-symbolic",
            Self::FirmwareUpdate => "software-update-urgent-symbolic",
            Self::GenericFallback => "dialog-warning-symbolic",
            Self::Unknown => "dialog-question-symbolic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareDevice {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub vendor_id: String,
    pub device_id: String,
    pub subsystem: Option<String>,
    pub category: DeviceCategory,
    pub current_driver: Option<String>,
    pub available_modules: Vec<String>,
    pub status: DriverStatus,
    pub recommendation: Option<String>,
    pub package_candidate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareItem {
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub updatable: bool,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnfTransaction {
    pub id: u64,
    pub command_line: String,
    pub date: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageBuild {
    pub evr: String,
    pub arch: String,
    pub repo: String,
}

/// Installed/available state of one driver candidate package, resolved
/// from `dnf5 info --json`. `available` may hold several arches of the
/// same EVR; both fields empty means "not in any enabled repository".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverPackage {
    pub name: String,
    pub installed_evr: Option<String>,
    pub available: Vec<PackageBuild>,
}

impl DriverPackage {
    /// The build to display/install: x86_64 first, else first available.
    pub fn preferred_available(&self) -> Option<&PackageBuild> {
        self.available
            .iter()
            .find(|b| b.arch == "x86_64")
            .or_else(|| self.available.first())
    }

    /// One-line user-facing state for list rows.
    pub fn status_text(&self) -> String {
        if let Some(installed) = &self.installed_evr {
            return format!("Installed {installed}");
        }
        match self.preferred_available() {
            Some(build) => format!("Available {} from {}", build.evr, build.repo),
            None => "Not found in enabled repositories".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(installed: Option<&str>, available: &[(&str, &str, &str)]) -> DriverPackage {
        DriverPackage {
            name: "libva-intel-media-driver".to_string(),
            installed_evr: installed.map(str::to_string),
            available: available
                .iter()
                .map(|(evr, arch, repo)| PackageBuild {
                    evr: evr.to_string(),
                    arch: arch.to_string(),
                    repo: repo.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn prefers_x86_64_build() {
        let p = pkg(None, &[("1-1.fc44", "i686", "updates"), ("1-1.fc44", "x86_64", "updates")]);
        assert_eq!(p.preferred_available().unwrap().arch, "x86_64");
    }

    #[test]
    fn falls_back_to_first_build_when_no_x86_64() {
        let p = pkg(None, &[("1-1.fc44", "i686", "updates")]);
        assert_eq!(p.preferred_available().unwrap().arch, "i686");
    }

    #[test]
    fn no_build_when_nothing_available() {
        let p = pkg(None, &[]);
        assert!(p.preferred_available().is_none());
    }

    #[test]
    fn status_text_covers_all_states() {
        assert_eq!(
            pkg(Some("1-1.fc44"), &[("1-1.fc44", "x86_64", "updates")]).status_text(),
            "Installed 1-1.fc44"
        );
        assert_eq!(
            pkg(None, &[("1-1.fc44", "x86_64", "updates")]).status_text(),
            "Available 1-1.fc44 from updates"
        );
        assert_eq!(pkg(None, &[]).status_text(), "Not found in enabled repositories");
    }
}
