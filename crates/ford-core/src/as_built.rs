//! As-Built helpers: Ford checksum groups + bit-field param decode/encode.

use crate::error::{Error, Result};
use crate::profile::{ParamDef, ParamKind};
use crate::uds::DidValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Decoded value for one profile parameter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamValue {
    pub id: String,
    pub label: String,
    pub did: u16,
    pub kind: ParamKind,
    pub value: u64,
    pub verified: bool,
    pub notes: Option<String>,
    pub options: Vec<crate::profile::ParamOption>,
}

/// User edit: set parameter `id` to `value`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamEdit {
    pub id: String,
    pub value: u64,
}

/// If `data` length is a multiple of 5, treat each group as 4 payload bytes + 1 checksum.
/// Checksum convention used here: `0xFF - (sum(bytes[0..4]) & 0xFF)` (common Ford AB style).
pub fn validate_ford_as_built_groups(data: &[u8]) -> Option<bool> {
    if data.is_empty() || data.len() % 5 != 0 {
        return None;
    }
    Some(data.chunks_exact(5).all(|g| g[4] == ford_group_checksum(&g[..4])))
}

/// Rewrite checksum bytes in-place for each 5-byte group. No-op if length is not a multiple of 5.
pub fn fix_ford_as_built_groups(data: &mut [u8]) -> bool {
    if data.is_empty() || data.len() % 5 != 0 {
        return false;
    }
    for g in data.chunks_exact_mut(5) {
        g[4] = ford_group_checksum(&g[..4]);
    }
    true
}

fn ford_group_checksum(payload4: &[u8]) -> u8 {
    let sum: u32 = payload4.iter().map(|&b| u32::from(b)).sum();
    (0xFFu32.wrapping_sub(sum & 0xFF)) as u8
}

/// Read `width` bits (LSB-first within the starting byte, then following bytes) as little-endian.
pub fn get_bits(data: &[u8], byte: u16, bit: u8, width: u8) -> Result<u64> {
    if width == 0 || width > 64 {
        return Err(Error::Profile(format!("invalid bit width {width}")));
    }
    if bit > 7 {
        return Err(Error::Profile(format!("invalid bit offset {bit}")));
    }
    let start_bit = u32::from(byte) * 8 + u32::from(bit);
    let end_bit = start_bit + u32::from(width);
    let need_bytes = ((end_bit + 7) / 8) as usize;
    if data.len() < need_bytes {
        return Err(Error::Profile(format!(
            "DID payload too short: need {need_bytes} bytes, have {}",
            data.len()
        )));
    }
    let mut value = 0u64;
    for i in 0..width {
        let abs = start_bit + u32::from(i);
        let b = data[(abs / 8) as usize];
        let bit_i = (abs % 8) as u8;
        if (b >> bit_i) & 1 != 0 {
            value |= 1u64 << i;
        }
    }
    Ok(value)
}

/// Write `width` bits into `data` (same layout as [`get_bits`]).
pub fn set_bits(data: &mut [u8], byte: u16, bit: u8, width: u8, value: u64) -> Result<()> {
    if width == 0 || width > 64 {
        return Err(Error::Profile(format!("invalid bit width {width}")));
    }
    if bit > 7 {
        return Err(Error::Profile(format!("invalid bit offset {bit}")));
    }
    let max_val = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    if value > max_val {
        return Err(Error::Profile(format!(
            "value {value} exceeds {width}-bit field (max {max_val})"
        )));
    }
    let start_bit = u32::from(byte) * 8 + u32::from(bit);
    let end_bit = start_bit + u32::from(width);
    let need_bytes = ((end_bit + 7) / 8) as usize;
    if data.len() < need_bytes {
        return Err(Error::Profile(format!(
            "DID payload too short: need {need_bytes} bytes, have {}",
            data.len()
        )));
    }
    for i in 0..width {
        let abs = start_bit + u32::from(i);
        let idx = (abs / 8) as usize;
        let bit_i = (abs % 8) as u8;
        let mask = 1u8 << bit_i;
        if (value >> i) & 1 != 0 {
            data[idx] |= mask;
        } else {
            data[idx] &= !mask;
        }
    }
    Ok(())
}

/// Decode all params that have a matching DID block present.
pub fn decode_params(blocks: &[DidValue], params: &[ParamDef]) -> Result<Vec<ParamValue>> {
    let by_did: HashMap<u16, &DidValue> = blocks.iter().map(|b| (b.did, b)).collect();
    let mut out = Vec::new();
    for p in params {
        let Some(block) = by_did.get(&p.did) else {
            continue;
        };
        let value = get_bits(&block.data, p.byte, p.bit, p.width)?;
        out.push(ParamValue {
            id: p.id.clone(),
            label: p.label.clone(),
            did: p.did,
            kind: p.kind,
            value,
            verified: p.verified,
            notes: p.notes.clone(),
            options: p.options.clone(),
        });
    }
    Ok(out)
}

/// Apply edits to DID payload map (did → bytes). Unknown param ids error.
pub fn apply_param_edits(
    payloads: &mut HashMap<u16, Vec<u8>>,
    edits: &[ParamEdit],
    params: &[ParamDef],
) -> Result<()> {
    let by_id: HashMap<&str, &ParamDef> = params.iter().map(|p| (p.id.as_str(), p)).collect();
    for edit in edits {
        let def = by_id.get(edit.id.as_str()).ok_or_else(|| {
            Error::Profile(format!("unknown parameter id: {}", edit.id))
        })?;
        let data = payloads.get_mut(&def.did).ok_or_else(|| {
            Error::Profile(format!(
                "no payload for DID {:04X} (param {})",
                def.did, def.id
            ))
        })?;
        match def.kind {
            ParamKind::Bool => {
                if edit.value > 1 {
                    return Err(Error::Profile(format!(
                        "param {}: bool value must be 0 or 1",
                        def.id
                    )));
                }
            }
            ParamKind::Enum => {
                if !def.options.is_empty() && !def.options.iter().any(|o| o.value == edit.value) {
                    return Err(Error::Profile(format!(
                        "param {}: value {} not in options",
                        def.id, edit.value
                    )));
                }
            }
            ParamKind::Uint => {}
        }
        set_bits(data, def.byte, def.bit, def.width, edit.value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{ParamDef, ParamKind, ParamOption};

    #[test]
    fn fix_and_validate() {
        let mut data = vec![0x01, 0x02, 0x03, 0x04, 0x00, 0x11, 0x22, 0x33, 0x44, 0x00];
        assert!(fix_ford_as_built_groups(&mut data));
        assert_eq!(validate_ford_as_built_groups(&data), Some(true));
        data[4] ^= 0x01;
        assert_eq!(validate_ford_as_built_groups(&data), Some(false));
    }

    #[test]
    fn non_grouped_skipped() {
        assert_eq!(validate_ford_as_built_groups(&[1, 2, 3]), None);
        let mut d = vec![1, 2, 3];
        assert!(!fix_ford_as_built_groups(&mut d));
    }

    #[test]
    fn get_set_bool_bit0() {
        let mut data = vec![0x00, 0x00];
        set_bits(&mut data, 0, 0, 1, 1).unwrap();
        assert_eq!(data[0], 0x01);
        assert_eq!(get_bits(&data, 0, 0, 1).unwrap(), 1);
        set_bits(&mut data, 0, 0, 1, 0).unwrap();
        assert_eq!(data[0], 0x00);
    }

    #[test]
    fn get_set_cross_byte() {
        // 3 bits starting at byte0 bit6 → spans into byte1
        let mut data = vec![0x00, 0x00];
        set_bits(&mut data, 0, 6, 3, 0b101).unwrap();
        assert_eq!(get_bits(&data, 0, 6, 3).unwrap(), 0b101);
    }

    #[test]
    fn decode_and_apply_demo_layout() {
        // Matches focus demo DE00 and YAML seed params
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let blocks = [DidValue {
            did: 0xDE00,
            data: data.clone(),
            hex: hex::encode_upper(&data),
            ascii: None,
        }];
        let params = [
            ParamDef {
                id: "drl_enable".into(),
                label: "DRL".into(),
                did: 0xDE00,
                byte: 0,
                bit: 0,
                width: 1,
                kind: ParamKind::Bool,
                options: vec![],
                verified: false,
                notes: None,
            },
            ParamDef {
                id: "approach_light_mode".into(),
                label: "Approach".into(),
                did: 0xDE00,
                byte: 2,
                bit: 0,
                width: 2,
                kind: ParamKind::Enum,
                options: vec![
                    ParamOption {
                        value: 0,
                        label: "Off".into(),
                    },
                    ParamOption {
                        value: 3,
                        label: "Long".into(),
                    },
                ],
                verified: false,
                notes: None,
            },
            ParamDef {
                id: "courtesy_delay".into(),
                label: "Delay".into(),
                did: 0xDE00,
                byte: 3,
                bit: 0,
                width: 4,
                kind: ParamKind::Uint,
                options: vec![],
                verified: false,
                notes: None,
            },
        ];
        let decoded = decode_params(&blocks, &params).unwrap();
        assert_eq!(decoded[0].value, 1); // bit0 of 0x01
        assert_eq!(decoded[1].value, 3); // low 2 bits of 0x03
        assert_eq!(decoded[2].value, 4); // low nibble of 0x04

        let mut payloads = HashMap::from([(0xDE00u16, data)]);
        apply_param_edits(
            &mut payloads,
            &[ParamEdit {
                id: "drl_enable".into(),
                value: 0,
            }],
            &params,
        )
        .unwrap();
        assert_eq!(payloads[&0xDE00][0] & 1, 0);
    }
}
