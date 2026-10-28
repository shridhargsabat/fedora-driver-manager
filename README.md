# Fedora Driver Manager

A modern, native hardware driver, firmware, and rollback manager for **Fedora Workstation**, built in Rust using GTK 4 and Libadwaita.

![Fedora Driver Manager](data/io.github.shridharsabat.DriverManager.svg)

---

## Features

- **Comprehensive Hardware Detection**:
  - Automatically identifies PCIe devices (`lspci -nnk`) and USB components (`lsusb`).
  - Categorizes components into Graphics, Wireless/Networking, Sound/Audio, NVMe/Storage, and Peripherals.
  - Inspects active kernel drivers in use and compatible kernel modules.
- **Intelligent Recommendations**:
  - Intel UHD / Iris graphics media acceleration driver suggestions (`libva-intel-media-driver`).
  - NVIDIA discrete GPU detection with direct links to RPM Fusion's `akmod-nvidia`.
  - Broadcom and Realtek wireless card proprietary module detection.
- **Firmware Management via `fwupd`**:
  - Live inspection of hardware firmware (BIOS, Intel ME, SSDs, TPM, Cameras).
  - Flags components with updatable firmware.
- **Safe Rollbacks via DNF5**:
  - Displays recent DNF5 transaction history.
  - Allows one-click undo of package installations or driver updates using `pkexec`.
- **System Integration**:
  - Shows real-time Secure Boot status and hardware model (e.g., Lenovo IdeaPad).
  - Modern Libadwaita navigation split view that adapts to GNOME dark/light preferences.

---

## Building & Installation

### Requirements (Fedora Workstation)

```bash
sudo dnf install -y rust cargo gtk4-devel libadwaita-devel gcc pkg-config
```

### Installation

Clone and run the installation script:

```bash
cd /home/shridharsabat/tools/drivers
./scripts/install.sh
```

Or run manually:

```bash
cargo run --release
```

---

## License

GPL-3.0-or-later
