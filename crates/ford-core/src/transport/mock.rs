use super::{AdapterInfo, CanBus, DiagAdapter, Transport};
use crate::error::{Error, Result};
use crate::fixture::DemoFixture;
use std::time::Duration;

/// Scripted transport for unit tests and demo mode (no hardware).
pub struct MockTransport {
    /// Queue/list of (command_prefix_or_exact, response_body).
    scripts: Vec<(String, String)>,
    /// When false (default), matching scripts are consumed. When true, they persist for replay.
    looping: bool,
    pub sent: Vec<String>,
    timeout: Duration,
    info: Option<AdapterInfo>,
    /// Preferred adapter info applied in [`initialize`] (from a fixture).
    fixture_adapter: Option<AdapterInfo>,
    current_bus: Option<CanBus>,
    current_header: Option<u32>,
    /// Synthetic CAN monitor lines for demo sniff mode.
    monitor_lines: Vec<String>,
    monitor_idx: usize,
    monitoring: bool,
}

impl MockTransport {
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
            looping: false,
            sent: Vec::new(),
            timeout: Duration::from_secs(1),
            info: None,
            fixture_adapter: None,
            current_bus: None,
            current_header: None,
            monitor_lines: default_monitor_lines(),
            monitor_idx: 0,
            monitoring: false,
        }
    }

    /// Build a looping mock from a demo fixture.
    pub fn from_fixture(fixture: &DemoFixture) -> Self {
        let mut m = Self::new();
        m.looping = true;
        m.fixture_adapter = Some(AdapterInfo {
            identity: fixture.adapter.identity.clone(),
            is_stn: fixture.adapter.is_stn,
            voltage: fixture.adapter.voltage,
        });
        for script in &fixture.scripts {
            m.scripts
                .push((script.match_prefix.clone(), script.response.clone()));
        }
        m
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// When a command starts with `prefix` (or equals it), return `response`.
    pub fn expect(&mut self, prefix: impl Into<String>, response: impl Into<String>) {
        self.scripts.push((prefix.into(), response.into()));
    }

    pub fn expect_exact(&mut self, cmd: impl Into<String>, response: impl Into<String>) {
        self.expect(cmd, response);
    }

    fn find_best_match(&self, upper: &str) -> Option<usize> {
        let mut best: Option<(usize, usize)> = None; // (index, match_len)
        for (i, (prefix, _)) in self.scripts.iter().enumerate() {
            let p = prefix.to_ascii_uppercase();
            if upper == p || upper.starts_with(&p) {
                let len = p.len();
                if best.map(|(_, bl)| len > bl).unwrap_or(true) {
                    best = Some((i, len));
                }
            }
        }
        best.map(|(i, _)| i)
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for MockTransport {
    fn command(&mut self, cmd: &str) -> Result<String> {
        self.sent.push(cmd.to_string());
        let upper = cmd.to_ascii_uppercase();

        if let Some(pos) = self.find_best_match(&upper) {
            let resp = self.scripts[pos].1.clone();
            if !self.looping {
                self.scripts.remove(pos);
            }
            return Ok(resp);
        }

        // Default sane AT responses for init sequences in tests / demo.
        if upper == "ATZ" || upper == "ATI" {
            if let Some(info) = &self.fixture_adapter {
                return Ok(info.identity.clone());
            }
            return Ok("ELM327 v1.5".into());
        }
        if upper == "ATRV" {
            if let Some(v) = self.fixture_adapter.as_ref().and_then(|a| a.voltage) {
                return Ok(format!("{v:.1}V"));
            }
            return Ok("12.0V".into());
        }
        if upper.starts_with("AT") || upper.starts_with("ST") {
            return Ok("OK".into());
        }

        Err(Error::UnexpectedResponse(format!(
            "mock has no script for command: {cmd}"
        )))
    }

    fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
}

impl DiagAdapter for MockTransport {
    fn initialize(&mut self) -> Result<AdapterInfo> {
        let _ = self.command("ATZ");
        let _ = self.command("ATE0");
        let _ = self.command("ATL0");
        let _ = self.command("ATS0");
        let _ = self.command("ATH1");
        let _ = self.command("ATAL");

        let info = if let Some(pref) = &self.fixture_adapter {
            pref.clone()
        } else {
            let identity = self.command("ATI")?.trim().to_string();
            let is_stn = identity.to_ascii_uppercase().contains("STN")
                || identity.to_ascii_uppercase().contains("OBDLINK");
            let voltage = self.command("ATRV").ok().and_then(|s| parse_voltage(&s));
            AdapterInfo {
                identity,
                is_stn,
                voltage,
            }
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
                self.command("ATSP6")?;
            }
            CanBus::Ms => {
                let is_stn = self.info.as_ref().map(|i| i.is_stn).unwrap_or(false);
                if is_stn {
                    self.command("STP 53")?;
                } else {
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
        let resp = header | 0x08;
        let _ = self.command(&format!("ATCRA{resp:03X}"));
        self.current_header = Some(header);
        Ok(())
    }

    fn adapter_info(&self) -> Option<&AdapterInfo> {
        self.info.as_ref()
    }

    fn start_monitor(&mut self) -> Result<()> {
        self.sent.push("ATMA".into());
        self.monitoring = true;
        self.monitor_idx = 0;
        Ok(())
    }

    fn read_monitor_chunk(&mut self, _max_wait: Duration) -> Result<String> {
        if !self.monitoring || self.monitor_lines.is_empty() {
            return Ok(String::new());
        }
        let line = self.monitor_lines[self.monitor_idx % self.monitor_lines.len()].clone();
        self.monitor_idx += 1;
        std::thread::sleep(Duration::from_millis(15));
        Ok(format!("{line}\r"))
    }

    fn stop_monitor(&mut self) -> Result<()> {
        self.monitoring = false;
        self.sent.push("STOP_MONITOR".into());
        Ok(())
    }
}

fn default_monitor_lines() -> Vec<String> {
    vec![
        "3B3 00 00 00 00 00 00 00 00".into(),
        "3B3 00 01 00 00 00 00 00 00".into(),
        "3B3 00 00 00 00 00 00 00 00".into(),
        "7E8 08 41 0C 1A F8 00 00 00".into(),
        "420 11 22 33 44 55 66 77 88".into(),
    ]
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
    fn mock_returns_scripted_response() {
        let mut m = MockTransport::new();
        m.expect("22F190", "62 F1 90 31 32 33");
        let r = m.command("22F190").unwrap();
        assert!(r.contains("62"));
        assert_eq!(m.sent, vec!["22F190"]);
    }

    #[test]
    fn one_shot_consumes_script() {
        let mut m = MockTransport::new();
        m.expect("22F190", "62F19041");
        assert!(m.command("22F190").is_ok());
        assert!(m.command("22F190").is_err());
    }

    #[test]
    fn looping_replays_same_script() {
        let mut m = MockTransport::new();
        m.set_looping(true);
        m.expect("22F190", "62F19041");
        assert_eq!(m.command("22F190").unwrap(), "62F19041");
        assert_eq!(m.command("22F190").unwrap(), "62F19041");
        assert_eq!(m.command("22F190").unwrap(), "62F19041");
    }

    #[test]
    fn longest_prefix_wins() {
        let mut m = MockTransport::new();
        m.set_looping(true);
        m.expect("22", "SHORT");
        m.expect("22F190", "LONG");
        assert_eq!(m.command("22F190").unwrap(), "LONG");
        assert_eq!(m.command("22F188").unwrap(), "SHORT");
    }
}
