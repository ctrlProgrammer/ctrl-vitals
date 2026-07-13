# Core Pulse

![screenshot](assets/image.png)

A lightweight, iOS-style system monitor for Linux. Real-time CPU, GPU, RAM, Disk, Network, and process monitoring in a compact draggable widget.

Built with **Rust + GTK4** — ~740KB binary, minimal resource usage.

## Features

- **CPU** — per-core usage, aggregate sparkline (2min history), temperature
- **GPU** — core utilization, VRAM usage, temperature, sparkline (NVIDIA)
- **RAM** — used/total with sparkline
- **Disk** — root partition usage
- **Network** — real-time up/down speed
- **Processes** — top 4 by CPU usage
- **Draggable** — click anywhere to drag, resizable

## Install

### Quick (from release)

```bash
# Download from GitHub Releases
wget https://github.com/YOUR_USER/core-pulse/releases/download/v0.1.0/core-pulse-v0.1.0-linux-x86_64.tar.gz
tar xzf core-pulse-v0.1.0-linux-x86_64.tar.gz
cd core-pulse-v0.1.0-linux-x86_64
./install.sh
```

### From source

```bash
git clone https://github.com/YOUR_USER/core-pulse.git
cd core-pulse
cargo build --release
./scripts/install.sh
```

**Prerequisites:** `libgtk-4-dev` (Ubuntu/Debian) or `gtk4-devel` (Fedora).

## Usage

Run `core-pulse` or launch from your application menu (search "Core Pulse").

- **Drag** anywhere on the widget to move it
- **Resize** by dragging the edges
- **Close** with `Ctrl+C` in the terminal or your window manager's close shortcut

## Requirements

| Component | Required |
|-----------|----------|
| OS | Linux (x86_64) |
| Display | GNOME / KDE / any X11 or Wayland compositor |
| GPU | NVIDIA (for GPU stats) |
| GTK | 4.14+ (runtime) |

## Build Dependencies

| Package | Ubuntu/Debian | Fedora |
|---------|---------------|--------|
| GTK4 dev | `libgtk-4-dev` | `gtk4-devel` |
| Rust | `rustc cargo` | `rust cargo` |

## License

MIT
