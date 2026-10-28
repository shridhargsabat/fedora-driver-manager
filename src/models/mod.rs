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
