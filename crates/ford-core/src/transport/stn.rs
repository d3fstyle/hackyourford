use super::{DiagAdapter, Transport};
use crate::error::{Error, Result};
use serialport::{ClearBuffer, SerialPort};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// Which vehicle CAN bus to talk on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanBus {
    /// High-speed CAN (~500 kbps) — PCM and legislated OBD.
    Hs,
    /// Medium-speed CAN (~125 kbps) — BCM, IPC, body modules (needs STN / MS-CAN).
    Ms,
}

/// Alias used in profiles.
pub type Bus = CanBus;

/// Detected adapter identity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterInfo {
    pub identity: String,
    pub is_stn: bool,
    pub voltage: Option<f32>,
}

/// Serial port listing for the UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SerialPortInfo {
    pub name: String,
    pub port_type: String,
}

/// List available serial ports (RFCOMM, USB CDC, etc.).
pub fn list_serial_ports() -> Result<Vec<SerialPortInfo>> {
    let ports = serialport::available_ports()?;
    Ok(ports
        .into_iter()
        .map(|p| {
            let port_type = match p.port_type {
                serialport::SerialPortType::UsbPort(info) => {
                    format!(
                        "USB {:04x}:{:04x} {}",
                        info.vid,
                        info.pid,
                        info.product.unwrap_or_default()
                    )
                }
                serialport::SerialPortType::BluetoothPort => "Bluetooth".into(),
                serialport::SerialPortType::PciPort => "PCI".into(),
                serialport::SerialPortType::Unknown => "Unknown".into(),
            };
            SerialPortInfo {
                name: p.port_name,
                port_type,
            }
        })
        .collect())
}

/// STN / ELM327 serial transport (OBDLink MX+, EX, clones).
pub struct StnTransport {
    port: Box<dyn SerialPort>,
    timeout: Duration,
    pub info: Option<AdapterInfo>,
    current_bus: Option<CanBus>,
    current_header: Option<u32>,
}

impl StnTransport {
    pub fn open(path: &str, baud: u32) -> Result<Self> {
        let port = serialport::new(path, baud)
            .timeout(Duration::from_millis(200))
            .open()?;
        Ok(Self {
            port,
            timeout: Duration::from_secs(5),
            info: None,
            current_bus: None,
            current_header: None,
        })
    }

    /// Read adapter battery voltage (ATRV).
    pub fn read_voltage(&mut self) -> Result<Option<f32>> {
        let s = self.command("ATRV")?;
        Ok(parse_voltage(&s))
    }

    fn drain(&mut self) {
        let mut buf = [0u8; 512];
        let _ = self.port.clear(ClearBuffer::All);
        loop {
            match self.port.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => continue,
            }
        }
    }

    fn read_until_prompt(&mut self) -> Result<String> {
        let deadline = Instant::now() + self.timeout;
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];

        while Instant::now() < deadline {
            match self.port.read(&mut byte) {
                Ok(1) => {
                    buf.push(byte[0]);
                    if byte[0] == b'>' {
                        break;
                    }
                }
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => return Err(e.into()),
            }
        }

        if !buf.contains(&b'>') {
            return Err(Error::Timeout);
        }

        let text = String::from_utf8_lossy(&buf).to_string();
        Ok(strip_prompt(&text))
    }

    fn write_raw(&mut self, data: &[u8]) -> Result<()> {
        self.port.write_all(data)?;
        self.port.flush()?;
        Ok(())
    }
}

impl Transport for StnTransport {
    fn command(&mut self, cmd: &str) -> Result<String> {
        self.drain();
        let line = format!("{cmd}\r");
        self.port.write_all(line.as_bytes())?;
        self.port.flush()?;
        let response = self.read_until_prompt()?;
        let trimmed = response.trim().to_string();
        if trimmed.contains('?') && trimmed.len() < 8 {
            return Err(Error::Adapter(format!("unknown command: {cmd} -> {trimmed}")));
        }
        Ok(trimmed)
    }

    fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
        let _ = self.port.set_timeout(Duration::from_millis(200));
    }
}

impl DiagAdapter for StnTransport {
    fn initialize(&mut self) -> Result<AdapterInfo> {
        let _ = self.command("ATZ");
        std::thread::sleep(Duration::from_millis(800));
        self.drain();

        let _ = self.command("ATE0"); // echo off
        let _ = self.command("ATL0"); // no linefeeds preferred (we still handle both)
        let _ = self.command("ATS0"); // spaces off for denser hex (we still parse spaces)
        let _ = self.command("ATH1"); // headers on — helps multi-module debugging
        let _ = self.command("ATAL"); // allow long messages

        let identity = self.command("ATI")?.trim().to_string();
        let is_stn = identity.to_ascii_uppercase().contains("STN")
            || identity.to_ascii_uppercase().contains("OBDLINK");

        let voltage = self
            .command("ATRV")
            .ok()
            .and_then(|s| parse_voltage(&s));

        let info = AdapterInfo {
            identity,
            is_stn,
            voltage,
        };
        self.info = Some(info.clone());
        Ok(info)
    }

    fn select_bus(&mut self, bus: CanBus) -> Result<()> {
        if self.current_bus == Some(bus) {
            return Ok(());
        }

        match bus {
            CanBus::Hs => {
                // ISO 15765-4 CAN (11 bit, 500 kbaud)
                self.command("ATSP6")?;
            }
            CanBus::Ms => {
                let is_stn = self.info.as_ref().map(|i| i.is_stn).unwrap_or(false);
                if is_stn {
                    // MS-CAN ISO 15765, 11-bit, 125 kbps, DLC=8
                    self.command("STP 53")?;
                } else {
                    // Cheap ELM clones: often a physical HS/MS switch — still try ATSPB
                    let _ = self.command("ATSPB");
                }
            }
        }

        self.current_bus = Some(bus);
        self.current_header = None;
        Ok(())
    }

    fn set_header(&mut self, header: u32) -> Result<()> {
        if self.current_header == Some(header) {
            return Ok(());
        }
        let cmd = format!("ATSH{header:03X}");
        self.command(&cmd)?;
        // Receive filter: response is typically request | 0x08 for standard Ford modules
        let resp = header | 0x08;
        let _ = self.command(&format!("ATCRA{resp:03X}"));
        self.current_header = Some(header);
        Ok(())
    }

    fn adapter_info(&self) -> Option<&AdapterInfo> {
        self.info.as_ref()
    }

    fn clear_receive_filter(&mut self) -> Result<()> {
        let _ = self.command("ATAR");
        Ok(())
    }

    fn start_monitor(&mut self) -> Result<()> {
        self.drain();
        // ATMA does not return until stopped — do not wait for prompt.
        self.write_raw(b"ATMA\r")?;
        Ok(())
    }

    fn read_monitor_chunk(&mut self, max_wait: Duration) -> Result<String> {
        let deadline = Instant::now() + max_wait;
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while Instant::now() < deadline {
            match self.port.read(&mut byte) {
                Ok(1) => {
                    if byte[0] == b'>' {
                        break;
                    }
                    buf.push(byte[0]);
                    // Prefer returning on newline batches for snappiness
                    if byte[0] == b'\n' || byte[0] == b'\r' {
                        if buf.len() > 8 {
                            break;
                        }
                    }
                    if buf.len() > 512 {
                        break;
                    }
                }
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if !buf.is_empty() {
                        break;
                    }
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    fn stop_monitor(&mut self) -> Result<()> {
        // Interrupt ATMA with a single CR, then restore prompt.
        let _ = self.write_raw(b"\r");
        std::thread::sleep(Duration::from_millis(50));
        match self.read_until_prompt() {
            Ok(_) => Ok(()),
            Err(_) => {
                // Force a benign command to resync
                let _ = self.command("ATI");
                Ok(())
            }
        }
    }
}

fn strip_prompt(s: &str) -> String {
    s.trim_end_matches('>').trim().to_string()
}

fn parse_voltage(s: &str) -> Option<f32> {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    let cleaned = cleaned.replace(',', ".");
    cleaned.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_voltage_variants() {
        assert_eq!(parse_voltage("12.3V"), Some(12.3));
        assert_eq!(parse_voltage("14.1"), Some(14.1));
    }

    #[test]
    fn strip_prompt_works() {
        assert_eq!(strip_prompt("OK\r\n>"), "OK");
    }
}
