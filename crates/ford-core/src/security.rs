//! Pluggable UDS SecurityAccess (0x27) seed → key algorithms.

use crate::error::{Error, Result};

/// Computes a SecurityAccess key from an ECU seed.
pub trait SecurityKeyAlgo: Send + Sync {
    /// Human-readable id for logs / errors.
    fn id(&self) -> &'static str;

    /// Derive the key bytes to send with odd+1 security level.
    fn compute_key(&self, seed: &[u8], level: u8) -> Result<Vec<u8>>;
}

/// No algorithm — used when the module does not require SecurityAccess.
/// Calling [`compute_key`] returns an error so unlock is not silently faked.
pub struct NoOpSecurity;

impl SecurityKeyAlgo for NoOpSecurity {
    fn id(&self) -> &'static str {
        "noop"
    }

    fn compute_key(&self, _seed: &[u8], _level: u8) -> Result<Vec<u8>> {
        Err(Error::Uds(
            "security access required but no key algorithm configured (noop)".into(),
        ))
    }
}

/// Experimental demo / lab algorithm: each key byte = seed XOR 0xFF.
/// Matches the focus demo fixture. Not a real Ford production algo.
pub struct ExperimentalXorSecurity;

impl SecurityKeyAlgo for ExperimentalXorSecurity {
    fn id(&self) -> &'static str {
        "experimental_xor_ff"
    }

    fn compute_key(&self, seed: &[u8], _level: u8) -> Result<Vec<u8>> {
        if seed.is_empty() {
            return Err(Error::Uds("empty security seed".into()));
        }
        Ok(seed.iter().map(|b| b ^ 0xFF).collect())
    }
}

/// Resolve algo by id (profile / API). Unknown ids fall back to noop with a clear error on use.
pub fn algo_by_id(id: &str) -> Box<dyn SecurityKeyAlgo> {
    match id {
        "experimental_xor_ff" | "demo_xor" => Box::new(ExperimentalXorSecurity),
        _ => Box::new(NoOpSecurity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_ff_key() {
        let algo = ExperimentalXorSecurity;
        let key = algo.compute_key(&[0x12, 0x34], 1).unwrap();
        assert_eq!(key, vec![0xED, 0xCB]);
    }
}
