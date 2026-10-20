# Linux setup — OBDLink MX+ and serial adapters

FordDiag talks to adapters over a **serial port**. On Linux that is usually a Bluetooth RFCOMM device (`/dev/rfcomm0`) or USB CDC (`/dev/ttyUSB0`, `/dev/ttyACM0`).

## Permissions (`dialout`)

Your user must read/write the serial device:

```bash
sudo usermod -aG dialout "$USER"
# log out and back in (or reboot)
groups   # should list dialout
```

Without this, connect fails with “Permission denied”.

## OBDLink MX+ (recommended)

The MX+ is an **STN** adapter with native **Ford MS-CAN** support. Body modules (BCM, IPC, …) on Focus / Tourneo 2017 sit on MS-CAN at 125 kbps. Cheap ELM327 clones usually only see **HS-CAN** (PCM).

### Pair with BlueZ

1. Plug the MX+ into the OBD port, ignition ON.
2. Pair:

```bash
bluetoothctl
power on
agent on
default-agent
scan on
# wait until you see OBDLink MX+ / similar
pair AA:BB:CC:DD:EE:FF
trust AA:BB:CC:DD:EE:FF
connect AA:BB:CC:DD:EE:FF
quit
```

3. Bind RFCOMM (Classic Bluetooth SPP):

```bash
sudo rfcomm bind 0 AA:BB:CC:DD:EE:FF
ls -l /dev/rfcomm0
```

4. In FordDiag, select `/dev/rfcomm0` and **Connect**.

To release the binding:

```bash
sudo rfcomm release 0
```

### Baud rate

Default in the app is **115200**. STN devices often accept that after reset; if connect hangs, try unplugging/replugging the adapter and reconnecting Bluetooth.

### MS-CAN

FordDiag detects STN/OBDLink from `ATI` and uses `STP 53` (MS-CAN, ISO 15765, 11-bit, 125 kbps) for modules marked `bus: ms` in the vehicle profile.

## Generic ELM327 adapters

| Feature | MX+ / STN | Typical ELM clone |
|---------|-----------|-------------------|
| HS-CAN / PCM / VIN | Yes | Often yes |
| MS-CAN / BCM / IPC | Yes (`STP 53`) | No, unless physical HS/MS switch + lucky firmware |
| Reliability | High | Variable |

If your clone has a **HS/MS switch**, set it to MS before scanning body modules. FordDiag will still warn when the adapter identity does not look like STN.

## Ignition

Many modules only answer with **ignition ON** (not accessories-only). Keep the engine running or a battery maintainer connected for long scans.

## Troubleshooting

| Symptom | Likely cause |
|---------|----------------|
| No ports listed | MX+ not paired / `rfcomm` not bound / USB not plugged |
| Permission denied | Not in `dialout` |
| Only PCM online | No MS-CAN (use MX+) |
| Timeout / NO DATA | Wrong profile IDs, ignition off, or module absent on that car |
| Bluetooth drops | Stay within range; prefer USB OBDLink EX for long sessions |

## Related commands

```bash
# list serial devices
ls -l /dev/ttyUSB* /dev/ttyACM* /dev/rfcomm* 2>/dev/null

# monitor RFCOMM (debug)
cat /dev/rfcomm0   # then open FordDiag in another terminal — prefer app-only access
```
