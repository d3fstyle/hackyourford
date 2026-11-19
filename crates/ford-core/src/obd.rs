//! OBD-II Mode 01 (current data) helpers over ELM/STN.

use crate::error::{Error, Result};
use crate::transport::Transport;
use serde::{Deserialize, Serialize};

/// A Mode 01 PID sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObdPidSample {
    pub pid: u8,
    pub raw: Vec<u8>,
    pub hex: String,
    pub decoded: Option<String>,
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub label: String,
}

/// SAE J1979 Mode 01 display names (Torque table + short overrides for common PIDs).
pub static MODE01_NAMES: &[&str] = &[
    "",                                                                 // 0x00
    "Number of trouble codes and I/M info",                             // 0x01
    "Freeze DTC",                                                       // 0x02
    "Fuel system status",                                               // 0x03
    "Engine load",                                                      // 0x04
    "Coolant temp",                                                     // 0x05
    "Short term fuel trim B1",                                          // 0x06
    "Long term fuel trim B1",                                           // 0x07
    "Short term fuel trim B2",                                          // 0x08
    "Long term fuel trim B2",                                           // 0x09
    "Fuel pressure",                                                    // 0x0A
    "MAP",                                                              // 0x0B
    "RPM",                                                              // 0x0C
    "Speed",                                                            // 0x0D
    "Timing advance",                                                   // 0x0E
    "Intake air temp",                                                  // 0x0F
    "MAF",                                                              // 0x10
    "Throttle",                                                         // 0x11
    "Sec. air status",                                                  // 0x12
    "Oxygen sensors present",                                           // 0x13
    "Bank 1, Sensor 1:Oxygen sensor & Short Term Fuel Trim",           // 0x14
    "Bank 1, Sensor 2:Oxygen sensor & Short Term Fuel Trim",           // 0x15
    "Bank 1, Sensor 3:Oxygen sensor & Short Term Fuel Trim",           // 0x16
    "Bank 1, Sensor 4:Oxygen sensor & Short Term Fuel Trim",           // 0x17
    "Bank 2, Sensor 1:Oxygen sensor & Short Term Fuel Trim",           // 0x18
    "Bank 2, Sensor 2:Oxygen sensor & Short Term Fuel Trim",           // 0x19
    "Bank 2, Sensor 3:Oxygen sensor & Short Term Fuel Trim",           // 0x1A
    "Bank 2, Sensor 4:Oxygen sensor & Short Term Fuel Trim",           // 0x1B
    "OBD standards this vehicle conforms to",                           // 0x1C
    "Oxygen sensors present",                                           // 0x1D
    "Auxiliary input status",                                           // 0x1E
    "Run time",                                                         // 0x1F
    "",                                                                 // 0x20
    "Distance with MIL",                                                // 0x21
    "Fuel Pressure relative to vacuum",                                 // 0x22
    "Fuel Rail Pressure",                                               // 0x23
    "O2 S1 Equiv. Ratio and/or Voltage",                               // 0x24
    "O2 S2 Equiv. Ratio and/or Voltage",                               // 0x25
    "O2 S3 Equiv. Ratio and/or Voltage",                               // 0x26
    "O2 S4 Equiv. Ratio and/or Voltage",                               // 0x27
    "O2 S5 Equiv. Ratio and/or Voltage",                               // 0x28
    "O2 S6 Equiv. Ratio and/or Voltage",                               // 0x29
    "O2 S7 Equiv. Ratio and/or Voltage",                               // 0x2A
    "O2 S8 Equiv. Ratio and/or Voltage",                               // 0x2B
    "Commanded EGR",                                                    // 0x2C
    "EGR Error",                                                        // 0x2D
    "Commanded evaporative purge",                                      // 0x2E
    "Fuel level",                                                       // 0x2F
    "Number of warm-ups since codes cleared",                           // 0x30
    "Distance traveled since codes cleared",                            // 0x31
    "Evap. System Vapor Pressure",                                      // 0x32
    "Barometric pressure",                                              // 0x33
    "O2 S1 Equiv. Ratio and/or Current",                               // 0x34
    "O2 S2 Equiv. Ratio and/or Current",                               // 0x35
    "O2 S3 Equiv. Ratio and/or Current",                               // 0x36
    "O2 S4 Equiv. Ratio and/or Current",                               // 0x37
    "O2 S5 Equiv. Ratio and/or Current",                               // 0x38
    "O2 S6 Equiv. Ratio and/or Current",                               // 0x39
    "O2 S7 Equiv. Ratio and/or Current",                               // 0x3A
    "O2 S8 Equiv. Ratio and/or Current",                               // 0x3B
    "Catalyst Temp - Bank 1, Sensor 1",                                 // 0x3C
    "Catalyst Temp - Bank 2, Sensor 1",                                 // 0x3D
    "Catalyst Temp - Bank 1, Sensor 2",                                 // 0x3E
    "Catalyst Temp - Bank 2, Sensor 2",                                 // 0x3F
    "",                                                                 // 0x40
    "Monitor status this drive cycle",                                  // 0x41
    "Control module voltage",                                           // 0x42
    "Absolute load",                                                    // 0x43
    "Command equivalence ratio",                                        // 0x44
    "Relative throttle",                                                // 0x45
    "Ambient air temp",                                                 // 0x46
    "Absolute throttle position B",                                     // 0x47
    "Absolute throttle position C",                                     // 0x48
    "Accelerator pedal position D",                                     // 0x49
    "Accelerator pedal position E",                                     // 0x4A
    "Accelerator pedal position F",                                     // 0x4B
    "Commanded throttle actuator",                                      // 0x4C
    "Time run with MIL on",                                             // 0x4D
    "Time since trouble codes cleared",                                 // 0x4E
    "Max val for equivalence ratio, oxygen sensor voltage, oxygen sensor current, and intake manifold absolute pressure", // 0x4F
    "Max val for air flow rate from mass air flow sensor",              // 0x50
    "Fuel Type",                                                        // 0x51
    "Ethanol fuel%",                                                    // 0x52
    "Absoulute Evap system Vapour Pressure",                            // 0x53
    "Evap system vapor pressure",                                       // 0x54
    "Short term secondary oxygen sensor trim bank 1 and bank 3",        // 0x55
    "Long term secondary oxygen sensor trim bank 1 and bank 3",         // 0x56
    "Short term secondary oxygen sensor trim bank 2 and bank 4",        // 0x57
    "Long term secondary oxygen sensor trim bank 2 and bank 4",         // 0x58
    "Fuel rail pressure (absolute)",                                    // 0x59
    "Relative accelerator pedal position",                              // 0x5A
    "Hybrid battery pack remaining life",                               // 0x5B
    "Oil temp",                                                         // 0x5C
    "Fuel injection timing",                                            // 0x5D
    "Fuel rate",                                                        // 0x5E
    "Emission requirements to which vehicle is designed",               // 0x5F
    "",                                                                 // 0x60
    "Driver's demand engine - percent torque",                          // 0x61
    "Actual engine - percent torque",                                   // 0x62
    "Engine reference torque",                                          // 0x63
    "Engine percent torque data",                                       // 0x64
    "Auxiliary input / output supported",                               // 0x65
    "Mass air flow sensor",                                             // 0x66
    "Engine coolant temperature",                                       // 0x67
    "Intake air temperature sensor",                                    // 0x68
    "Commanded EGR and EGR Error",                                      // 0x69
    "Commanded Diesel intake air flow control and relative intake air flow position", // 0x6A
    "Exhaust gas recirculation temperature",                            // 0x6B
    "Commanded throttle actuator control and relative throttle position", // 0x6C
    "Fuel pressure control system",                                     // 0x6D
    "Injection pressure control system",                                // 0x6E
    "Turbocharger compressor inlet pressure",                           // 0x6F
    "Boost pressure control",                                           // 0x70
    "Variable Geometry turbo (VGT) control",                            // 0x71
    "Wastegate control",                                                // 0x72
    "Exhaust pressure",                                                 // 0x73
    "Turbocharger RPM",                                                 // 0x74
    "Turbocharger A temperature",                                       // 0x75
    "Turbocharger B temperature",                                       // 0x76
    "Charge air cooler temperature (CACT)",                             // 0x77
    "Exhaust Gas temperature (EGT) Bank 1",                             // 0x78
    "Exhaust Gas temperature (EGT) Bank 2",                             // 0x79
    "Diesel particulate filter (DPF) Bank 1",                           // 0x7A
    "Diesel particulate filter (DPF) Bank 2",                           // 0x7B
    "Diesel Particulate filter (DPF) temperature",                      // 0x7C
    "NOx NTE control area status",                                      // 0x7D
    "PM NTE control area status",                                       // 0x7E
    "Engine run time",                                                  // 0x7F
    "",                                                                 // 0x80
    "Engine run time for AECD #1 - #5",                                 // 0x81
    "Engine run time for AECD #6 - #10",                                // 0x82
    "NOx sensor",                                                       // 0x83
    "Manifold surface temperature",                                     // 0x84
    "NOx control system",                                               // 0x85
    "Particulate matter (PM) sensor",                                   // 0x86
    "Intake manifold absolute pressure",                                // 0x87
    "SCR inducement system actual state",                               // 0x88
    "Engine Run Time for AECD #11 - #15",                               // 0x89
    "Engine Run Time for AECD #16 - #20",                               // 0x8A
    "Diesel Aftertreatment Status",                                     // 0x8B
    "O2 Sensor (Wide Range)",                                           // 0x8C
    "Absolute Throttle Position G",                                     // 0x8D
    "Engine Friction - Percent Torque",                                 // 0x8E
    "Particulate Matter (PM) Sensor Output",                            // 0x8F
    "WWH-OBD Vehicle OBD System Information",                           // 0x90
    "WWH-OBD ECU OBD System Information",                               // 0x91
    "Fuel System Control Status(Compression Ignition)",                 // 0x92
    "WWH-OBD Vehicle OBD Counters",                                     // 0x93
    "NOx control - driver inducement system status and counters",       // 0x94
    "",                                                                 // 0x95
    "",                                                                 // 0x96
    "",                                                                 // 0x97
    "Exhaust Gas Temperature (EGT) Bank 1",                             // 0x98
    "Exhaust Gas Temperature (EGT) Bank 2",                             // 0x99
    "Hybrid/EV Vehicle System Data",                                    // 0x9A
    "Diesel Exhaust Fluid Sensor Output",                               // 0x9B
    "O2 Sensor (Wide Range)",                                           // 0x9C
    "Engine Fuel Rate",                                                 // 0x9D
    "Engine Exhaust Flow Rate",                                         // 0x9E
    "Fuel System Percentage Use",                                       // 0x9F
    "",                                                                 // 0xA0
    "NOx Sensor Corrected",                                             // 0xA1
    "Cylinder Fuel Rate",                                               // 0xA2
];

/// Helper to read u8 from data slice.
#[inline]
fn u8_at(data: &[u8], i: usize) -> Option<u8> {
    data.get(i).copied()
}

/// Helper to read u16 big-endian from data slice.
#[inline]
fn u16_at(data: &[u8], i: usize) -> Option<u16> {
    if i + 1 < data.len() {
        Some((u16::from(data[i]) << 8) | u16::from(data[i + 1]))
    } else {
        None
    }
}

/// Decode percent from 0-255 range.
#[inline]
fn pct255(data: &[u8]) -> Option<f64> {
    u8_at(data, 0).map(|v| f64::from(v) * 100.0 / 255.0)
}

/// Decode temperature (A - 40) °C.
#[inline]
fn temp_c(data: &[u8]) -> Option<f64> {
    u8_at(data, 0).map(|v| f64::from(v as i16 - 40))
}

/// Decode fuel trim (A - 128) * 100 / 128.
#[inline]
fn fuel_trim(data: &[u8]) -> Option<f64> {
    u8_at(data, 0).map(|v| f64::from(v as i16 - 128) * 100.0 / 128.0)
}

/// Decode catalyst temp: (A*256 + B) / 10 - 40.
#[inline]
fn catalyst_temp(data: &[u8]) -> Option<f64> {
    u16_at(data, 0).map(|v| f64::from(v) / 10.0 - 40.0)
}

/// Decoded PID result with value, unit, and label.
#[derive(Debug, Clone)]
pub struct ObdPidDecoded {
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub label: String,
    pub text: Option<String>,
}

/// Friendly label for a Mode 01 PID, or fallback to hex.
pub fn pid_label(pid: u8) -> String {
    MODE01_NAMES
        .get(pid as usize)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("PID {:02X}", pid))
}

/// Decode Mode 01 PID into numeric value + display text.
pub fn decode_pid_full(pid: u8, data: &[u8]) -> ObdPidDecoded {
    let label = pid_label(pid);
    
    let (value, unit) = match pid {
        0x04 => (pct255(data), Some("%".to_string())),
        0x05 => (temp_c(data), Some("°C".to_string())),
        0x06 => (fuel_trim(data), Some("%".to_string())),
        0x07 => (fuel_trim(data), Some("%".to_string())),
        0x08 => (fuel_trim(data), Some("%".to_string())),
        0x09 => (fuel_trim(data), Some("%".to_string())),
        0x0A => (u8_at(data, 0).map(|v| f64::from(v) * 3.0), Some("kPa".to_string())),
        0x0B => (u8_at(data, 0).map(|v| f64::from(v)), Some("kPa".to_string())),
        0x0C => (u16_at(data, 0).map(|v| f64::from(v) / 4.0), Some("rpm".to_string())),
        0x0D => (u8_at(data, 0).map(|v| f64::from(v)), Some("km/h".to_string())),
        0x0E => (u8_at(data, 0).map(|v| f64::from(v) / 2.0 - 64.0), Some("°".to_string())),
        0x0F => (temp_c(data), Some("°C".to_string())),
        0x10 => (u16_at(data, 0).map(|v| f64::from(v) / 100.0), Some("g/s".to_string())),
        0x11 => (pct255(data), Some("%".to_string())),
        0x1F => (u16_at(data, 0).map(|v| f64::from(v)), Some("s".to_string())),
        0x21 => (u16_at(data, 0).map(|v| f64::from(v)), Some("km".to_string())),
        0x22 => (u16_at(data, 0).map(|v| f64::from(v) as f64 * 0.079), Some("kPa".to_string())),
        0x23 => (u16_at(data, 0).map(|v| f64::from(v) * 10.0), Some("kPa".to_string())),
        0x2C => (pct255(data), Some("%".to_string())),
        0x2D => (fuel_trim(data), Some("%".to_string())),
        0x2E => (pct255(data), Some("%".to_string())),
        0x2F => (pct255(data), Some("%".to_string())),
        0x30 => (u8_at(data, 0).map(|v| f64::from(v)), None),
        0x31 => (u16_at(data, 0).map(|v| f64::from(v)), Some("km".to_string())),
        0x33 => (u8_at(data, 0).map(|v| f64::from(v)), Some("kPa".to_string())),
        0x3C => (catalyst_temp(data), Some("°C".to_string())),
        0x3D => (catalyst_temp(data), Some("°C".to_string())),
        0x3E => (catalyst_temp(data), Some("°C".to_string())),
        0x3F => (catalyst_temp(data), Some("°C".to_string())),
        0x42 => (u16_at(data, 0).map(|v| f64::from(v) / 1000.0), Some("V".to_string())),
        0x43 => (u16_at(data, 0).map(|v| f64::from(v) * 100.0 / 255.0), Some("%".to_string())),
        0x44 => (u16_at(data, 0).map(|v| f64::from(v) / 32768.0), None),
        0x45 => (pct255(data), Some("%".to_string())),
        0x46 => (temp_c(data), Some("°C".to_string())),
        0x47 => (pct255(data), Some("%".to_string())),
        0x48 => (pct255(data), Some("%".to_string())),
        0x49 => (pct255(data), Some("%".to_string())),
        0x4A => (pct255(data), Some("%".to_string())),
        0x4B => (pct255(data), Some("%".to_string())),
        0x4C => (pct255(data), Some("%".to_string())),
        0x4D => (u16_at(data, 0).map(|v| f64::from(v)), Some("min".to_string())),
        0x4E => (u16_at(data, 0).map(|v| f64::from(v)), Some("min".to_string())),
        0x52 => (pct255(data), Some("%".to_string())),
        0x59 => (u16_at(data, 0).map(|v| f64::from(v) * 10.0), Some("kPa".to_string())),
        0x5A => (pct255(data), Some("%".to_string())),
        0x5B => (pct255(data), Some("%".to_string())),
        0x5C => (temp_c(data), Some("°C".to_string())),
        0x5D => (u16_at(data, 0).map(|v| f64::from(v) / 128.0 - 210.0), Some("°".to_string())),
        0x5E => (u16_at(data, 0).map(|v| f64::from(v) / 20.0), Some("L/h".to_string())),
        0x61 => (u8_at(data, 0).map(|v| f64::from(v as i16 - 125)), Some("%".to_string())),
        0x62 => (u8_at(data, 0).map(|v| f64::from(v as i16 - 125)), Some("%".to_string())),
        0x63 => (u16_at(data, 0).map(|v| f64::from(v)), Some("Nm".to_string())),
        0x8D => (pct255(data), Some("%".to_string())),
        0x8E => (u8_at(data, 0).map(|v| f64::from(v as i16 - 125)), Some("%".to_string())),
        _ => (raw_numeric_did(data), None),
    };

    let text = value.and_then(|v| {
        let formatted = if v == v.trunc() {
            format!("{:.0}", v)
        } else {
            format!("{:.2}", v)
        };
        unit.as_ref().map(|u| {
            if u.is_empty() {
                formatted.clone()
            } else {
                format!("{} {}", formatted, u)
            }
        }).or(Some(formatted))
    });

    ObdPidDecoded {
        value,
        unit,
        label,
        text,
    }
}

/// Decode common Mode 01 PIDs into a short human string (compatibility).
pub fn decode_pid(pid: u8, data: &[u8]) -> Option<String> {
    decode_pid_full(pid, data).text
}

/// Best-effort numeric from short DID payloads (1-2 bytes as unsigned).
/// Longer payloads return None (show hex/ascii in UI instead).
pub fn raw_numeric_did(data: &[u8]) -> Option<f64> {
    match data.len() {
        1 => Some(f64::from(data[0])),
        2 => u16_at(data, 0).map(|v| f64::from(v)),
        _ => None,
    }
}

/// Parse supported-PID bitmask response (`41 XX` + 4 bytes).
pub fn parse_supported_bitmask(base_pid: u8, payload: &[u8]) -> Vec<u8> {
    // payload is A B C D after 41 XX
    if payload.len() < 4 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (byte_i, byte) in payload.iter().take(4).enumerate() {
        for bit in 0..8 {
            if byte & (0x80 >> bit) != 0 {
                let pid = base_pid.saturating_add(1).saturating_add((byte_i * 8 + bit) as u8);
                out.push(pid);
            }
        }
    }
    out
}

/// Request Mode 01 PID and return data bytes (after `41 PID`).
pub fn read_pid(transport: &mut dyn Transport, pid: u8) -> Result<ObdPidSample> {
    let cmd = format!("01{pid:02X}");
    let resp = transport.send_hex(&cmd)?;
    let bytes = crate::transport::parse_hex_response(&resp)?;
    let bytes = crate::transport::normalize_elm_payload(&bytes);

    let make_sample = |data: Vec<u8>| -> ObdPidSample {
        let decoded_full = decode_pid_full(pid, &data);
        ObdPidSample {
            pid,
            hex: hex::encode_upper(&data),
            decoded: decoded_full.text.clone(),
            value: decoded_full.value,
            unit: decoded_full.unit.clone(),
            label: decoded_full.label,
            raw: data,
        }
    };

    // Find 41 <pid> …
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0x41 && bytes[i + 1] == pid {
            let data = bytes[i + 2..].to_vec();
            return Ok(make_sample(data));
        }
        i += 1;
    }

    // Some adapters strip the service echo — treat whole payload as data if short
    if !bytes.is_empty() && bytes[0] != 0x41 {
        return Ok(make_sample(bytes));
    }

    Err(Error::UnexpectedResponse(format!(
        "no Mode 01 response for PID {pid:02X}: {resp:?}"
    )))
}

/// Query supported PIDs via 01 00 / 01 20 / 01 40 / 01 60 / 01 80 / 01 A0 / 01 C0.
/// Continues past 0x60 until no next bitmask is indicated, like Android.
pub fn list_supported_pids(transport: &mut dyn Transport) -> Result<Vec<u8>> {
    let mut supported = Vec::new();
    let support_pids = [0x00u8, 0x20, 0x40, 0x60, 0x80, 0xA0, 0xC0];
    
    for &base in &support_pids {
        match read_pid(transport, base) {
            Ok(sample) if sample.raw.len() >= 4 => {
                let next_supported = sample.raw[3] & 0x01 != 0;
                supported.extend(parse_supported_bitmask(base, &sample.raw[..4]));
                if !next_supported {
                    break;
                }
            }
            Ok(_) | Err(_) => break,
        }
    }
    // Filter out the bitmap PIDs themselves from "live" list preference
    supported.retain(|p| !support_pids.contains(p));
    Ok(supported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    #[test]
    fn decode_rpm() {
        assert_eq!(decode_pid(0x0C, &[0x1A, 0xF8]).as_deref(), Some("1726 rpm"));
    }

    #[test]
    fn bitmask_01_00() {
        // classic example: PIDs 01-20 support bits
        let bits = [0xBE, 0x1F, 0xA8, 0x13];
        let pids = parse_supported_bitmask(0x00, &bits);
        assert!(pids.contains(&0x0C));
        assert!(pids.contains(&0x0D));
    }

    #[test]
    fn read_pid_mock() {
        let mut t = MockTransport::new();
        t.expect("010C", "41 0C 1A F8");
        let s = read_pid(&mut t, 0x0C).unwrap();
        assert_eq!(s.pid, 0x0C);
        assert!(s.decoded.as_ref().unwrap().contains("rpm"));
    }
}
