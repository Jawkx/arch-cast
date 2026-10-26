/// A bounded arithmetic parser, never a shell or language evaluator.
pub fn calculate(query: &str) -> Option<String> {
    let source = query
        .trim()
        .strip_prefix('=')
        .unwrap_or(query.trim())
        .trim()
        .replace('×', "*")
        .replace('÷', "/");
    if source.len() > 160
        || !source.bytes().any(|b| b.is_ascii_digit())
        || !source.contains(['+', '-', '*', '/', '%', '(', ')', '^'])
    {
        return None;
    }
    let mut parser = Parser {
        source: source.as_bytes(),
        at: 0,
        depth: 0,
    };
    let value = parser.expression(0)?;
    parser.whitespace();
    (parser.at == parser.source.len()).then(|| format_number(value))
}

struct Parser<'a> {
    source: &'a [u8],
    at: usize,
    depth: usize,
}
impl Parser<'_> {
    fn whitespace(&mut self) {
        while self
            .source
            .get(self.at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.at += 1;
        }
    }
    fn expression(&mut self, min: u8) -> Option<f64> {
        self.depth += 1;
        if self.depth > 64 {
            return None;
        }
        self.whitespace();
        let mut left = match self.source.get(self.at)? {
            b'+' | b'-' => {
                let minus = self.source[self.at] == b'-';
                self.at += 1;
                self.expression(5)? * if minus { -1. } else { 1. }
            }
            b'(' => {
                self.at += 1;
                let v = self.expression(0)?;
                self.whitespace();
                if self.source.get(self.at) != Some(&b')') {
                    return None;
                }
                self.at += 1;
                v
            }
            _ => self.number()?,
        };
        loop {
            self.whitespace();
            let Some(&op) = self.source.get(self.at) else {
                break;
            };
            let double = self.source.get(self.at + 1) == Some(&op);
            let (lbp, rbp, len) = match op {
                b'+' | b'-' => (1, 2, 1),
                b'*' if double => (7, 6, 2),
                b'^' => (7, 6, 1),
                b'*' | b'/' | b'%' => (3, 4, if op == b'/' && double { 2 } else { 1 }),
                _ => break,
            };
            if lbp < min {
                break;
            }
            self.at += len;
            let right = self.expression(rbp)?;
            left = match op {
                b'^' | b'*' if lbp == 7 => {
                    if right.abs() > 100. || left.abs() > 1e12 {
                        return None;
                    }
                    left.powf(right)
                }
                b'+' => left + right,
                b'-' => left - right,
                b'*' => left * right,
                b'/' if right != 0. => {
                    if len == 2 {
                        (left / right).floor()
                    } else {
                        left / right
                    }
                }
                b'%' if right != 0. => left - (left / right).floor() * right,
                _ => return None,
            };
            if !left.is_finite() || left.abs() > 1e100 {
                return None;
            }
        }
        self.depth -= 1;
        (left.is_finite() && left.abs() <= 1e100).then_some(left)
    }
    fn number(&mut self) -> Option<f64> {
        let start = self.at;
        while self.source.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self.source.get(self.at) == Some(&b'.') {
            self.at += 1;
            while self.source.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
        }
        if matches!(self.source.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.source.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            let exponent = self.at;
            while self.source.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
            if self.at == exponent {
                return None;
            }
        }
        std::str::from_utf8(&self.source[start..self.at])
            .ok()?
            .parse()
            .ok()
    }
}

fn format_number(value: f64) -> String {
    if value == 0. {
        return "0".into();
    }
    // Round to twelve significant digits, matching the old launcher's display precision.
    let scientific = format!("{value:.11e}");
    let (mantissa, exp) = scientific.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if !(-4..12).contains(&exp) {
        format!(
            "{}e{:+03}",
            mantissa.trim_end_matches('0').trim_end_matches('.'),
            exp
        )
    } else {
        let rounded: f64 = scientific.parse().unwrap();
        let fixed = format!("{:.*}", (11 - exp).max(0) as usize, rounded);
        if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            fixed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arithmetic_and_precedence() {
        for (input, expected) in [
            ("2+2", "4"),
            ("= 10 / 4", "2.5"),
            ("2^8", "256"),
            ("-4 * 3", "-12"),
            ("5 % 2", "1"),
            ("(12 + 8) × 3", "60"),
            ("-2^2", "-4"),
            ("2**3**2", "512"),
            ("2^-2", "0.25"),
            ("-5//2", "-3"),
            ("-5%2", "1"),
            ("1e-6 + 0", "1e-06"),
            ("1/3", "0.333333333333"),
        ] {
            assert_eq!(calculate(input).as_deref(), Some(expected), "{input}");
        }
    }
    #[test]
    fn rejects_code_and_expensive_input() {
        for input in [
            "Firefox",
            "1/0",
            "__import__('os').system('true')",
            "9**999999999",
            "2**(2**100)",
            "[1]*1000000",
            "1e100*1e100",
            "(-1)**0.5",
            "True+1",
            "1+",
            "1%0",
            "NaN+1",
            "(2+3",
        ] {
            assert_eq!(calculate(input), None, "{input}");
        }
        assert_eq!(
            calculate(&format!("{}1+1{}", "(".repeat(70), ")".repeat(70))),
            None
        );
    }
}
