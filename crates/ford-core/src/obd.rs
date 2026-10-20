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
}

/// Decode common Mode 01 PIDs into a short human string.
pub fn decode_pid(pid: u8, data: &[u8]) -> Option<String> {
    match pid {
        0x0C if data.len() >= 2 => {
            let rpm = ((u16::from(data[0]) << 8) | u16::from(data[1])) as f32 / 4.0;
            Some(format!("{rpm:.0} rpm"))
        }
        0x0D if !data.is_empty() => Some(format!("{} km/h", data[0])),
        0x05 if !data.is_empty() => Some(format!("{} °C", data[0] as i16 - 40)),
        0x0F if !data.is_empty() => Some(format!("{} °C", data[0] as i16 - 40)),
        0x11 if !data.is_empty() => Some(format!("{:.0}%", data[0] as f32 * 100.0 / 255.0)),
        0x04 if !data.is_empty() => Some(format!("{:.0}%", data[0] as f32 * 100.0 / 255.0)),
        0x42 if data.len() >= 2 => {
            let mv = ((u16::from(data[0]) << 8) | u16::from(data[1])) as f32 / 1000.0;
            Some(format!("{mv:.2} V"))
        }
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

    // Find 41 <pid> …
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == 0x41 && bytes[i + 1] == pid {
            let data = bytes[i + 2..].to_vec();
            return Ok(ObdPidSample {
                pid,
                hex: hex::encode_upper(&data),
                decoded: decode_pid(pid, &data),
                raw: data,
            });
        }
        i += 1;
    }

    // Some adapters strip the service echo — treat whole payload as data if short
    if !bytes.is_empty() && bytes[0] != 0x41 {
        return Ok(ObdPidSample {
            pid,
            hex: hex::encode_upper(&bytes),
            decoded: decode_pid(pid, &bytes),
            raw: bytes,
        });
    }

    Err(Error::UnexpectedResponse(format!(
        "no Mode 01 response for PID {pid:02X}: {resp:?}"
    )))
}

/// Query supported PIDs via 01 00 / 01 20 / 01 40 / 01 60.
pub fn list_supported_pids(transport: &mut dyn Transport) -> Result<Vec<u8>> {
    let mut supported = Vec::new();
    for base in [0x00u8, 0x20, 0x40, 0x60] {
        match read_pid(transport, base) {
            Ok(sample) if sample.raw.len() >= 4 => {
                let more = sample.raw[3] & 0x01 != 0; // bit for next range present in last byte LSB of D
                supported.extend(parse_supported_bitmask(base, &sample.raw[..4]));
                if !more && base > 0 {
                    break;
                }
                // Also stop if next range not indicated — ISO uses PID (base+0x20) support bit
                let next_supported = sample.raw[3] & 0x01 != 0;
                if !next_supported {
                    break;
                }
            }
            Ok(_) | Err(_) => break,
        }
    }
    // Filter out the bitmap PIDs themselves from "live" list preference
    supported.retain(|p| !matches!(p, 0x00 | 0x20 | 0x40 | 0x60));
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
