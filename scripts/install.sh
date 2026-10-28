#!/usr/bin/env bash
set -euo pipefail

echo "==> Building Fedora Driver Manager (Release mode)..."
cargo build --release

echo "==> Installing executable to /usr/local/bin..."
sudo install -Dm755 target/release/fedora-driver-manager /usr/local/bin/fedora-driver-manager

echo "==> Installing desktop launcher..."
sudo install -Dm644 data/io.github.shridharsabat.DriverManager.desktop /usr/local/share/applications/io.github.shridharsabat.DriverManager.desktop

echo "==> Installing AppStream metadata..."
sudo install -Dm644 data/io.github.shridharsabat.DriverManager.metainfo.xml /usr/local/share/metainfo/io.github.shridharsabat.DriverManager.metainfo.xml

echo "==> Installing icon..."
sudo install -Dm644 data/io.github.shridharsabat.DriverManager.svg /usr/local/share/icons/hicolor/scalable/apps/io.github.shridharsabat.DriverManager.svg

echo "==> Installing PolicyKit action policy..."
sudo install -Dm644 data/io.github.shridharsabat.DriverManager.policy /usr/share/polkit-1/actions/io.github.shridharsabat.DriverManager.policy

echo "==> Updating desktop database..."
sudo update-desktop-database /usr/local/share/applications || true

echo "Fedora Driver Manager installed successfully! You can launch it by running 'fedora-driver-manager' or finding 'Driver Manager' in the applications menu."
