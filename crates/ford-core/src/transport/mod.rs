//! Serial transport for ELM327 / STN adapters (OBDLink MX+, clones).

mod mock;
mod stn;

pub use mock::MockTransport;
pub use stn::{list_serial_ports, AdapterInfo, Bus, CanBus, SerialPortInfo, StnTransport};

use crate::error::Result;
use std::time::Duration;

/// Low-level line-oriented adapter transport.
pub trait Transport: Send {
    /// Send a raw AT/ST/command line (without trailing CR) and return the response body
    /// (text before the `>` prompt), trimmed.
    fn command(&mut self, cmd: &str) -> Result<String>;

    /// Send a hex UDS/OBD payload (already formatted as hex digits, no spaces) and return
    /// raw response text from the adapter.
    fn send_hex(&mut self, hex_payload: &str) -> Result<String> {
        self.command(hex_payload)
    }

    fn set_timeout(&mut self, timeout: Duration);
}

/// Full diagnostic adapter: line transport plus bus/header setup used by [`crate::scanner::ScannerSession`].
pub trait DiagAdapter: Transport {
    /// Soft reset, identify adapter, basic init for CAN diagnostics.
    fn initialize(&mut self) -> Result<AdapterInfo>;

    /// Select HS-CAN or MS-CAN.
    fn select_bus(&mut self, bus: CanBus) -> Result<()>;

    /// Set 11-bit CAN request header (e.g. 0x7E0 for PCM).
    fn set_header(&mut self, header: u32) -> Result<()>;

    /// Adapter identity after a successful [`initialize`].
    fn adapter_info(&self) -> Option<&AdapterInfo>;

    /// Clear CAN receive address filter (accept all) for monitor mode.
    fn clear_receive_filter(&mut self) -> Result<()> {
        let _ = self.command("ATAR");
        Ok(())
    }

    /// Start continuous CAN monitor (`ATMA`). Does not wait for `>`.
    fn start_monitor(&mut self) -> Result<()>;

    /// Read a chunk of monitor output (no prompt required) for up to `max_wait`.
    fn read_monitor_chunk(&mut self, max_wait: Duration) -> Result<String>;

    /// Stop monitor (send CR) and wait for prompt.
    fn stop_monitor(&mut self) -> Result<()>;
}

/// Parse whitespace-separated hex bytes from an ELM/STN response, ignoring `SEARCHING...`,
/// `OK`, empty lines, and prompt characters.
pub fn parse_hex_response(response: &str) -> Result<Vec<u8>> {
    // ELM often uses CR without LF — normalize so we can split lines.
    let normalized = response.replace("\r\n", "\n").replace('\r', "\n");
    let mut bytes = Vec::new();
    for line in normalized.lines() {
        let line = line.trim().trim_end_matches('>');
        if line.is_empty() {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        if upper.contains("NO DATA")
            || upper.contains("UNABLE TO CONNECT")
            || upper.contains("CAN ERROR")
            || upper.contains("BUS ERROR")
            || upper.contains("BUFFER FULL")
        {
            return Err(crate::error::Error::Adapter(line.to_string()));
        }
        if upper.contains("SEARCHING")
            || upper == "OK"
            || upper == "STOPPED"
            || upper.starts_with("AT")
            || upper.starts_with("ST")
            || upper.contains('?')
            || upper == "ERROR"
            || upper.contains("BUS INIT")
        {
            continue;
        }

        // Split on spaces so "62 F190 41 42" does not glue into odd-length tokens incorrectly.
        // Prefer whole-line hex-digit filter when the line is mostly hex.
        let cleaned: String = line.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if cleaned.is_empty() {
            continue;
        }
        if cleaned.len() % 2 != 0 {
            return Err(crate::error::Error::UnexpectedResponse(format!(
                "odd hex length in line: {line}"
            )));
        }
        bytes.extend(hex::decode(&cleaned)?);
    }

    if bytes.is_empty() {
        return Err(crate::error::Error::UnexpectedResponse(format!(
            "no hex data in response: {response:?}"
        )));
    }
    Ok(bytes)
}

/// ELM/STN often prefixes CAN responses with the echo header / PCI. Strip leading CAN ID
/// if present (3 or 8 hex digits) when the payload looks like ISO-TP or raw UDS.
pub fn normalize_elm_payload(bytes: &[u8]) -> Vec<u8> {
    // Already a UDS positive/negative response starting with SID
    if !bytes.is_empty() {
        let sid = bytes[0];
        if (0x40..=0x7E).contains(&sid) || sid == 0x7F {
            return bytes.to_vec();
        }
    }

    // Single-frame ISO-TP: PCI in high nibble of first byte
    if !bytes.is_empty() {
        let pci_type = bytes[0] >> 4;
        if pci_type <= 2 {
            return bytes.to_vec();
        }
    }

    bytes.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_hex_line() {
        let data = parse_hex_response("62 F1 90 31 32\r>").unwrap();
        assert_eq!(data, vec![0x62, 0xF1, 0x90, 0x31, 0x32]);
    }

    #[test]
    fn parse_multiline_ignores_searching() {
        let data = parse_hex_response("SEARCHING...\r62 F190 41 42\r>").unwrap();
        assert_eq!(data, vec![0x62, 0xF1, 0x90, 0x41, 0x42]);
    }

    #[test]
    fn parse_no_data_is_error() {
        assert!(parse_hex_response("NO DATA\r>").is_err());
    }
}
