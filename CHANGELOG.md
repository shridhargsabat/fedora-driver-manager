# Changelog

All notable changes to Driver Manager. Format: `## [version] - date`
followed by `- bullet` lines (parsed by the in-app What's New page).

## [0.4.1] - 2026-10-29

- Compact update rows: short action labels, single-line titles
- Live install progress dialog with streaming dnf output (button locked while running)

## [0.4.0] - 2026-10-29

- Self-update check: Driver Manager updates appear in the Updates page with Install/Download

## [0.3.0] - 2026-10-28

- New Updates page: pending driver upgrades with Install and Download-only options
- Automatic update checks with desktop notifications (header menu toggle)
- What's New dialog uses expandable release rows

## [0.2.1] - 2026-10-28

- What's New moved from sidebar to header menu dialog (next to About)
- New Reinstall button for installed driver packages (repair via dnf5 reinstall)

## [0.2.0] - 2026-10-28

- New Available Drivers page: installed/available repo versions with one-click install
- New What's New page: current release notes plus full history

## [0.1.4] - 2026-10-28

- Security: root dnf5 commands pinned to absolute /usr/bin/dnf5 (no PATH resolution as root)
- Failed lspci/lsusb scans now surface errors instead of false Optimal
- Script installs rewrite the D-Bus Exec path so launcher activation works
- Single-window guard hardened; pkexec exit codes shown in errors; honest device counts

## [0.1.3] - 2026-10-28

- New About dialog (header menu): version, developer, license, links

## [0.1.2] - 2026-10-28

- Fixed app-grid launch (ship D-Bus service file for DBusActivatable)
- Fixed double window on launch (reuse existing window on re-activate)

## [0.1.1] - 2026-10-28

- 25 bug fixes: working Refresh, correct GPU/Thunderbolt labels, Other Devices page
- dnf history via structured JSON; Undo/Install confirmations; no UI freezes
- Honest error states; 38 unit tests; clippy clean

## [0.1.0] - 2026-10-28

- Initial release: hardware/driver overview, firmware via fwupd, dnf rollback
- Software Centre metadata: desktop file, icon, AppStream metainfo
