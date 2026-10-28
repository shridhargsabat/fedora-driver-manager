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

/// One `## [x.y.z] - date` section of CHANGELOG.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseNotes {
    pub version: String,
    pub date: String,
    pub bullets: Vec<String>,
}

/// Parse CHANGELOG.md (the `## [version] - date` + `- bullet` format).
/// Unknown lines are ignored; an empty file yields no notes (never an error,
// so the UI always has something graceful to show).
pub fn parse_changelog(text: &str) -> Vec<ReleaseNotes> {
    let mut notes = Vec::new();
    let mut current: Option<ReleaseNotes> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("## [") {
            if let Some(end) = rest.find(']') {
                if let Some(date) = rest[end + 1..].strip_prefix(" - ") {
                    if let Some(done) = current.take() {
                        notes.push(done);
                    }
                    current = Some(ReleaseNotes {
                        version: rest[..end].to_string(),
                        date: date.trim().to_string(),
                        bullets: Vec::new(),
                    });
                    continue;
                }
            }
        }
        if let Some(bullet) = line.strip_prefix("- ") {
            if let Some(ref mut rel) = current {
                rel.bullets.push(bullet.to_string());
            }
        }
    }
    if let Some(done) = current.take() {
        notes.push(done);
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANGELOG_FIXTURE: &str = "# Changelog\n\n## [0.2.0] - 2026-11-01\n\n- New thing\n- Other thing\n\n## [0.1.0] - 2026-10-28\n\n- Initial release\n";

    #[test]
    fn parses_releases_newest_first() {
        let notes = parse_changelog(CHANGELOG_FIXTURE);
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].version, "0.2.0");
        assert_eq!(notes[0].date, "2026-11-01");
        assert_eq!(notes[0].bullets, vec!["New thing".to_string(), "Other thing".to_string()]);
        assert_eq!(notes[1].version, "0.1.0");
    }

    #[test]
    fn ignores_non_bullet_lines() {
        let notes = parse_changelog("# Title\nSome prose\n## [1.0.0] - 2026-01-01\nNot a bullet\n- Real bullet\n");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].bullets, vec!["Real bullet".to_string()]);
    }

    #[test]
    fn empty_changelog_yields_no_notes() {
        assert!(parse_changelog("").is_empty());
        assert!(parse_changelog("# Changelog\n").is_empty());
    }
}
