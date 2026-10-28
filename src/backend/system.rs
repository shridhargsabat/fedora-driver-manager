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

        Self::ensure_command_success("fwupdmgr", &output)?;
        Self::parse_firmware_json(&String::from_utf8_lossy(&output.stdout))
    }

    /// Fail with stderr context when a helper tool exits non-zero, so
    /// callers can tell "tool failed" apart from "genuine empty result".
    fn ensure_command_success(tool: &str, output: &std::process::Output) -> Result<()> {
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        match output.status.code() {
            Some(code) if !stderr.is_empty() => {
                anyhow::bail!("{tool} failed with exit code {code}: {stderr}")
            }
            Some(code) => anyhow::bail!("{tool} failed with exit code {code}"),
            None => anyhow::bail!("{tool} terminated by signal"),
        }
    }

    /// Parse `fwupdmgr get-devices --json` output. A missing or empty
    /// `Devices` list is a genuine empty result (`Ok(vec![])`), never an
    /// error.
    fn parse_firmware_json(stdout: &str) -> Result<Vec<FirmwareItem>> {
        let val: serde_json::Value = serde_json::from_str(stdout)?;
        let mut items = Vec::new();

        if let Some(devices) = val.get("Devices").and_then(|d| d.as_array()) {
            for dev in devices {
                // Skip entries with neither Name nor Vendor (e.g. the GPIO
                // plugin device): they would render as meaningless
                // "Unknown Device / Unknown Vendor / N/A" rows.
                let has_name = dev
                    .get("Name")
                    .and_then(|n| n.as_str())
                    .is_some_and(|s| !s.trim().is_empty());
                let has_vendor = dev
                    .get("Vendor")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.trim().is_empty());
                if !has_name && !has_vendor {
                    continue;
                }

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
            .args(["history", "list", "--json"])
            .output()?;

        Self::ensure_command_success("dnf5", &output)?;
        Self::parse_dnf_history_json(&String::from_utf8_lossy(&output.stdout), limit)
    }

    /// Parse `dnf5 history list --json` output (newest transaction first).
    /// Whitespace-splitting the human-readable table corrupted fields, so the
    /// structured form is the source of truth for id, command line and date.
    fn parse_dnf_history_json(stdout: &str, limit: usize) -> Result<Vec<DnfTransaction>> {
        let entries: Vec<DnfHistoryEntry> = serde_json::from_str(stdout)?;
        Ok(entries
            .into_iter()
            .take(limit)
            .map(|entry| {
                let command_line = entry.command_line.unwrap_or_default();
                DnfTransaction {
                    id: entry.id,
                    action: action_from_command_line(&command_line),
                    date: entry
                        .start_time
                        .map(format_unix_time)
                        .unwrap_or_else(|| "Unknown".to_string()),
                    command_line,
                }
            })
            .collect())
    }
}

/// Privileged operations allowed to run as root via pkexec. The closed set
/// of variants (plus package-name validation) guarantees only known argv
/// shapes can ever reach `/usr/bin/pkexec`.
#[derive(Debug, Clone)]
pub enum RootAction {
    DnfInstall { package: String },
    HistoryUndo { id: u64 },
}

impl RootAction {
    /// `dnf5 install -y <package>`, rejecting anything that is not a plain
    /// package name (`^[A-Za-z0-9][A-Za-z0-9.+_-]*$`) before pkexec is reached.
    pub fn dnf_install(package: &str) -> Result<Self> {
        if valid_package_name(package) {
            Ok(Self::DnfInstall {
                package: package.to_string(),
            })
        } else {
            anyhow::bail!("invalid package name: {package:?}")
        }
    }

    pub fn history_undo(id: u64) -> Self {
        Self::HistoryUndo { id }
    }

    /// Exact argv (after pkexec) for this action.
    fn argv(&self) -> Vec<String> {
        match self {
            Self::DnfInstall { package } => {
                vec!["dnf5".to_string(), "install".to_string(), "-y".to_string(), package.clone()]
            }
            Self::HistoryUndo { id } => vec![
                "dnf5".to_string(),
                "history".to_string(),
                "undo".to_string(),
                "-y".to_string(),
                id.to_string(),
            ],
        }
    }
}

/// Plain package names only: first char alphanumeric, the rest
/// alphanumeric or `.+_-`. Equivalent to
/// `^[A-Za-z0-9][A-Za-z0-9.+_-]*$` without regex overhead.
fn valid_package_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-' | '_'))
}

impl SystemBackend {
    /// Async variant of [`Self::execute_root`] that runs the blocking pkexec
    /// subprocess on a worker thread, keeping the GTK main thread responsive.
    /// Callers still `await` it from the main thread, so result toasts behave
    /// exactly as before.
    pub async fn execute_root_async(action: RootAction) -> Result<String> {
        gio::spawn_blocking(move || Self::execute_root(&action))
            .await
            .map_err(|_| anyhow::anyhow!("pkexec background task did not complete"))?
    }

    /// Run a validated [`RootAction`] as root via absolute-path pkexec.
    pub fn execute_root(action: &RootAction) -> Result<String> {
        // Absolute path: never resolve a setuid-root helper via PATH.
        let output = Command::new("/usr/bin/pkexec").args(action.argv()).output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!("Command failed: {}", stderr);
        }

        Ok(stdout)
    }
}

/// One entry of `dnf5 history list --json`. Only the fields we need are
/// modeled; everything else is ignored for forward compatibility.
#[derive(Debug, serde::Deserialize)]
struct DnfHistoryEntry {
    id: u64,
    #[serde(default)]
    command_line: Option<String>,
    #[serde(default)]
    start_time: Option<i64>,
}

/// Derive a short action label ("install", "remove", ...) from a full
/// command line. Non-dnf invocations (e.g. `dnf5daemon-server`) keep their
/// program name.
fn action_from_command_line(cmd: &str) -> String {
    let mut words = cmd.split_whitespace();
    let program = words.next().unwrap_or("");
    let base = program.rsplit('/').next().unwrap_or(program);
    if matches!(base, "dnf" | "dnf5") {
        for word in words {
            if !word.starts_with('-') {
                return word.to_string();
            }
        }
    } else if !base.is_empty() {
        return base.to_string();
    }
    "Transaction".to_string()
}

/// Format a Unix timestamp as `YYYY-MM-DD HH:MM` (UTC) without extra
/// dependencies.
fn format_unix_time(timestamp: i64) -> String {
    let (year, month, day) = civil_from_days(timestamp.div_euclid(86_400));
    let secs = timestamp.rem_euclid(86_400);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        year,
        month,
        day,
        secs / 3600,
        (secs % 3600) / 60
    )
}

/// Days-since-epoch to civil date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim `dnf5 history list --json` entries captured on this machine.
    const DNF_HISTORY_FIXTURE: &str = r#"[
  {
    "id": 66,
    "command_line": "dnf reinstall -y fedora-driver-manager",
    "start_time": 1793170896,
    "end_time": 1793170897,
    "user_id": 1000,
    "status": "Ok",
    "releasever": "44",
    "altered_count": 2
  },
  {
    "id": 65,
    "command_line": "dnf install -y fedora-driver-manager",
    "start_time": 1793170851,
    "end_time": 1793170852,
    "user_id": 1000,
    "status": "Ok",
    "releasever": "44",
    "altered_count": 1
  },
  {
    "id": 64,
    "command_line": "dnf install -y gh copr-cli fedpkg cargo-rpm-macros rpm-build rpmdevtools rpmlint",
    "start_time": 1793167025,
    "end_time": 1793167030,
    "user_id": 1000,
    "status": "Ok",
    "releasever": "44",
    "altered_count": 209
  }
]"#;

    #[test]
    fn parses_id_command_line_and_date() {
        let txs =
            SystemBackend::parse_dnf_history_json(DNF_HISTORY_FIXTURE, 10).expect("fixture must parse");
        assert_eq!(txs.len(), 3);
        assert_eq!(txs[0].id, 66);
        assert_eq!(txs[0].command_line, "dnf reinstall -y fedora-driver-manager");
        assert_eq!(txs[0].date, "2026-10-28 07:01");
        assert_eq!(txs[0].action, "reinstall");
        assert_eq!(txs[1].id, 65);
        assert_eq!(txs[1].command_line, "dnf install -y fedora-driver-manager");
        assert_eq!(txs[1].date, "2026-10-28 07:00");
        assert_eq!(txs[1].action, "install");
    }

    #[test]
    fn long_commands_are_not_truncated() {
        let txs =
            SystemBackend::parse_dnf_history_json(DNF_HISTORY_FIXTURE, 10).expect("fixture must parse");
        assert_eq!(
            txs[2].command_line,
            "dnf install -y gh copr-cli fedpkg cargo-rpm-macros rpm-build rpmdevtools rpmlint"
        );
        assert_eq!(txs[2].date, "2026-10-28 05:57");
    }

    #[test]
    fn respects_limit() {
        let txs =
            SystemBackend::parse_dnf_history_json(DNF_HISTORY_FIXTURE, 2).expect("fixture must parse");
        assert_eq!(txs.len(), 2);
        assert_eq!(txs[0].id, 66);
    }

    #[test]
    fn formats_unix_time() {
        assert_eq!(format_unix_time(1793170896), "2026-10-28 07:01");
        assert_eq!(format_unix_time(0), "1970-01-01 00:00");
    }

    /// Machine-local check: the real `dnf5 history list --json` output on this
    /// system must parse. Ignored by default (needs dnf5 + system history).
    #[test]
    #[ignore]
    fn live_dnf_history_parses() {
        let txs = SystemBackend::get_dnf_history(3).expect("dnf5 must run");
        assert!(!txs.is_empty(), "expected at least one transaction");
        assert_ne!(txs[0].date, "Recent");
        assert!(!txs[0].command_line.is_empty());
    }

    #[test]
    fn derives_action_from_command_line() {
        assert_eq!(action_from_command_line("dnf install -y foo"), "install");
        assert_eq!(action_from_command_line("/usr/bin/dnf remove -y foo"), "remove");
        assert_eq!(action_from_command_line("dnf5daemon-server"), "dnf5daemon-server");
        assert_eq!(action_from_command_line(""), "Transaction");
    }

    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    fn test_output(code: i32, stdout: &str, stderr: &str) -> Output {
        // from_raw takes a raw waitpid status; the exit code lives in bits 8..16.
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn successful_exit_passes_through() {
        let out = test_output(0, "ok", "");
        assert!(SystemBackend::ensure_command_success("fwupdmgr", &out).is_ok());
    }

    #[test]
    fn failing_exit_returns_stderr_context() {
        let out = test_output(1, "", "No such device");
        let err = SystemBackend::ensure_command_success("fwupdmgr", &out).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("fwupdmgr"), "missing tool name: {msg}");
        assert!(msg.contains("No such device"), "missing stderr: {msg}");
    }

    #[test]
    fn failing_exit_without_stderr_still_errors() {
        let out = test_output(2, "", "");
        let err = SystemBackend::ensure_command_success("dnf5", &out).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("dnf5"), "missing tool name: {msg}");
        assert!(msg.contains('2'), "missing exit code: {msg}");
    }

    /// Representative `fwupdmgr get-devices --json` payload (trimmed).
    const FWUPD_FIXTURE: &str = r#"{
  "Devices": [
    {
      "Name": "System Firmware",
      "Vendor": "Lenovo",
      "Version": "1.45",
      "Summary": "UEFI firmware",
      "Flags": ["updatable", "internal"]
    },
    {
      "Name": "NVMe SSD",
      "Flags": []
    }
  ]
}"#;

    #[test]
    fn parses_firmware_devices() {
        let items = SystemBackend::parse_firmware_json(FWUPD_FIXTURE).expect("fixture must parse");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "System Firmware");
        assert_eq!(items[0].vendor, "Lenovo");
        assert_eq!(items[0].version, "1.45");
        assert!(items[0].updatable);
        assert_eq!(items[1].vendor, "Unknown Vendor");
        assert_eq!(items[1].version, "N/A");
        assert!(!items[1].updatable);
    }

    #[test]
    fn empty_firmware_list_is_ok_empty() {
        let items =
            SystemBackend::parse_firmware_json(r#"{"Devices": []}"#).expect("empty must parse");
        assert!(items.is_empty());
    }

    #[test]
    fn missing_devices_key_is_ok_empty() {
        let items = SystemBackend::parse_firmware_json(r#"{}"#).expect("missing key must parse");
        assert!(items.is_empty());
    }

    /// fwupd entries with neither Name nor Vendor (e.g. the GPIO plugin
    /// device) carry no displayable identity and must be filtered out.
    const FWUPD_NAMELESS_FIXTURE: &str = r#"{
  "Devices": [
    {
      "Name": "System Firmware",
      "Vendor": "Lenovo",
      "Version": "1.45",
      "Flags": ["updatable"]
    },
    {
      "Plugin": "gpio",
      "Flags": ["internal"],
      "Guid": ["[UUID_PLACEHOLDER_1]"]
    },
    {
      "Name": "  ",
      "Vendor": "",
      "Version": "0.0.1"
    }
  ]
}"#;

    #[test]
    fn nameless_firmware_entries_are_filtered() {
        let items =
            SystemBackend::parse_firmware_json(FWUPD_NAMELESS_FIXTURE).expect("fixture must parse");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "System Firmware");
        assert_eq!(items[0].vendor, "Lenovo");
    }

    #[test]
    fn named_but_vendorless_firmware_entries_are_kept() {
        // The NVMe SSD entry in FWUPD_FIXTURE has a Name but no Vendor or
        // Version: still identifiable, so it must survive filtering.
        let items = SystemBackend::parse_firmware_json(FWUPD_FIXTURE).expect("fixture must parse");
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].name, "NVMe SSD");
        assert_eq!(items[1].vendor, "Unknown Vendor");
    }

    #[test]
    fn legitimate_package_names_validate() {
        for name in [
            "akmod-nvidia",
            "broadcom-wl",
            "libva-intel-media-driver",
            "foo",
            "a",
            "0ad",
            "x264-libs",
            "gtk4-devel",
            "foo.bar_baz+qux-1",
        ] {
            assert!(RootAction::dnf_install(name).is_ok(), "{name} should validate");
        }
    }

    #[test]
    fn malicious_package_names_are_rejected() {
        for name in [
            "foo;rm",
            "../../../x",
            "a b",
            "",
            "-foo",
            ".foo",
            "foo/bar",
            "foo$bar",
            "foo`id`",
            "foo\nbar",
            "foo|tee",
            "foo&",
            "foo=bar",
        ] {
            assert!(RootAction::dnf_install(name).is_err(), "{name:?} should be rejected");
        }
    }

    #[test]
    fn root_action_argv_shapes_are_fixed() {
        assert_eq!(
            RootAction::dnf_install("akmod-nvidia").expect("valid").argv(),
            ["dnf5", "install", "-y", "akmod-nvidia"]
        );
        assert_eq!(
            RootAction::history_undo(66).argv(),
            ["dnf5", "history", "undo", "-y", "66"]
        );
    }
}
