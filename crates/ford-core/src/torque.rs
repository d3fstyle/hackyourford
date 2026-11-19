//! Minimal Torque equation evaluator for Ford extended PIDs.
//!
//! Supports: A-F (unsigned bytes), signed(X), + - * /, parentheses, literals,
//! Torque bit-shift `<` (e.g. `A<8` ≡ `A << 8`), and optional BARO().

use crate::error::{Error, Result};

/// Evaluate a Torque-format equation against raw data bytes.
/// 
/// # Arguments
/// * `equation` - The Torque equation string (e.g. "A-40", "((A*256)+B)/100")
/// * `data` - The raw byte array from the DID/PID response
/// * `baro` - Optional barometric pressure value for BARO() function
/// 
/// # Returns
/// The calculated numeric value, or an error if evaluation fails.
pub fn evaluate(equation: &str, data: &[u8], baro: Option<f64>) -> Result<f64> {
    let eq = equation.trim();
    if eq.is_empty() {
        return Err(Error::Other("empty equation".into()));
    }
    
    Parser::new(eq, data, baro)
        .parse_expression()
        .map_err(|e| Error::Other(format!("equation error: {}", e)))
}

struct Parser<'a> {
    src: &'a str,
    data: &'a [u8],
    baro: Option<f64>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, data: &'a [u8], baro: Option<f64>) -> Self {
        Self {
            src,
            data,
            baro,
            pos: 0,
        }
    }

    fn parse_expression(&mut self) -> std::result::Result<f64, String> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_ws();
            if self.match_char('+') {
                value += self.parse_term()?;
            } else if self.match_char('-') {
                value -= self.parse_term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_term(&mut self) -> std::result::Result<f64, String> {
        let mut value = self.parse_factor()?;
        loop {
            self.skip_ws();
            if self.match_char('*') {
                value *= self.parse_factor()?;
            } else if self.match_char('/') {
                let divisor = self.parse_factor()?;
                if divisor == 0.0 {
                    return Err("division by zero".into());
                }
                value /= divisor;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_factor(&mut self) -> std::result::Result<f64, String> {
        self.skip_ws();
        
        // Unary plus
        if self.match_char('+') {
            return self.parse_factor();
        }
        
        // Unary minus
        if self.match_char('-') {
            return Ok(-self.parse_factor()?);
        }

        // Parse primary (number, variable, function, or parenthesized expression)
        let mut value = self.parse_primary()?;
        
        // Torque bit-shift: A<8 (only when followed by digit)
        self.skip_ws();
        if self.peek() == Some('<') && self.peek_offset(1).map_or(false, |c| c.is_ascii_digit()) {
            self.pos += 1; // consume '<'
            let shift = self.parse_primary()? as i64;
            value = ((value as i64) << shift) as f64;
        }
        
        Ok(value)
    }

    fn parse_primary(&mut self) -> std::result::Result<f64, String> {
        self.skip_ws();
        
        // Parenthesized expression
        if self.match_char('(') {
            let value = self.parse_expression()?;
            self.expect(')')?;
            return Ok(value);
        }

        // Functions / identifiers
        if self.peek().map_or(false, |c| c.is_ascii_alphabetic()) {
            let name = self.read_ident();
            self.skip_ws();
            
            if self.match_char('(') {
                return match name.to_uppercase().as_str() {
                    "SIGNED" => {
                        let arg = self.parse_expression()?;
                        self.expect(')')?;
                        // Interpret low 8 bits as signed byte
                        let byte = (arg as i64) & 0xFF;
                        let signed = if byte >= 128 { byte - 256 } else { byte };
                        Ok(signed as f64)
                    }
                    "BARO" => {
                        self.expect(')')?;
                        self.baro.ok_or_else(|| "BARO() unavailable".to_string())
                    }
                    _ => Err(format!("unknown function: {}", name)),
                };
            }
            
            // Variable (A-F, L is Torque alias for A)
            return match name.to_uppercase().as_str() {
                "A" => self.byte_at(0),
                "B" => self.byte_at(1),
                "C" => self.byte_at(2),
                "D" => self.byte_at(3),
                "E" => self.byte_at(4),
                "F" => self.byte_at(5),
                "L" => self.byte_at(0), // Torque alias
                _ => Err(format!("unknown variable: {}", name)),
            };
        }

        // Number literal
        if self.peek().map_or(false, |c| c.is_ascii_digit() || c == '.') {
            return self.read_number();
        }

        Err(format!("unexpected character at position {}", self.pos))
    }

    fn byte_at(&self, index: usize) -> std::result::Result<f64, String> {
        self.data
            .get(index)
            .map(|&b| f64::from(b))
            .ok_or_else(|| format!("byte {} missing in data", index))
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src.as_bytes()[self.pos] as char;
            if c.is_ascii_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_string()
    }

    fn read_number(&mut self) -> std::result::Result<f64, String> {
        let start = self.pos;
        while self.pos < self.src.len() {
            let c = self.src.as_bytes()[self.pos] as char;
            if c.is_ascii_digit() || c == '.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.src[start..self.pos]
            .parse::<f64>()
            .map_err(|e| format!("invalid number: {}", e))
    }

    fn skip_ws(&mut self) {
        while self.pos < self.src.len() {
            let c = self.src.as_bytes()[self.pos] as char;
            if c.is_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.peek_offset(0)
    }

    fn peek_offset(&self, offset: usize) -> Option<char> {
        self.src.as_bytes().get(self.pos + offset).map(|&b| b as char)
    }

    fn match_char(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: char) -> std::result::Result<(), String> {
        self.skip_ws();
        if self.peek() != Some(c) {
            return Err(format!("expected '{}' at position {}", c, self.pos));
        }
        self.pos += 1;
        Ok(())
    }
}

/// Format a decoded Ford extended value for display.
pub fn format_value(value: f64, unit: &str) -> String {
    let text = if value == value.trunc() {
        format!("{:.0}", value)
    } else {
        format!("{:.2}", value)
    };
    if unit.is_empty() {
        text
    } else {
        format!("{} {}", text, unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_offset() {
        let data = [80u8];
        let result = evaluate("A-40", &data, None).unwrap();
        assert_eq!(result, 40.0);
    }

    #[test]
    fn test_multi_byte() {
        let data = [0x1A, 0xF8];
        let result = evaluate("((A*256)+B)/4", &data, None).unwrap();
        assert_eq!(result, 1726.0); // RPM calculation
    }

    #[test]
    fn test_bit_shift() {
        let data = [0x12, 0x34];
        let result = evaluate("A<8", &data, None).unwrap();
        assert_eq!(result, 4608.0); // 0x1200
    }

    #[test]
    fn test_bit_shift_plus_b() {
        let data = [0x12, 0x34];
        let result = evaluate("(A<8)+B", &data, None).unwrap();
        assert_eq!(result, 4660.0); // 0x1234
    }

    #[test]
    fn test_signed() {
        let data = [200u8]; // 200 unsigned, -56 signed
        let result = evaluate("signed(A)", &data, None).unwrap();
        assert_eq!(result, -56.0);
    }

    #[test]
    fn test_baro() {
        let data = [];
        let result = evaluate("BARO()", &data, Some(101.3)).unwrap();
        assert_eq!(result, 101.3);
    }

    #[test]
    fn test_baro_missing() {
        let data = [];
        let result = evaluate("BARO()", &data, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_complex_equation() {
        let data = [0x10, 0x20];
        let result = evaluate("((A*256)+B)*(100/256)/100", &data, None).unwrap();
        assert!((result - 16.125).abs() < 0.001);
    }

    #[test]
    fn test_format_value() {
        assert_eq!(format_value(100.0, "rpm"), "100 rpm");
        assert_eq!(format_value(25.5, "°C"), "25.50 °C");
        assert_eq!(format_value(42.0, ""), "42");
    }

    #[test]
    fn test_division_by_zero() {
        let data = [10u8];
        let result = evaluate("A/0", &data, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_byte() {
        let data = [10u8];
        let result = evaluate("B", &data, None);
        assert!(result.is_err());
    }
}
