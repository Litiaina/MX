use std::collections::{BTreeMap, BTreeSet};

pub fn referenced_fields(expression: &str) -> BTreeSet<String> {
    let source = expression
        .trim()
        .strip_prefix('=')
        .unwrap_or(expression.trim())
        .as_bytes();
    let mut fields = BTreeSet::new();
    let mut position = 0;
    while position < source.len() {
        if source[position].is_ascii_alphabetic() || source[position] == b'_' {
            let start = position;
            position += 1;
            while position < source.len()
                && (source[position].is_ascii_alphanumeric() || source[position] == b'_')
            {
                position += 1;
            }
            let name = String::from_utf8_lossy(&source[start..position]).to_ascii_lowercase();
            let mut next = position;
            while next < source.len() && source[next].is_ascii_whitespace() {
                next += 1;
            }
            if source.get(next) != Some(&b'(') && !matches!(name.as_str(), "pi" | "e") {
                fields.insert(name);
            }
        } else {
            position += 1;
        }
    }
    fields
}

pub fn validate_expression(expression: &str) -> Result<(), String> {
    evaluate_expression(expression, None).map(|_| ())
}

pub fn evaluate_expression(
    expression: &str,
    variables: Option<&BTreeMap<String, f64>>,
) -> Result<f64, String> {
    let expression = expression
        .trim()
        .strip_prefix('=')
        .unwrap_or(expression.trim());
    if expression.is_empty() {
        return Err("Formula expression cannot be empty.".to_string());
    }
    let mut parser = Parser {
        source: expression.as_bytes(),
        position: 0,
        variables,
    };
    let value = parser.expression()?;
    parser.skip_space();
    if parser.position != parser.source.len() {
        return Err(format!(
            "Unexpected character at position {}.",
            parser.position + 1
        ));
    }
    if !value.is_finite() {
        return Err("Formula result is not a finite number.".to_string());
    }
    Ok(value)
}

struct Parser<'a> {
    source: &'a [u8],
    position: usize,
    variables: Option<&'a BTreeMap<String, f64>>,
}

impl Parser<'_> {
    fn expression(&mut self) -> Result<f64, String> {
        self.additive()
    }

    fn additive(&mut self) -> Result<f64, String> {
        let mut value = self.multiplicative()?;
        loop {
            if self.consume(b'+') {
                value += self.multiplicative()?;
            } else if self.consume(b'-') {
                value -= self.multiplicative()?;
            } else {
                return self.finite(value);
            }
        }
    }

    fn multiplicative(&mut self) -> Result<f64, String> {
        let mut value = self.power()?;
        loop {
            if self.consume(b'*') {
                value *= self.power()?;
            } else if self.consume(b'/') {
                let divisor = self.power()?;
                if divisor == 0.0 {
                    return Err("Formula attempted to divide by zero.".to_string());
                }
                value /= divisor;
            } else if self.consume(b'%') {
                let divisor = self.power()?;
                if divisor == 0.0 {
                    return Err("Formula attempted to divide by zero.".to_string());
                }
                value %= divisor;
            } else {
                return self.finite(value);
            }
        }
    }

    fn power(&mut self) -> Result<f64, String> {
        let value = self.unary()?;
        if self.consume(b'^') {
            let exponent = self.power()?;
            self.finite(value.powf(exponent))
        } else {
            Ok(value)
        }
    }

    fn unary(&mut self) -> Result<f64, String> {
        if self.consume(b'+') {
            self.unary()
        } else if self.consume(b'-') {
            self.unary().map(|value| -value)
        } else {
            self.primary()
        }
    }

    fn primary(&mut self) -> Result<f64, String> {
        self.skip_space();
        if self.consume(b'(') {
            let value = self.expression()?;
            self.expect(b')')?;
            return Ok(value);
        }
        if self
            .peek()
            .is_some_and(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return self.number();
        }
        if self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        {
            let name = self.identifier();
            if self.consume(b'(') {
                let arguments = self.arguments()?;
                return self.function(&name, &arguments);
            }
            return match name.as_str() {
                "pi" => Ok(std::f64::consts::PI),
                "e" => Ok(std::f64::consts::E),
                _ => self
                    .variables
                    .map(|variables| variables.get(&name).copied())
                    .unwrap_or(Some(0.0))
                    .ok_or_else(|| format!("Unknown field key '{name}' in formula.")),
            };
        }
        Err(format!(
            "Expected a value at position {}.",
            self.position + 1
        ))
    }

    fn arguments(&mut self) -> Result<Vec<f64>, String> {
        let mut values = Vec::new();
        if self.consume(b')') {
            return Ok(values);
        }
        loop {
            values.push(self.expression()?);
            if self.consume(b')') {
                return Ok(values);
            }
            self.expect(b',')?;
        }
    }

    fn function(&self, name: &str, values: &[f64]) -> Result<f64, String> {
        let result = match name {
            "sum" => values.iter().sum(),
            "product" => values.iter().product(),
            "average" | "avg" if !values.is_empty() => {
                values.iter().sum::<f64>() / values.len() as f64
            }
            "min" if !values.is_empty() => values.iter().copied().fold(f64::INFINITY, f64::min),
            "max" if !values.is_empty() => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            "abs" if values.len() == 1 => values[0].abs(),
            "ceil" if values.len() == 1 => values[0].ceil(),
            "floor" if values.len() == 1 => values[0].floor(),
            "sqrt" if values.len() == 1 && values[0] >= 0.0 => values[0].sqrt(),
            "ln" if values.len() == 1 && values[0] > 0.0 => values[0].ln(),
            "log10" if values.len() == 1 && values[0] > 0.0 => values[0].log10(),
            "exp" if values.len() == 1 => values[0].exp(),
            "sin" if values.len() == 1 => values[0].sin(),
            "cos" if values.len() == 1 => values[0].cos(),
            "tan" if values.len() == 1 => values[0].tan(),
            "pow" | "power" if values.len() == 2 => values[0].powf(values[1]),
            "mod" if values.len() == 2 && values[1] != 0.0 => values[0] % values[1],
            "round" if values.len() == 1 => values[0].round(),
            "round" if values.len() == 2 => {
                let places = values[1].clamp(-12.0, 12.0).trunc() as i32;
                let factor = 10_f64.powi(places.abs());
                if places >= 0 {
                    (values[0] * factor).round() / factor
                } else {
                    (values[0] / factor).round() * factor
                }
            }
            "clamp" if values.len() == 3 && values[1] <= values[2] => {
                values[0].clamp(values[1], values[2])
            }
            _ => {
                return Err(format!(
                    "Unknown function or invalid arguments for '{name}'."
                ));
            }
        };
        self.finite(result)
    }

    fn number(&mut self) -> Result<f64, String> {
        self.skip_space();
        let start = self.position;
        while self.peek().is_some_and(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-')
        }) {
            if matches!(self.peek(), Some(b'+' | b'-'))
                && self.position > start
                && !matches!(self.source[self.position - 1], b'e' | b'E')
            {
                break;
            }
            self.position += 1;
        }
        std::str::from_utf8(&self.source[start..self.position])
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("Invalid number at position {}.", start + 1))
    }

    fn identifier(&mut self) -> String {
        self.skip_space();
        let start = self.position;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.position += 1;
        }
        String::from_utf8_lossy(&self.source[start..self.position]).to_ascii_lowercase()
    }

    fn finite(&self, value: f64) -> Result<f64, String> {
        value
            .is_finite()
            .then_some(value)
            .ok_or_else(|| "Formula result is not a finite number.".to_string())
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.position).copied()
    }

    fn consume(&mut self, expected: u8) -> bool {
        self.skip_space();
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!(
                "Expected '{}' at position {}.",
                expected as char,
                self.position + 1
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_record_math_and_functions() {
        let variables = BTreeMap::from([("value".to_string(), 1000.0), ("khw".to_string(), 170.0)]);
        assert_eq!(
            evaluate_expression("value * khw", Some(&variables)).unwrap(),
            170_000.0
        );
        assert_eq!(
            evaluate_expression("ROUND(AVERAGE(value, khw), 2)", Some(&variables)).unwrap(),
            585.0
        );
    }

    #[test]
    fn rejects_invalid_or_unsafe_expressions() {
        assert!(validate_expression("value +").is_err());
        assert!(evaluate_expression("10 / 0", None).is_err());
        assert!(validate_expression("system('no')").is_err());
    }

    #[test]
    fn extracts_field_keys_without_function_names() {
        assert_eq!(
            referenced_fields("ROUND(value * khw, 2) + tax_rate"),
            BTreeSet::from([
                "khw".to_string(),
                "tax_rate".to_string(),
                "value".to_string()
            ])
        );
    }
}
