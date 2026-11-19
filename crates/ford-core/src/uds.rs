//! Unified Diagnostic Services (ISO 14229) client over ISO-TP / ELM.

use crate::error::{nrc_description, Error, Result};
use crate::isotp;
use crate::transport::Transport;
use serde::{Deserialize, Serialize};

pub const SID_DIAGNOSTIC_SESSION_CONTROL: u8 = 0x10;
pub const SID_CLEAR_DTC: u8 = 0x14;
pub const SID_READ_DTC: u8 = 0x19;
pub const SID_READ_DATA_BY_ID: u8 = 0x22;
pub const SID_SECURITY_ACCESS: u8 = 0x27;
pub const SID_WRITE_DATA_BY_ID: u8 = 0x2E;
pub const SID_TESTER_PRESENT: u8 = 0x3E;

pub const SESSION_DEFAULT: u8 = 0x01;
pub const SESSION_EXTENDED: u8 = 0x03;

/// NRC: securityAccessDenied — write/programming needs SecurityAccess first.
pub const NRC_SECURITY_ACCESS_DENIED: u8 = 0x33;

/// A diagnostic trouble code with decoded status bits (ISO 14229).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dtc {
    pub code: String,
    pub status: u8,
    pub raw: u32,
    pub test_failed: bool,
    pub pending: bool,
    pub confirmed: bool,
}

impl Dtc {
    pub fn from_bytes(a: u8, b: u8, c: u8, status: u8) -> Self {
        let raw = u32::from_be_bytes([0, a, b, c]);
        Self {
            code: format_dtc_iso(a, b, c),
            status,
            raw,
            test_failed: status & 0x01 != 0,
            pending: status & 0x04 != 0,
            confirmed: status & 0x08 != 0,
        }
    }
}

/// Result of reading a data identifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidValue {
    pub did: u16,
    pub data: Vec<u8>,
    pub hex: String,
    pub ascii: Option<String>,
}

pub struct UdsClient<'a> {
    transport: &'a mut dyn Transport,
}

impl<'a> UdsClient<'a> {
    pub fn new(transport: &'a mut dyn Transport) -> Self {
        Self { transport }
    }

    pub fn raw_request(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut response = isotp::request(self.transport, payload)?;
        if response.len() >= 3 && response[0] == 0x7F && response[2] == 0x78 {
            response = isotp::request(self.transport, payload)?;
        }
        check_negative(&response, payload.first().copied().unwrap_or(0))?;
        Ok(response)
    }

    pub fn diagnostic_session_control(&mut self, session: u8) -> Result<()> {
        let resp = self.raw_request(&[SID_DIAGNOSTIC_SESSION_CONTROL, session])?;
        if resp.first() != Some(&(SID_DIAGNOSTIC_SESSION_CONTROL + 0x40)) {
            return Err(Error::Uds(format!("unexpected session response: {resp:02X?}")));
        }
        Ok(())
    }

    pub fn tester_present(&mut self) -> Result<()> {
        let resp = self.raw_request(&[SID_TESTER_PRESENT, 0x00])?;
        if resp.first() != Some(&(SID_TESTER_PRESENT + 0x40)) {
            return Err(Error::Uds(format!("unexpected tester present: {resp:02X?}")));
        }
        Ok(())
    }

    pub fn read_data_by_identifier(&mut self, did: u16) -> Result<DidValue> {
        let payload = [SID_READ_DATA_BY_ID, (did >> 8) as u8, (did & 0xFF) as u8];
        let resp = self.raw_request(&payload)?;
        if resp.len() < 3 || resp[0] != SID_READ_DATA_BY_ID + 0x40 {
            return Err(Error::Uds(format!("bad ReadDataById response: {resp:02X?}")));
        }
        let got_did = u16::from_be_bytes([resp[1], resp[2]]);
        if got_did != did {
            return Err(Error::Uds(format!(
                "DID mismatch: expected {did:04X}, got {got_did:04X}"
            )));
        }
        let data = resp[3..].to_vec();
        Ok(DidValue {
            did,
            hex: hex::encode_upper(&data),
            ascii: bytes_to_ascii(&data),
            data,
        })
    }

    /// Read DTCs (reportType 0x02 = reportDTCByStatusMask, mask 0xFF).
    pub fn read_dtcs(&mut self) -> Result<Vec<Dtc>> {
        let resp = self.raw_request(&[SID_READ_DTC, 0x02, 0xFF])?;
        if resp.first() != Some(&(SID_READ_DTC + 0x40)) {
            return Err(Error::Uds(format!("bad ReadDTC response: {resp:02X?}")));
        }
        if resp.len() < 3 {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let mut i = 3;
        while i + 3 < resp.len() {
            let status = resp[i + 3];
            out.push(Dtc::from_bytes(resp[i], resp[i + 1], resp[i + 2], status));
            i += 4;
        }
        Ok(out)
    }

    /// Clear diagnostic information (group = all).
    pub fn clear_dtcs(&mut self) -> Result<()> {
        let resp = self.raw_request(&[SID_CLEAR_DTC, 0xFF, 0xFF, 0xFF])?;
        if resp.first() != Some(&(SID_CLEAR_DTC + 0x40)) {
            return Err(Error::Uds(format!("bad ClearDTC response: {resp:02X?}")));
        }
        Ok(())
    }

    /// WriteDataByIdentifier (0x2E).
    pub fn write_data_by_identifier(&mut self, did: u16, data: &[u8]) -> Result<()> {
        let mut payload = Vec::with_capacity(3 + data.len());
        payload.push(SID_WRITE_DATA_BY_ID);
        payload.push((did >> 8) as u8);
        payload.push((did & 0xFF) as u8);
        payload.extend_from_slice(data);
        let resp = self.raw_request(&payload)?;
        if resp.len() < 3 || resp[0] != SID_WRITE_DATA_BY_ID + 0x40 {
            return Err(Error::Uds(format!("bad WriteDataById response: {resp:02X?}")));
        }
        let got_did = u16::from_be_bytes([resp[1], resp[2]]);
        if got_did != did {
            return Err(Error::Uds(format!(
                "DID mismatch on write: expected {did:04X}, got {got_did:04X}"
            )));
        }
        Ok(())
    }

    /// SecurityAccess requestSeed (odd level, e.g. 0x01).
    pub fn security_access_request_seed(&mut self, level: u8) -> Result<Vec<u8>> {
        if level % 2 == 0 {
            return Err(Error::Uds(format!(
                "requestSeed level must be odd, got 0x{level:02X}"
            )));
        }
        let resp = self.raw_request(&[SID_SECURITY_ACCESS, level])?;
        if resp.len() < 2 || resp[0] != SID_SECURITY_ACCESS + 0x40 || resp[1] != level {
            return Err(Error::Uds(format!(
                "bad security requestSeed response: {resp:02X?}"
            )));
        }
        Ok(resp[2..].to_vec())
    }

    /// SecurityAccess sendKey (even level, typically requestSeed level + 1).
    pub fn security_access_send_key(&mut self, send_level: u8, key: &[u8]) -> Result<()> {
        if send_level % 2 != 0 {
            return Err(Error::Uds(format!(
                "sendKey level must be even, got 0x{send_level:02X}"
            )));
        }
        let mut payload = Vec::with_capacity(2 + key.len());
        payload.push(SID_SECURITY_ACCESS);
        payload.push(send_level);
        payload.extend_from_slice(key);
        let resp = self.raw_request(&payload)?;
        if resp.len() < 2 || resp[0] != SID_SECURITY_ACCESS + 0x40 || resp[1] != send_level {
            return Err(Error::Uds(format!(
                "bad security sendKey response: {resp:02X?}"
            )));
        }
        Ok(())
    }

    /// Request seed at odd `level`, compute key with `algo`, send key at level+1.
    pub fn unlock_security(
        &mut self,
        level: u8,
        algo: &dyn crate::security::SecurityKeyAlgo,
    ) -> Result<()> {
        let seed = self.security_access_request_seed(level)?;
        // Already unlocked (seed all zeros is a common ECU convention).
        if seed.iter().all(|&b| b == 0) {
            return Ok(());
        }
        let key = algo.compute_key(&seed, level)?;
        self.security_access_send_key(level.wrapping_add(1), &key)
    }
}

fn check_negative(response: &[u8], requested_sid: u8) -> Result<()> {
    if response.len() >= 3 && response[0] == 0x7F {
        let nrc = response[2];
        if nrc == 0x78 {
            return Ok(());
        }
        return Err(Error::UdsNegative {
            nrc,
            description: nrc_description(nrc).to_string(),
        });
    }
    if response.is_empty() {
        return Err(Error::Uds(format!(
            "empty response for SID 0x{requested_sid:02X}"
        )));
    }
    Ok(())
}

/// Format ISO 15031-6 style DTC from three bytes.
pub fn format_dtc_iso(a: u8, b: u8, c: u8) -> String {
    let prefix = match (a & 0xC0) >> 6 {
        0x0 => "P",
        0x1 => "C",
        0x2 => "B",
        0x3 => "U",
        _ => "P",
    };
    let d1 = (a & 0x30) >> 4;
    let d2 = a & 0x0F;
    format!("{prefix}{d1}{d2:X}{b:02X}{c:02X}")
}

fn bytes_to_ascii(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    if data
        .iter()
        .all(|b| b.is_ascii_graphic() || *b == b' ' || *b == 0)
    {
        let s: String = data
            .iter()
            .map(|&b| if b == 0 { ' ' } else { b as char })
            .collect::<String>()
            .trim()
            .to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    #[test]
    fn format_p0301() {
        assert_eq!(format_dtc_iso(0x03, 0x01, 0x00), "P030100");
    }

    #[test]
    fn read_did_via_mock() {
        let mut t = MockTransport::new();
        t.expect("22F190", "62F19057464C414241");
        let mut client = UdsClient::new(&mut t);
        let v = client.read_data_by_identifier(0xF190).unwrap();
        assert_eq!(v.did, 0xF190);
        assert_eq!(v.ascii.as_deref(), Some("WFLABA"));
    }

    #[test]
    fn negative_response() {
        let mut t = MockTransport::new();
        t.expect("22F190", "7F2231");
        let mut client = UdsClient::new(&mut t);
        let err = client.read_data_by_identifier(0xF190).unwrap_err();
        match err {
            Error::UdsNegative { nrc, .. } => assert_eq!(nrc, 0x31),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn session_control() {
        let mut t = MockTransport::new();
        t.expect("1003", "5003003201F4");
        let mut client = UdsClient::new(&mut t);
        client.diagnostic_session_control(SESSION_EXTENDED).unwrap();
    }

    #[test]
    fn read_dtcs_empty_mask_only() {
        let mut t = MockTransport::new();
        t.expect("1902FF", "5902FF");
        let mut client = UdsClient::new(&mut t);
        let dtcs = client.read_dtcs().unwrap();
        assert!(dtcs.is_empty());
    }

    #[test]
    fn write_did_via_mock() {
        let mut t = MockTransport::new();
        t.expect("2EDE00", "6EDE00");
        let mut client = UdsClient::new(&mut t);
        client
            .write_data_by_identifier(0xDE00, &[0x01, 0x02, 0x03, 0x04])
            .unwrap();
    }

    #[test]
    fn unlock_security_xor() {
        use crate::security::ExperimentalXorSecurity;
        let mut t = MockTransport::new();
        t.expect("2701", "67011234");
        t.expect("2702", "6702");
        let mut client = UdsClient::new(&mut t);
        client
            .unlock_security(0x01, &ExperimentalXorSecurity)
            .unwrap();
    }
}
