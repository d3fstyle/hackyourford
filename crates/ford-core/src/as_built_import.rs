//! Ford dealer / PTS As-Built XML (`AS_BUILT_DATA`) parser and merger.
//!
//! Labels like `726-01-01` map to CAN addr 0x726, block 01 → DID 0xDE00.

use crate::error::{Error, Result};
use crate::profile::{DidDef, ModuleDef, VehicleProfile};
use crate::transport::CanBus;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Parsed As-Built XML data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedAsBuilt {
    pub vin: Option<String>,
    /// CAN addresses discovered via NODEID elements.
    pub node_ids: HashSet<u16>,
    /// 1-based block numbers per CAN address.
    pub blocks_by_addr: HashMap<u16, HashSet<u8>>,
}

impl ParsedAsBuilt {
    pub fn module_count(&self) -> usize {
        let mut addrs = self.node_ids.clone();
        addrs.extend(self.blocks_by_addr.keys().copied());
        addrs.len()
    }

    pub fn as_built_did_count(&self) -> usize {
        self.blocks_by_addr.values().map(|s| s.len()).sum()
    }
}

/// Well-known Ford 11-bit diagnostic request IDs for Focus / Tourneo era.
#[derive(Debug, Clone)]
pub struct KnownModule {
    pub id: &'static str,
    pub name: &'static str,
    pub bus: CanBus,
}

fn known_modules() -> HashMap<u16, KnownModule> {
    let mut map = HashMap::new();
    map.insert(0x7E0, KnownModule { id: "PCM", name: "Powertrain Control Module", bus: CanBus::Hs });
    map.insert(0x760, KnownModule { id: "ABS", name: "Anti-lock Braking System", bus: CanBus::Hs });
    map.insert(0x726, KnownModule { id: "BCM", name: "Body Control Module", bus: CanBus::Ms });
    map.insert(0x720, KnownModule { id: "IPC", name: "Instrument Panel Cluster", bus: CanBus::Ms });
    map.insert(0x730, KnownModule { id: "EPS", name: "Electronic Power Steering", bus: CanBus::Ms });
    map.insert(0x737, KnownModule { id: "RCM", name: "Restraints Control Module", bus: CanBus::Ms });
    map.insert(0x736, KnownModule { id: "PAM", name: "Parking Aid Module", bus: CanBus::Ms });
    map.insert(0x727, KnownModule { id: "ACM", name: "Audio Control Module", bus: CanBus::Ms });
    map.insert(0x7D0, KnownModule { id: "APIM", name: "SYNC (APIM)", bus: CanBus::Ms });
    map.insert(0x7E5, KnownModule { id: "AHCM", name: "Auxiliary Heater Control Module", bus: CanBus::Ms });
    map.insert(0x733, KnownModule { id: "SODL", name: "Side Obstacle Detection Left", bus: CanBus::Ms });
    map.insert(0x734, KnownModule { id: "SODR", name: "Side Obstacle Detection Right", bus: CanBus::Ms });
    map.insert(0x740, KnownModule { id: "DDM", name: "Driver Door Module", bus: CanBus::Ms });
    map.insert(0x741, KnownModule { id: "PDM", name: "Passenger Door Module", bus: CanBus::Ms });
    map.insert(0x7A7, KnownModule { id: "GWM", name: "Gateway Module", bus: CanBus::Ms });
    map.insert(0x793, KnownModule { id: "HVAC", name: "Climate Control Module", bus: CanBus::Ms });
    map.insert(0x797, KnownModule { id: "PSCM", name: "Parking Steer Assist", bus: CanBus::Ms });
    map
}

/// Parse Ford dealer AS_BUILT_DATA XML.
pub fn parse(xml: &str) -> Result<ParsedAsBuilt> {
    if !xml.contains("AS_BUILT_DATA") && !xml.contains("as_built_data") {
        return Err(Error::Profile("not a Ford AS_BUILT_DATA file".into()));
    }

    let vin_re = Regex::new(r"<VIN>([^<]+)</VIN>").unwrap();
    let node_id_re = Regex::new(r"<NODEID>([0-9A-Fa-f]{3})").unwrap();
    let bce_label_re = Regex::new(r#"LABEL\s*=\s*"([0-9A-Fa-f]{3})-(\d{2})-\d{2}""#).unwrap();
    let pcm_label_re = Regex::new(r#"LABEL\s*=\s*"PCM\s+(\d+)""#).unwrap();

    let vin = vin_re
        .captures(xml)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty());

    let node_ids: HashSet<u16> = node_id_re
        .captures_iter(xml)
        .filter_map(|c| u16::from_str_radix(c.get(1)?.as_str(), 16).ok())
        .collect();

    let mut blocks: HashMap<u16, HashSet<u8>> = HashMap::new();

    for cap in bce_label_re.captures_iter(xml) {
        if let (Some(addr_str), Some(block_str)) = (cap.get(1), cap.get(2)) {
            if let (Ok(addr), Ok(block)) = (
                u16::from_str_radix(addr_str.as_str(), 16),
                block_str.as_str().parse::<u8>(),
            ) {
                if (1..=255).contains(&block) {
                    blocks.entry(addr).or_default().insert(block);
                }
            }
        }
    }

    for cap in pcm_label_re.captures_iter(xml) {
        if let Some(block_str) = cap.get(1) {
            if let Ok(block) = block_str.as_str().parse::<u8>() {
                if (1..=255).contains(&block) {
                    blocks.entry(0x7E0).or_default().insert(block);
                }
            }
        }
    }

    if node_ids.is_empty() && blocks.is_empty() {
        return Err(Error::Profile(
            "no NODEID or as-built blocks found in .ab file".into(),
        ));
    }

    Ok(ParsedAsBuilt {
        vin,
        node_ids,
        blocks_by_addr: blocks,
    })
}

/// Merge As-Built data into a vehicle profile.
pub fn apply_as_built(base: &VehicleProfile, parsed: &ParsedAsBuilt) -> VehicleProfile {
    let known = known_modules();
    let mut by_tx: HashMap<u16, ModuleDef> = base
        .modules
        .iter()
        .map(|m| (m.tx_id, m.clone()))
        .collect();
    let mut order: Vec<u16> = base.modules.iter().map(|m| m.tx_id).collect();

    let mut all_addrs: Vec<u16> = parsed
        .node_ids
        .iter()
        .chain(parsed.blocks_by_addr.keys())
        .copied()
        .collect();
    all_addrs.sort_unstable();

    for addr in all_addrs {
        let blocks = parsed.blocks_by_addr.get(&addr).cloned().unwrap_or_default();
        let mut block_vec: Vec<u8> = blocks.into_iter().collect();
        block_vec.sort_unstable();

        let new_ab_dids: Vec<DidDef> = block_vec
            .into_iter()
            .map(|block| {
                let offset = block - 1;
                DidDef {
                    did: 0xDE00 + offset as u16,
                    label: format!("As-Built DE{:02X}", offset),
                    as_built: true,
                }
            })
            .collect();

        if let Some(existing) = by_tx.get(&addr) {
            let non_ab: Vec<DidDef> = existing
                .dids
                .iter()
                .filter(|d| !d.as_built)
                .cloned()
                .collect();
            let mut ab_by_did: HashMap<u16, DidDef> = existing
                .dids
                .iter()
                .filter(|d| d.as_built)
                .chain(new_ab_dids.iter())
                .map(|d| (d.did, d.clone()))
                .collect();
            let mut ab_dids: Vec<DidDef> = ab_by_did.drain().map(|(_, d)| d).collect();
            ab_dids.sort_by_key(|d| d.did);

            let mut updated = existing.clone();
            updated.dids = non_ab;
            updated.dids.extend(ab_dids);
            by_tx.insert(addr, updated);
        } else {
            let meta = known.get(&addr);
            let identity = vec![DidDef {
                did: 0xF188,
                label: "Software number".to_string(),
                as_built: false,
            }];

            let module = ModuleDef {
                id: meta.map(|m| m.id.to_string()).unwrap_or_else(|| format!("0x{:03X}", addr)),
                name: meta
                    .map(|m| m.name.to_string())
                    .unwrap_or_else(|| format!("Module 0x{:03X}", addr)),
                tx_id: addr,
                rx_id: None,
                bus: meta.map(|m| m.bus).unwrap_or(CanBus::Ms),
                security_level: None,
                security_algo: None,
                dids: identity.into_iter().chain(new_ab_dids).collect(),
                params: vec![],
            };
            by_tx.insert(addr, module);
            order.push(addr);
        }
    }

    // Preserve original order + append new modules sorted
    let mut modules: Vec<ModuleDef> = order
        .iter()
        .filter_map(|addr| by_tx.remove(addr))
        .collect();
    
    // Add remaining modules sorted by address
    let mut remaining: Vec<_> = by_tx.into_iter().collect();
    remaining.sort_by_key(|(addr, _)| *addr);
    modules.extend(remaining.into_iter().map(|(_, m)| m));

    let notes = merge_vin_note(base.notes.as_deref(), parsed.vin.as_deref());

    VehicleProfile {
        id: base.id.clone(),
        name: base.name.clone(),
        years: base.years.clone(),
        notes,
        modules,
    }
}

/// Generate a summary string for parsed As-Built data.
pub fn summary(parsed: &ParsedAsBuilt) -> String {
    let vin = parsed.vin.as_deref().unwrap_or("—");
    format!(
        "VIN {} · {} modules · {} as-built DIDs",
        vin,
        parsed.module_count(),
        parsed.as_built_did_count()
    )
}

fn merge_vin_note(existing: Option<&str>, vin: Option<&str>) -> Option<String> {
    match (existing, vin) {
        (None, None) => None,
        (Some(e), None) => Some(e.to_string()),
        (None, Some(v)) if !v.is_empty() => Some(format!("VIN {}", v)),
        (Some(e), Some(v)) if !v.is_empty() => {
            if e.contains(v) {
                Some(e.to_string())
            } else {
                Some(format!("{} · VIN {}", e, v))
            }
        }
        _ => existing.map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_XML: &str = include_str!("../../../test_data/sample_as_built.ab");

    #[test]
    fn parse_extracts_vin_nodes_and_blocks() {
        let parsed = parse(SAMPLE_XML).unwrap();
        assert_eq!(parsed.vin.as_deref(), Some("WF05XXGCC5HU30679"));
        assert!(parsed.node_ids.contains(&0x7E0));
        assert!(parsed.node_ids.contains(&0x726));
        assert!(parsed.node_ids.contains(&0x760));

        // PCM 1..3 → blocks on 7E0
        assert_eq!(
            parsed.blocks_by_addr.get(&0x7E0),
            Some(&[1, 2, 3].iter().copied().collect())
        );
        assert_eq!(
            parsed.blocks_by_addr.get(&0x726),
            Some(&[1, 2].iter().copied().collect())
        );
        assert_eq!(
            parsed.blocks_by_addr.get(&0x7D0),
            Some(&[1, 2].iter().copied().collect())
        );
        assert_eq!(
            parsed.blocks_by_addr.get(&0x720),
            Some(&[1].iter().copied().collect())
        );
    }

    #[test]
    fn apply_merges_as_built_dids_into_seed_profile() {
        let seed = VehicleProfile {
            id: "focus_test".to_string(),
            name: "Focus Test".to_string(),
            years: "2017".to_string(),
            notes: Some("seed".to_string()),
            modules: vec![ModuleDef {
                id: "BCM".to_string(),
                name: "Body Control Module".to_string(),
                tx_id: 0x726,
                rx_id: None,
                bus: CanBus::Ms,
                security_level: None,
                security_algo: None,
                dids: vec![
                    DidDef {
                        did: 0xF188,
                        label: "Software number".to_string(),
                        as_built: false,
                    },
                    DidDef {
                        did: 0xDE00,
                        label: "As-Built DE00".to_string(),
                        as_built: true,
                    },
                ],
                params: vec![],
            }],
        };

        let parsed = parse(SAMPLE_XML).unwrap();
        let updated = apply_as_built(&seed, &parsed);

        assert!(updated.notes.as_ref().unwrap().contains("WF05XXGCC5HU30679"));

        let bcm = updated.modules.iter().find(|m| m.id == "BCM").unwrap();
        assert!(bcm.dids.iter().any(|d| d.did == 0xF188 && !d.as_built));
        assert!(bcm.dids.iter().any(|d| d.did == 0xDE00 && d.as_built));
        assert!(bcm.dids.iter().any(|d| d.did == 0xDE01 && d.as_built));

        let pcm = updated.modules.iter().find(|m| m.tx_id == 0x7E0);
        assert!(pcm.is_some());
        let pcm = pcm.unwrap();
        assert_eq!(pcm.id, "PCM");
        assert_eq!(pcm.bus, CanBus::Hs);
        let pcm_ab_dids: HashSet<u16> = pcm
            .dids
            .iter()
            .filter(|d| d.as_built)
            .map(|d| d.did)
            .collect();
        assert_eq!(pcm_ab_dids, [0xDE00, 0xDE01, 0xDE02].iter().copied().collect());

        let abs = updated.modules.iter().find(|m| m.tx_id == 0x760);
        assert!(abs.is_some());
        assert_eq!(abs.unwrap().id, "ABS");
    }

    #[test]
    fn block_maps_to_de_did() {
        // block 1 → DE00, block 18 → DE11
        let did = 0xDE00 + (18 - 1);
        assert_eq!(0xDE11, did);
    }

    #[test]
    fn summary_renders_correctly() {
        let parsed = parse(SAMPLE_XML).unwrap();
        let s = summary(&parsed);
        assert!(s.contains("WF05XXGCC5HU30679"));
        assert!(s.contains("modules"));
        assert!(s.contains("as-built DIDs"));
    }
}
