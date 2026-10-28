#!/usr/bin/env bash
set -euo pipefail

echo "==> Building Fedora Driver Manager (Release mode)..."
cargo build --release

echo "==> Installing executable to /usr/local/bin..."
sudo install -Dm755 target/release/fedora-driver-manager /usr/local/bin/fedora-driver-manager

echo "==> Installing desktop launcher..."
sudo install -Dm644 data/io.github.shridhargsabat.DriverManager.desktop /usr/local/share/applications/io.github.shridhargsabat.DriverManager.desktop

echo "==> Installing AppStream metadata..."
sudo install -Dm644 data/io.github.shridhargsabat.DriverManager.metainfo.xml /usr/local/share/metainfo/io.github.shridhargsabat.DriverManager.metainfo.xml

echo "==> Installing icon..."
sudo install -Dm644 data/io.github.shridhargsabat.DriverManager.svg /usr/local/share/icons/hicolor/scalable/apps/io.github.shridhargsabat.DriverManager.svg

echo "==> Installing PolicyKit action policy..."
sudo install -Dm644 data/io.github.shridhargsabat.DriverManager.policy /usr/share/polkit-1/actions/io.github.shridhargsabat.DriverManager.policy

echo "==> Installing D-Bus service file..."
# The repo service file points at /usr/bin (RPM layout); script installs
# land in /usr/local/bin, so rewrite Exec= or D-Bus activation fails.
tmp_service="$(mktemp)"
sed 's|^Exec=.*|Exec=/usr/local/bin/fedora-driver-manager|' data/io.github.shridhargsabat.DriverManager.service > "$tmp_service"
sudo install -Dm644 "$tmp_service" /usr/local/share/dbus-1/services/io.github.shridhargsabat.DriverManager.service
rm -f "$tmp_service"

echo "==> Updating desktop database..."
sudo update-desktop-database /usr/local/share/applications || true

echo "Fedora Driver Manager installed successfully! You can launch it by running 'fedora-driver-manager' or finding 'Driver Manager' in the applications menu."
