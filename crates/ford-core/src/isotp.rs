//! ISO-TP helpers.
//!
//! On ELM327/STN in ISO 15765 mode the adapter performs flow control. This module:
//! 1. Formats UDS payloads as hex for the adapter.
//! 2. Reassembles multi-frame payloads when PCI is present.
//! 3. Provides a pure software ISO-TP codec for unit tests.

use crate::error::{Error, Result};
use crate::transport::{normalize_elm_payload, parse_hex_response, Transport};

/// Send a UDS request via an ELM/STN ISO-TP session and return the reassembled payload
/// (UDS bytes, without ISO-TP PCI).
pub fn request(transport: &mut dyn Transport, uds_payload: &[u8]) -> Result<Vec<u8>> {
    let hex = hex::encode_upper(uds_payload);
    let raw = transport.send_hex(&hex)?;
    let bytes = parse_hex_response(&raw)?;
    reassemble_from_elm(&bytes)
}

/// Reassemble ISO-TP from ELM bytes that may include CAN headers and PCI.
pub fn reassemble_from_elm(bytes: &[u8]) -> Result<Vec<u8>> {
    let bytes = normalize_elm_payload(bytes);

    // If response already looks like UDS (positive SID or 0x7F), return as-is.
    if !bytes.is_empty() {
        let b0 = bytes[0];
        if b0 == 0x7F || (0x40..=0x7E).contains(&b0) {
            return Ok(bytes);
        }
    }

    decode_isotp_frames(&bytes)
}

/// Decode a contiguous stream of 8-byte ISO-TP CAN frames into a single payload.
pub fn decode_isotp_frames(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Err(Error::Isotp("empty frame".into()));
    }

    let pci_type = data[0] >> 4;
    match pci_type {
        0 => {
            let len = (data[0] & 0x0F) as usize;
            if data.len() < 1 + len {
                return Err(Error::Isotp(format!(
                    "single frame too short: need {len}, have {}",
                    data.len().saturating_sub(1)
                )));
            }
            Ok(data[1..1 + len].to_vec())
        }
        1 => {
            if data.len() < 2 {
                return Err(Error::Isotp("first frame too short".into()));
            }
            let len = (((data[0] & 0x0F) as usize) << 8) | (data[1] as usize);
            let mut out = Vec::with_capacity(len);
            // First frame data starts at byte 2; on classic CAN up to 6 bytes in FF
            let ff_data_end = data.len().min(8);
            out.extend_from_slice(&data[2..ff_data_end]);

            let mut i = 8;
            while out.len() < len && i < data.len() {
                let cf_pci = data[i] >> 4;
                if cf_pci != 2 {
                    return Err(Error::Isotp(format!(
                        "expected consecutive frame at offset {i}, got PCI type {cf_pci}"
                    )));
                }
                let chunk_end = data.len().min(i + 8);
                out.extend_from_slice(&data[i + 1..chunk_end]);
                i += 8;
            }

            if out.len() < len {
                return Err(Error::Isotp(format!(
                    "incomplete multi-frame: got {} of {len} bytes",
                    out.len()
                )));
            }
            out.truncate(len);
            Ok(out)
        }
        2 => Err(Error::Isotp(
            "consecutive frame without first frame".into(),
        )),
        3 => Err(Error::Isotp("flow control frame not expected here".into())),
        _ => {
            // No PCI — assume ELM already delivered full UDS payload
            Ok(data.to_vec())
        }
    }
}

/// Build a single-frame ISO-TP message (for tests / raw CAN).
pub fn encode_single_frame(payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > 7 {
        return Err(Error::Isotp(
            "payload too long for single frame (max 7)".into(),
        ));
    }
    let mut frame = vec![payload.len() as u8];
    frame.extend_from_slice(payload);
    while frame.len() < 8 {
        frame.push(0x00);
    }
    Ok(frame)
}

/// Encode first + consecutive frames for a long payload (software stack / tests).
pub fn encode_multi_frame(payload: &[u8]) -> Result<Vec<Vec<u8>>> {
    if payload.len() <= 7 {
        return Ok(vec![encode_single_frame(payload)?]);
    }
    if payload.len() > 4095 {
        return Err(Error::Isotp("payload exceeds ISO-TP max length".into()));
    }

    let mut frames = Vec::new();
    let len = payload.len();
    let mut first = vec![0x10 | ((len >> 8) as u8), (len & 0xFF) as u8];
    first.extend_from_slice(&payload[..6.min(len)]);
    while first.len() < 8 {
        first.push(0);
    }
    frames.push(first);

    let mut offset = 6;
    let mut seq = 1u8;
    while offset < len {
        let mut cf = vec![0x20 | (seq & 0x0F)];
        let end = (offset + 7).min(len);
        cf.extend_from_slice(&payload[offset..end]);
        while cf.len() < 8 {
            cf.push(0);
        }
        frames.push(cf);
        offset = end;
        seq = (seq + 1) & 0x0F;
    }
    Ok(frames)
}

/// Concatenate multi-frame encode and decode round-trip helper for tests.
pub fn assemble_frames(frames: &[Vec<u8>]) -> Result<Vec<u8>> {
    if frames.is_empty() {
        return Err(Error::Isotp("no frames".into()));
    }
    let mut flat = Vec::new();
    for f in frames {
        flat.extend_from_slice(f);
    }
    decode_isotp_frames(&flat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    #[test]
    fn single_frame_roundtrip() {
        let payload = vec![0x22, 0xF1, 0x90];
        let frame = encode_single_frame(&payload).unwrap();
        assert_eq!(frame[0], 0x03);
        let decoded = decode_isotp_frames(&frame[..4]).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn multi_frame_roundtrip() {
        let payload: Vec<u8> = (0..20).collect();
        let frames = encode_multi_frame(&payload).unwrap();
        assert!(frames.len() >= 2);
        let decoded = assemble_frames(&frames).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn elm_style_uds_without_pci() {
        let data = vec![0x62, 0xF1, 0x90, 0x41, 0x42];
        let out = reassemble_from_elm(&data).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn request_via_mock_transport() {
        let mut t = MockTransport::new();
        t.expect("22F190", "62F190313233");
        let resp = request(&mut t, &[0x22, 0xF1, 0x90]).unwrap();
        assert_eq!(resp[0], 0x62);
    }
}
