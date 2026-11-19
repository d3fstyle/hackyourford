//! CAN monitor frame capture and button-press pattern correlation.

use regex::Regex;
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

/// One timed stage of a guided press-capture sequence.
/// `expected_presses` is 0 for the idle baseline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureStage {
    pub expected_presses: u32,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// True when ELM/STN ended ATMA (buffer full or stopped).
/// Torque re-issues ATMA when it sees these; silence follows if we do not.
pub fn monitor_output_ended(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let upper = text.to_uppercase();
    upper.contains("BUFFER FULL")
        || upper.contains("STOPPED")
        || Regex::new(r"(?i)(^|[^A-Z])FULL([^A-Z]|$)")
            .map(|re| re.is_match(&upper))
            .unwrap_or(false)
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

const SEQUENCE_SCORE_THRESHOLD: f64 = 0.4;
const BASELINE_FRAME_NOISE_RATIO: f64 = 0.15;
const EXPECTED_PRESS_RANKS: [f64; 3] = [1.0, 2.0, 3.0];

/// Score IDs/bytes/bits whose change counts across guided stages rise with press counts.
/// Stage 0 (expected_presses == 0) is the idle baseline used to reject periodic noise.
/// Bits are scored independently so a quiet button bit survives inside a noisy byte.
pub fn correlate_press_sequence(
    frames: &[CanFrame],
    stages: &[CaptureStage],
) -> Vec<PatternCandidate> {
    if frames.is_empty() || stages.len() < 2 {
        return Vec::new();
    }

    let baseline = stages.iter().find(|s| s.expected_presses == 0);
    if baseline.is_none() {
        return Vec::new();
    }
    let baseline = baseline.unwrap();

    let mut press_stages: Vec<&CaptureStage> = stages
        .iter()
        .filter(|s| s.expected_presses > 0)
        .collect();
    press_stages.sort_by_key(|s| s.expected_presses);

    if press_stages.is_empty() {
        return Vec::new();
    }

    let baseline_idx = stages.iter().position(|s| s.expected_presses == 0).unwrap();
    let press_idx: Vec<usize> = press_stages
        .iter()
        .map(|s| stages.iter().position(|x| x.start_ms == s.start_ms).unwrap())
        .collect();

    let expected_ranks: Vec<f64> = if press_stages.is_empty() {
        EXPECTED_PRESS_RANKS.to_vec()
    } else {
        press_stages.iter().map(|s| s.expected_presses as f64).collect()
    };

    let mut by_id: HashMap<u16, Vec<&CanFrame>> = HashMap::new();
    for f in frames {
        by_id.entry(f.id).or_default().push(f);
    }

    let mut candidates = Vec::new();

    for (can_id, list) in by_id {
        if list.len() < 2 {
            continue;
        }
        let baseline_frames = list
            .iter()
            .filter(|f| f.ts_ms >= baseline.start_ms && f.ts_ms <= baseline.end_ms)
            .count();
        let max_len = list.iter().map(|f| f.data.len()).max().unwrap_or(0);

        for byte_i in 0..max_len {
            let mut byte_counts = vec![0u32; stages.len()];
            let mut bit_counts = vec![vec![0u32; stages.len()]; 8];
            let mut stage_samples = vec![Vec::new(); stages.len()];

            for w in list.windows(2) {
                let a = w[0].data.get(byte_i).copied();
                let b = w[1].data.get(byte_i).copied();
                if a.is_none() || b.is_none() || a == b {
                    continue;
                }
                let a = a.unwrap();
                let b = b.unwrap();
                let ts = w[1].ts_ms;
                let stage_idx = stages
                    .iter()
                    .position(|s| ts >= s.start_ms && ts <= s.end_ms);
                if stage_idx.is_none() {
                    continue;
                }
                let stage_idx = stage_idx.unwrap();
                byte_counts[stage_idx] += 1;
                if stage_samples[stage_idx].len() < 4 {
                    stage_samples[stage_idx].push(format!("s{stage_idx}:{a:02X}->{b:02X}"));
                }
                let flipped = a ^ b;
                for bit in 0..8 {
                    if (flipped >> bit) & 1 != 0 {
                        bit_counts[bit][stage_idx] += 1;
                    }
                }
            }

            let mut best_bit: Option<(u8, f64)> = None;
            for bit in 0..8u8 {
                let counts = &bit_counts[bit as usize];
                let c0 = counts[baseline_idx];
                let observed: Vec<f64> = press_idx.iter().map(|&i| counts[i] as f64).collect();
                if !passes_baseline(c0, &observed, baseline_frames) {
                    continue;
                }
                let score = score_sequence_changes(&observed, &expected_ranks, c0, baseline_frames);
                if score < SEQUENCE_SCORE_THRESHOLD {
                    continue;
                }
                if best_bit.map(|(_, s)| score > s).unwrap_or(true) {
                    best_bit = Some((bit, score));
                }
            }

            let byte_c0 = byte_counts[baseline_idx];
            let byte_obs: Vec<f64> = press_idx.iter().map(|&i| byte_counts[i] as f64).collect();
            let byte_score = if passes_baseline(byte_c0, &byte_obs, baseline_frames) {
                score_sequence_changes(&byte_obs, &expected_ranks, byte_c0, baseline_frames)
            } else {
                0.0
            };

            let bit_score = best_bit.map(|(_, s)| s).unwrap_or(0.0);
            let best_score = byte_score.max(bit_score);
            if best_score < SEQUENCE_SCORE_THRESHOLD {
                continue;
            }

            candidates.push(PatternCandidate {
                can_id,
                byte_index: byte_i,
                bit_index: best_bit
                    .filter(|(_, s)| *s >= SEQUENCE_SCORE_THRESHOLD)
                    .map(|(b, _)| b),
                change_score: best_score,
                sample_values: stage_samples.into_iter().flatten().take(8).collect(),
            });
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

/// Baseline is too noisy when idle changes rival press activity or exceed a share of
/// baseline frames (covers periodic counters without killing quiet button bits).
fn passes_baseline(baseline_changes: u32, press_observed: &[f64], baseline_frame_count: usize) -> bool {
    if press_observed.is_empty() || press_observed.iter().all(|&x| x == 0.0) {
        return false;
    }
    if press_observed[0] <= 0.0 {
        return false;
    }
    let noise_ceiling = 3.max((BASELINE_FRAME_NOISE_RATIO * baseline_frame_count as f64) as u32);
    if baseline_changes >= noise_ceiling {
        return false;
    }
    let min_press = press_observed.iter().copied().fold(f64::INFINITY, f64::min);
    if min_press > 0.0 && baseline_changes as f64 >= min_press {
        return false;
    }
    true
}

/// Primary score: Pearson correlation of observed press-stage counts vs expected ranks
/// (monotonicity). Exact-count similarity with k=1..6 is a small tie-break bonus.
fn score_sequence_changes(
    observed: &[f64],
    expected_ranks: &[f64],
    baseline_changes: u32,
    baseline_frame_count: usize,
) -> f64 {
    if observed.len() != expected_ranks.len() || observed.is_empty() {
        return 0.0;
    }
    if observed[0] <= 0.0 {
        return 0.0;
    }

    let mono = pearson_correlation(observed, expected_ranks).max(0.0).min(1.0);
    if mono < SEQUENCE_SCORE_THRESHOLD {
        return mono;
    }

    let mut exact_best: f64 = 0.0;
    for k in 1..=6 {
        let expected: Vec<f64> = expected_ranks.iter().map(|&r| r * k as f64).collect();
        exact_best = exact_best.max(sequence_similarity(observed, &expected));
    }

    let noise_ceiling = 3.max((BASELINE_FRAME_NOISE_RATIO * baseline_frame_count as f64) as u32);
    let penalty = (1.0 - baseline_changes as f64 / noise_ceiling as f64).max(0.0).min(1.0);

    // Weight monotonic gate heavily; exact match only breaks ties.
    (0.85 * mono + 0.15 * exact_best) * penalty
}

/// Pearson correlation clamped to [0, 1] (negative correlations → 0).
fn pearson_correlation(xs: &[f64], ys: &[f64]) -> f64 {
    if xs.len() != ys.len() || xs.len() < 2 {
        return 0.0;
    }
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den_x = 0.0;
    let mut den_y = 0.0;
    for i in 0..xs.len() {
        let dx = xs[i] - mean_x;
        let dy = ys[i] - mean_y;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }
    if den_x <= 0.0 || den_y <= 0.0 {
        // Constant series: treat perfect proportional match as 1 when all positive & ordered.
        return if xs == ys { 1.0 } else { 0.0 };
    }
    (num / (den_x * den_y).sqrt()).max(0.0).min(1.0)
}

/// Mean relative similarity of observed change counts to an expected sequence.
fn sequence_similarity(observed: &[f64], expected: &[f64]) -> f64 {
    if observed.len() != expected.len() || observed.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..observed.len() {
        let exp = expected[i];
        let obs = observed[i];
        if exp <= 0.0 {
            sum += if obs == 0.0 { 1.0 } else { 0.0 };
            continue;
        }
        let err = (obs - exp).abs() / exp;
        sum += (1.0 - err).max(0.0).min(1.0);
    }
    sum / observed.len() as f64
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
    fn monitor_output_ended_buffer_full() {
        assert!(monitor_output_ended("BUFFER FULL"));
        assert!(monitor_output_ended("buffer full"));
    }

    #[test]
    fn monitor_output_ended_stopped() {
        assert!(monitor_output_ended("STOPPED"));
    }

    #[test]
    fn monitor_output_ended_full() {
        assert!(monitor_output_ended("FULL"));
        assert!(!monitor_output_ended("FULLFRAME")); // Adjacent letters should not match
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

    #[test]
    fn correlate_press_sequence_finds_progressive() {
        // Simulate 3 stages: idle (0 presses), 1 press, 2 presses
        let stages = vec![
            CaptureStage {
                expected_presses: 0,
                start_ms: 0,
                end_ms: 100,
            },
            CaptureStage {
                expected_presses: 1,
                start_ms: 100,
                end_ms: 200,
            },
            CaptureStage {
                expected_presses: 2,
                start_ms: 200,
                end_ms: 300,
            },
        ];

        // Build frames where byte 1 changes once in stage 1, twice in stage 2, never in baseline
        let mut frames = Vec::new();
        // Baseline: no changes
        for i in 0..10 {
            frames.push(CanFrame {
                ts_ms: i * 10,
                id: 0x3B3,
                data: vec![0x00, 0x00],
                raw_line: String::new(),
            });
        }
        // Stage 1: 1 change
        frames.push(CanFrame {
            ts_ms: 110,
            id: 0x3B3,
            data: vec![0x00, 0x00],
            raw_line: String::new(),
        });
        frames.push(CanFrame {
            ts_ms: 120,
            id: 0x3B3,
            data: vec![0x00, 0x01],
            raw_line: String::new(),
        });
        // Stage 2: 2 changes
        frames.push(CanFrame {
            ts_ms: 210,
            id: 0x3B3,
            data: vec![0x00, 0x01],
            raw_line: String::new(),
        });
        frames.push(CanFrame {
            ts_ms: 220,
            id: 0x3B3,
            data: vec![0x00, 0x02],
            raw_line: String::new(),
        });
        frames.push(CanFrame {
            ts_ms: 230,
            id: 0x3B3,
            data: vec![0x00, 0x03],
            raw_line: String::new(),
        });

        let c = correlate_press_sequence(&frames, &stages);
        assert!(!c.is_empty());
        assert_eq!(c[0].can_id, 0x3B3);
        assert_eq!(c[0].byte_index, 1);
        assert!(c[0].change_score > SEQUENCE_SCORE_THRESHOLD);
    }

    #[test]
    fn pearson_correlation_perfect() {
        let xs = vec![1.0, 2.0, 3.0];
        let ys = vec![1.0, 2.0, 3.0];
        let r = pearson_correlation(&xs, &ys);
        assert!((r - 1.0).abs() < 0.01);
    }

    #[test]
    fn pearson_correlation_monotonic() {
        let xs = vec![1.0, 2.0, 3.0];
        let ys = vec![2.0, 4.0, 6.0];
        let r = pearson_correlation(&xs, &ys);
        assert!((r - 1.0).abs() < 0.01);
    }

    #[test]
    fn passes_baseline_rejects_noisy() {
        // Baseline has 10 changes, press has 5 changes → baseline too noisy
        let baseline_frames = 100;
        // noise_ceiling would be 15 (0.15 * 100)
        assert!(!passes_baseline(20, &[5.0], baseline_frames));
    }
}
