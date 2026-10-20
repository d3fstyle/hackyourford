//! CAN monitor frame capture and button-press pattern correlation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single captured CAN frame (11-bit ID).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanFrame {
    pub ts_ms: u64,
    pub id: u16,
    pub data: Vec<u8>,
    pub raw_line: String,
}

/// Candidate ID/byte that correlates with press windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternCandidate {
    pub can_id: u16,
    pub byte_index: usize,
    pub bit_index: Option<u8>,
    pub change_score: f64,
    pub sample_values: Vec<String>,
}

/// Parse one ELM monitor line into a frame (headers on).
/// Formats: `7E8 06 41 0C …` or `12345678 AABBCCDD…`
pub fn parse_monitor_line(line: &str, ts_ms: u64) -> Option<CanFrame> {
    let line = line.trim().trim_end_matches('>');
    if line.is_empty() {
        return None;
    }
    let upper = line.to_ascii_uppercase();
    if upper.contains("SEARCHING")
        || upper == "OK"
        || upper == "STOPPED"
        || upper.starts_with("AT")
        || upper.starts_with("ST")
        || upper.contains('?')
    {
        return None;
    }

    let cleaned: String = line.chars().filter(|c| c.is_ascii_hexdigit() || c.is_whitespace()).collect();
    let parts: Vec<&str> = cleaned.split_whitespace().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        // Continuous hex without spaces: first 3 digits = ID
        let hex_only: String = line.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if hex_only.len() < 5 {
            return None;
        }
        let id = u16::from_str_radix(&hex_only[..3], 16).ok()?;
        let data = hex::decode(&hex_only[3..]).ok()?;
        return Some(CanFrame {
            ts_ms,
            id,
            data,
            raw_line: line.to_string(),
        });
    }

    let id = u16::from_str_radix(parts[0], 16).ok()?;
    let mut data = Vec::new();
    for p in parts.iter().skip(1) {
        if p.len() == 2 {
            if let Ok(b) = u8::from_str_radix(p, 16) {
                data.push(b);
            }
        } else {
            // glued hex
            if let Ok(bytes) = hex::decode(p) {
                data.extend(bytes);
            }
        }
    }
    Some(CanFrame {
        ts_ms,
        id,
        data,
        raw_line: line.to_string(),
    })
}

/// Split a monitor buffer into frames.
pub fn parse_monitor_buffer(buf: &str, base_ts_ms: u64) -> Vec<CanFrame> {
    let normalized = buf.replace("\r\n", "\n").replace('\r', "\n");
    let mut frames = Vec::new();
    let mut offset = 0u64;
    for line in normalized.lines() {
        if let Some(f) = parse_monitor_line(line, base_ts_ms + offset) {
            frames.push(f);
            offset += 1;
        }
    }
    frames
}

/// Score IDs/bytes whose changes cluster inside press windows more than outside.
pub fn correlate_press_pattern(
    frames: &[CanFrame],
    press_windows: &[(u64, u64)],
) -> Vec<PatternCandidate> {
    if frames.is_empty() || press_windows.is_empty() {
        return Vec::new();
    }

    // Group consecutive frames by ID, track byte diffs over time
    let mut by_id: HashMap<u16, Vec<&CanFrame>> = HashMap::new();
    for f in frames {
        by_id.entry(f.id).or_default().push(f);
    }

    let mut candidates = Vec::new();
    for (can_id, list) in by_id {
        if list.len() < 2 {
            continue;
        }
        let max_len = list.iter().map(|f| f.data.len()).max().unwrap_or(0);
        for byte_i in 0..max_len {
            let mut changes_in = 0u32;
            let mut changes_out = 0u32;
            let mut samples = Vec::new();
            for w in list.windows(2) {
                let a = w[0].data.get(byte_i).copied().unwrap_or(0);
                let b = w[1].data.get(byte_i).copied().unwrap_or(0);
                if a == b {
                    continue;
                }
                let ts = w[1].ts_ms;
                let inside = press_windows.iter().any(|(s, e)| ts >= *s && ts <= *e);
                if inside {
                    changes_in += 1;
                    samples.push(format!("{a:02X}->{b:02X} @{ts}"));
                } else {
                    changes_out += 1;
                }
            }
            if changes_in == 0 {
                continue;
            }
            let score = f64::from(changes_in) / f64::from(changes_in + changes_out.max(1));
            if score >= 0.5 {
                // bit-level hints
                let mut best_bit: Option<(u8, f64)> = None;
                for bit in 0..8u8 {
                    let mut bin = 0u32;
                    let mut bout = 0u32;
                    for w in list.windows(2) {
                        let a = w[0].data.get(byte_i).copied().unwrap_or(0);
                        let b = w[1].data.get(byte_i).copied().unwrap_or(0);
                        let mask = 1u8 << bit;
                        if (a ^ b) & mask == 0 {
                            continue;
                        }
                        let ts = w[1].ts_ms;
                        let inside = press_windows.iter().any(|(s, e)| ts >= *s && ts <= *e);
                        if inside {
                            bin += 1;
                        } else {
                            bout += 1;
                        }
                    }
                    if bin == 0 {
                        continue;
                    }
                    let bscore = f64::from(bin) / f64::from(bin + bout.max(1));
                    if best_bit.map(|(_, s)| bscore > s).unwrap_or(true) {
                        best_bit = Some((bit, bscore));
                    }
                }
                candidates.push(PatternCandidate {
                    can_id,
                    byte_index: byte_i,
                    bit_index: best_bit.filter(|(_, s)| *s >= 0.5).map(|(b, _)| b),
                    change_score: score,
                    sample_values: samples.into_iter().take(8).collect(),
                });
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.change_score
            .partial_cmp(&a.change_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(40);
    candidates
}

/// Diff two DID snapshots (did -> hex).
pub fn diff_did_snapshots(
    before: &HashMap<u16, String>,
    after: &HashMap<u16, String>,
) -> Vec<(u16, String, String)> {
    let mut out = Vec::new();
    for (did, after_hex) in after {
        match before.get(did) {
            Some(before_hex) if before_hex != after_hex => {
                out.push((*did, before_hex.clone(), after_hex.clone()));
            }
            None => out.push((*did, String::new(), after_hex.clone())),
            _ => {}
        }
    }
    out.sort_by_key(|(d, _, _)| *d);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_spaced_frame() {
        let f = parse_monitor_line("7E8 06 41 0C 1A F8", 100).unwrap();
        assert_eq!(f.id, 0x7E8);
        assert_eq!(f.data[0], 0x06);
    }

    #[test]
    fn correlate_finds_changing_byte() {
        let frames = vec![
            CanFrame {
                ts_ms: 0,
                id: 0x3B3,
                data: vec![0x00, 0x00],
                raw_line: String::new(),
            },
            CanFrame {
                ts_ms: 10,
                id: 0x3B3,
                data: vec![0x00, 0x01],
                raw_line: String::new(),
            },
            CanFrame {
                ts_ms: 20,
                id: 0x3B3,
                data: vec![0x00, 0x00],
                raw_line: String::new(),
            },
            CanFrame {
                ts_ms: 50,
                id: 0x3B3,
                data: vec![0x00, 0x00],
                raw_line: String::new(),
            },
        ];
        let c = correlate_press_pattern(&frames, &[(5, 25)]);
        assert!(!c.is_empty());
        assert_eq!(c[0].can_id, 0x3B3);
        assert_eq!(c[0].byte_index, 1);
    }
}
