# FordDiag
> [!NOTE] 
> This is a work in progress, only tested with ford models listed below. Currently is read only (no ability to send anything to the car)

Open-source **Ford** OBD diagnostics for **Linux**, built with **Rust + Tauri 2**.

Target vehicles (v0.1 profiles):

- Ford Focus Mk3 EU 2017 (1.0 EcoBoost)
- Ford Tourneo Connect 2017

Primary adapter: **OBDLink MX+** (Bluetooth STN, HS-CAN + MS-CAN). Generic ELM327 adapters work for HS-CAN / PCM only.

> **Not a FORScan clone.** Protocols are implemented from public UDS/ISO-TP/ELM–STN behaviour and community module addresses. No proprietary binaries are used.

## Screenshots

UI walkthrough (Home, Profiles, ECUs, DTCs, Live, OBD): [docs/screenshots.md](docs/screenshots.md).

## Features (MVP)

- List serial ports and connect to ELM/STN adapters
- Detect STN/OBDLink and enable MS-CAN (`STP 53`)
- Vehicle profiles (YAML) with module CAN IDs
- Scan modules online/offline
- Read VIN, identity DIDs, DTCs
- Dump As-Built DIDs (read-only) and export JSON

**Not in v0.1:** As-Built write, feature bitflip UI, PCM calibration / power unlock.

## Quick start

### Dependencies (Arch / Manjaro)

```bash
sudo pacman -S rust nodejs npm webkit2gtk-4.1 base-devel curl wget openssl \
  appmenu-gtk-module gtk3 libappindicator-gtk3 librsvg libvips
sudo usermod -aG dialout "$USER"   # then re-login
```

See [docs/linux-setup.md](docs/linux-setup.md) for BlueZ / `rfcomm` with the MX+.

### Run

```bash
npm install
npm run tauri dev
```

### Tests

```bash
cargo test -p ford-core
```

## Layout

```
crates/ford-core/   # STN transport, ISO-TP, UDS, profiles, scanner
src-tauri/          # Tauri 2 shell + commands
src/                # React + Tailwind UI
profiles/           # Vehicle YAML profiles
docs/               # Linux adapter setup, screenshots
```

## Safety

Writing to ECUs can brick modules or affect safety systems. This release is **read-oriented**. You are responsible for anything you do on your vehicle. Always keep a battery maintainer connected during diagnostics.

## Android

Same MVP as a Kotlin + Compose app: sibling repo
[`../fordscanner_android`](../fordscanner_android) (Bluetooth SPP → OBDLink MX+).

## License

MIT — see [LICENSE](LICENSE).
