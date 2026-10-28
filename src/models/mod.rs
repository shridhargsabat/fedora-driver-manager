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

/// One pending driver-package upgrade: installed EVR → repo EVR.
/// Sourced from `dnf5 check-upgrade --json` (libdnf5 compares EVRs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageUpdate {
    pub name: String,
    pub arch: String,
    pub from_evr: String,
    pub to_evr: String,
    pub repo: String,
}

impl PackageUpdate {
    /// "old → new (repo)" for list subtitles and notifications.
    pub fn summary(&self) -> String {
        format!("{} → {} ({})", self.from_evr, self.to_evr, self.repo)
    }

    /// Stable identity for arrival/dismissal diffing across checks.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.name, self.arch, self.to_evr)
    }
}

/// Pure helpers for comparing update sets between checks.
pub struct UpdateSet;

impl UpdateSet {
    /// Names present in `new` whose key was absent from `old`.
    /// Drives "new updates available" notifications (no spam on re-scan).
    pub fn newly_arrived(old: &[PackageUpdate], new: &[PackageUpdate]) -> Vec<String> {
        let old_keys: std::collections::HashSet<String> = old.iter().map(|u| u.key()).collect();
        new.iter()
            .filter(|u| !old_keys.contains(&u.key()))
            .map(|u| u.name.clone())
            .collect()
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

    #[test]
    fn package_update_summary_line() {
        let u = PackageUpdate {
            name: "akmod-nvidia".to_string(),
            arch: "x86_64".to_string(),
            from_evr: "3:615.71.09-2.fc44".to_string(),
            to_evr: "3:615.71.09-3.fc44".to_string(),
            repo: "rpmfusion-nonfree-nvidia-driver".to_string(),
        };
        assert_eq!(u.summary(), "3:615.71.09-2.fc44 → 3:615.71.09-3.fc44 (rpmfusion-nonfree-nvidia-driver)");
    }

    #[test]
    fn update_set_detects_new_arrivals() {
        let mk = |n: &str| PackageUpdate {
            name: n.to_string(),
            arch: "x86_64".to_string(),
            from_evr: "1".to_string(),
            to_evr: "2".to_string(),
            repo: "updates".to_string(),
        };
        let old = vec![mk("a")];
        let new = vec![mk("a"), mk("b")];
        assert_eq!(UpdateSet::newly_arrived(&old, &new), vec!["b".to_string()]);
        assert!(UpdateSet::newly_arrived(&new, &old).is_empty());
        assert!(UpdateSet::newly_arrived(&old, &old).is_empty());
    }
}
