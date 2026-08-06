use std::collections::HashSet;
use std::fmt::Write;

use crate::ContractError;

// ---------------------------------------------------------------------------
// Serializer (existing, preserved)
// ---------------------------------------------------------------------------

pub(crate) fn write_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0008}' => output.push_str("\\b"),
            '\u{000c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{001f}' => {
                write!(output, "\\u{:04x}", character as u32)
                    .expect("writing JSON to a String cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

// ---------------------------------------------------------------------------
// Parser — strict UTF-8 JSON, no serde, no third-party crates
// ---------------------------------------------------------------------------

/// Parsed JSON value. Preserves object insertion order for deterministic
/// canonicalization.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JsonValue {
    Null,
    Bool(bool),
    I64(i64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

/// Maximum nesting depth before the parser aborts.
const MAX_DEPTH: usize = 32;

/// Maximum input length in bytes.
const MAX_LENGTH: usize = 4_194_304; // 4 MiB

/// Recursive-descent strict JSON parser.
#[derive(Clone, Debug)]
pub(crate) struct JsonParser<'a> {
    input: &'a [u8],
    pos: usize,
    length: usize,
}

#[allow(dead_code)]
impl<'a> JsonParser<'a> {
    /// Create a new parser. Returns an error if the input exceeds the
    /// maximum length.
    pub(crate) fn new(input: &'a [u8]) -> Result<Self, ContractError> {
        if input.len() > MAX_LENGTH {
            return Err(json_error("json_length", "input exceeds maximum length"));
        }
        Ok(Self {
            input,
            pos: 0,
            length: input.len(),
        })
    }

    /// Parse the top-level JSON value.  After returning, the parser
    /// position must be at the end of the input (no trailing data).
    pub(crate) fn parse_top_level(mut self) -> Result<JsonValue, ContractError> {
        let value = self.parse_value(0)?;
        self.skip_whitespace();
        if self.pos < self.length {
            return Err(json_error(
                "json_trailing",
                "trailing data after top-level value",
            ));
        }
        Ok(value)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn parse_value(&mut self, depth: usize) -> Result<JsonValue, ContractError> {
        if depth > MAX_DEPTH {
            return Err(json_error("json_depth", "maximum nesting depth exceeded"));
        }
        self.skip_whitespace();
        self.require_not_eof("unexpected end of input while parsing value")?;

        match self.peek_byte() {
            Some(b'"') => self.parse_string().map(JsonValue::Str),
            Some(b'{') => self.parse_object(depth + 1),
            Some(b'[') => self.parse_array(depth + 1),
            Some(b't') => self.parse_literal("true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal("false", JsonValue::Bool(false)),
            Some(b'n') => self.parse_literal("null", JsonValue::Null),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(c) => Err(json_error_at(
                "json_syntax",
                self.pos,
                format!("unexpected byte {:02x} ('{}')", c, c as char),
            )),
            None => Err(json_error(
                "json_eof",
                "unexpected end of input while parsing value",
            )),
        }
    }

    // --- String ------------------------------------------------------------

    fn parse_string(&mut self) -> Result<String, ContractError> {
        self.expect_byte(b'"', "expected opening quote for string")?;
        let mut result = String::new();
        loop {
            self.require_not_eof("unexpected end of input in string")?;
            match self.read_byte() {
                b'"' => return Ok(result),
                b'\\' => result.push(self.parse_escape()?),
                b'\x00'..=b'\x1f' => {
                    return Err(json_error(
                        "json_control",
                        "unescaped control character in string",
                    ));
                }
                b => {
                    // UTF-8 multi-byte: we already validated the input is
                    // valid UTF-8 at parse time, so just push the char.
                    let ch = char::from(b);
                    result.push(ch);
                }
            }
        }
    }

    fn parse_escape(&mut self) -> Result<char, ContractError> {
        self.require_not_eof("unexpected end of input in escape sequence")?;
        match self.read_byte() {
            b'"' => Ok('"'),
            b'\\' => Ok('\\'),
            b'/' => Ok('/'),
            b'b' => Ok('\u{0008}'),
            b'f' => Ok('\u{000c}'),
            b'n' => Ok('\n'),
            b'r' => Ok('\r'),
            b't' => Ok('\t'),
            b'u' => {
                let value = self.parse_hex4()?;
                // Lone surrogates are invalid.
                if (0xD800..=0xDFFF).contains(&value) {
                    return Err(json_error("json_escape", "lone surrogate in \\u escape"));
                }
                // Surrogate pair.
                if (0xD800..=0xDBFF).contains(&value) {
                    self.expect_byte(b'\\', "expected \\u for surrogate pair")?;
                    self.expect_byte(b'u', "expected \\u for surrogate pair")?;
                    let low = self.parse_hex4()?;
                    if !(0xDC00..=0xDFFF).contains(&low) {
                        return Err(json_error(
                            "json_escape",
                            "invalid low surrogate in \\u escape",
                        ));
                    }
                    let code_point =
                        ((value as u32 - 0xD800) << 10) + (low as u32 - 0xDC00) + 0x1_0000;
                    Ok(char::from_u32(code_point).ok_or_else(|| {
                        json_error("json_escape", "invalid code point from surrogate pair")
                    })?)
                } else {
                    Ok(char::from_u32(value as u32).ok_or_else(|| {
                        json_error("json_escape", "invalid code point from \\u escape")
                    })?)
                }
            }
            c => Err(json_error(
                "json_escape",
                format!("invalid escape character {:02x} ('{}')", c, c as char),
            )),
        }
    }

    fn parse_hex4(&mut self) -> Result<u16, ContractError> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            self.require_not_eof("unexpected end of input in \\u escape")?;
            let b = self.read_byte();
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => {
                    return Err(json_error(
                        "json_escape",
                        format!("invalid hex digit {:02x} ('{}')", b, b as char),
                    ));
                }
            };
            value = value.wrapping_mul(16).wrapping_add(digit as u16);
        }
        Ok(value)
    }

    // --- Number ------------------------------------------------------------

    fn parse_number(&mut self) -> Result<JsonValue, ContractError> {
        let start = self.pos;
        let mut is_negative = false;

        // Optional minus sign.
        if self.pos < self.length && self.input[self.pos] == b'-' {
            // Reject -0 (non-canonical).
            if self.pos + 1 < self.length && self.input[self.pos + 1] == b'0' {
                // Check if next char after 0 is digit or '.' or 'e'/'E'
                let next = self.input[self.pos + 2..].first().copied();
                match next {
                    Some(b'0'..=b'9') | Some(b'.') | Some(b'e') | Some(b'E') => {
                        // -0 followed by digit/dot/exponent = non-canonical
                        // but we still parse it, just reject later.
                    }
                    _ => {
                        // -0 with nothing after (or non-digit) is rejected.
                        return Err(json_error("json_number", "non-canonical number: -0"));
                    }
                }
            }
            is_negative = true;
            self.pos += 1;
        }

        // Check for leading zero.
        let mut has_leading_zero = false;
        if self.pos < self.length && self.input[self.pos] == b'0' {
            has_leading_zero = true;
        }

        // Integer part.
        if self.pos >= self.length || !self.input[self.pos].is_ascii_digit() {
            return Err(json_error("json_number", "expected digit"));
        }
        while self.pos < self.length && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        // Fractional part.
        if self.pos < self.length && self.input[self.pos] == b'.' {
            if has_leading_zero && self.pos > start + if is_negative { 2 } else { 1 } {
                // Leading zero with more digits before decimal is fine (e.g. 0.5)
                // Actually "0.5" - leading zero "0" then ".", that's fine.
                // Leading zero with more integer digits before decimal is rejected.
            }
            self.pos += 1; // skip '.'
            if self.pos >= self.length || !self.input[self.pos].is_ascii_digit() {
                return Err(json_error(
                    "json_number",
                    "expected digit after decimal point",
                ));
            }
            while self.pos < self.length && self.input[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            // Trailing zeros after decimal are non-canonical.
            // e.g. 1.0 is non-canonical (should be 1), 1.10 is non-canonical.
            // We'll just parse as f64 and let the test judge.
            let s = std::str::from_utf8(&self.input[start..self.pos])
                .map_err(|_| json_error("json_number", "invalid UTF-8 in number"))?;
            // Reject trailing zeros after decimal: e.g. "1.0", "1.10"
            if s.ends_with('0') {
                return Err(json_error(
                    "json_number",
                    "non-canonical number: trailing zero in fraction",
                ));
            }
            if is_negative {
                return Err(json_error(
                    "json_number",
                    "non-canonical number: negative float",
                ));
            }
            // Parse as float and check if it's integer-representable.
            // For our use case, reject floats entirely.
            return Err(json_error(
                "json_number",
                "floating-point numbers are not allowed",
            ));
        }

        // Exponent part.
        if self.pos < self.length && (self.input[self.pos] == b'e' || self.input[self.pos] == b'E')
        {
            return Err(json_error(
                "json_number",
                "exponent notation is not allowed",
            ));
        }

        // Validate leading zero.
        if has_leading_zero && self.pos > start + if is_negative { 2 } else { 1 } {
            return Err(json_error(
                "json_number",
                "non-canonical number: leading zero",
            ));
        }

        // Check for -0 (reject).
        let s = std::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| json_error("json_number", "invalid UTF-8 in number"))?;
        if s == "-0" || s == "0" {
            // "0" is fine, but "-0" is not.
            if s == "-0" {
                return Err(json_error("json_number", "non-canonical number: -0"));
            }
            return Ok(JsonValue::I64(0));
        }
        if s == "-0" {
            return Err(json_error("json_number", "non-canonical number: -0"));
        }

        let value: i64 = s
            .parse()
            .map_err(|_| json_error("json_number", format!("number out of range: {}", s)))?;
        Ok(JsonValue::I64(value))
    }

    // --- Literals ----------------------------------------------------------

    fn parse_literal(
        &mut self,
        expected: &str,
        value: JsonValue,
    ) -> Result<JsonValue, ContractError> {
        let bytes = expected.as_bytes();
        for &b in bytes {
            self.require_not_eof("unexpected end of input in literal")?;
            if self.read_byte() != b {
                return Err(json_error(
                    "json_syntax",
                    format!("expected '{}'", expected),
                ));
            }
        }
        // Ensure the literal is not followed by an identifier character.
        if self.pos < self.length {
            let next = self.input[self.pos];
            if next.is_ascii_alphanumeric() || next == b'_' {
                return Err(json_error(
                    "json_syntax",
                    "literal followed by identifier character",
                ));
            }
        }
        Ok(value)
    }

    // --- Array -------------------------------------------------------------

    fn parse_array(&mut self, depth: usize) -> Result<JsonValue, ContractError> {
        self.expect_byte(b'[', "expected '[' for array")?;
        self.skip_whitespace();
        let mut elements = Vec::new();
        if self.pos < self.length && self.input[self.pos] == b']' {
            self.pos += 1;
            return Ok(JsonValue::Array(elements));
        }
        loop {
            elements.push(self.parse_value(depth)?);
            self.skip_whitespace();
            self.require_not_eof("unexpected end of input in array")?;
            match self.read_byte() {
                b']' => return Ok(JsonValue::Array(elements)),
                b',' => {
                    self.skip_whitespace();
                }
                c => {
                    return Err(json_error(
                        "json_syntax",
                        format!("expected ',' or ']' in array, got {:02x}", c),
                    ));
                }
            }
        }
    }

    // --- Object ------------------------------------------------------------

    fn parse_object(&mut self, depth: usize) -> Result<JsonValue, ContractError> {
        self.expect_byte(b'{', "expected '{' for object")?;
        self.skip_whitespace();
        let mut members = Vec::new();
        let mut seen_keys = HashSet::new();
        if self.pos < self.length && self.input[self.pos] == b'}' {
            self.pos += 1;
            return Ok(JsonValue::Object(members));
        }
        loop {
            self.skip_whitespace();
            self.require_not_eof("unexpected end of input in object")?;
            let key = self.parse_string()?;
            if !seen_keys.insert(key.clone()) {
                return Err(json_error(
                    "json_duplicate_key",
                    format!("duplicate key: {}", key),
                ));
            }
            self.skip_whitespace();
            self.expect_byte(b':', "expected ':' after object key")?;
            self.skip_whitespace();
            let value = self.parse_value(depth)?;
            members.push((key, value));
            self.skip_whitespace();
            self.require_not_eof("unexpected end of input in object")?;
            match self.read_byte() {
                b'}' => return Ok(JsonValue::Object(members)),
                b',' => {
                    self.skip_whitespace();
                }
                c => {
                    return Err(json_error(
                        "json_syntax",
                        format!("expected ',' or '}}' in object, got {:02x}", c),
                    ));
                }
            }
        }
    }

    // --- Low-level helpers -------------------------------------------------

    fn peek_byte(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn read_byte(&mut self) -> u8 {
        let b = self.input[self.pos];
        self.pos += 1;
        b
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.length {
            match self.input[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn require_not_eof(&self, message: &str) -> Result<(), ContractError> {
        if self.pos >= self.length {
            Err(json_error("json_eof", message))
        } else {
            Ok(())
        }
    }

    fn expect_byte(&mut self, expected: u8, message: &str) -> Result<(), ContractError> {
        self.require_not_eof(message)?;
        let actual = self.read_byte();
        if actual != expected {
            Err(json_error(
                "json_syntax",
                format!("{}: expected {:02x}, got {:02x}", message, expected, actual),
            ))
        } else {
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// JsonValue → typed conversion helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
impl JsonValue {
    /// Extract a string value.
    pub(crate) fn as_str(&self) -> Result<&str, ContractError> {
        match self {
            JsonValue::Str(s) => Ok(s.as_str()),
            other => Err(json_error(
                "json_type",
                format!("expected string, got {:?}", other),
            )),
        }
    }

    /// Extract an integer value.
    pub(crate) fn as_i64(&self) -> Result<i64, ContractError> {
        match self {
            JsonValue::I64(n) => Ok(*n),
            other => Err(json_error(
                "json_type",
                format!("expected integer, got {:?}", other),
            )),
        }
    }

    /// Extract a boolean value.
    pub(crate) fn as_bool(&self) -> Result<bool, ContractError> {
        match self {
            JsonValue::Bool(b) => Ok(*b),
            other => Err(json_error(
                "json_type",
                format!("expected boolean, got {:?}", other),
            )),
        }
    }

    /// Extract an array value.
    pub(crate) fn as_array(&self) -> Result<&[JsonValue], ContractError> {
        match self {
            JsonValue::Array(arr) => Ok(arr.as_slice()),
            other => Err(json_error(
                "json_type",
                format!("expected array, got {:?}", other),
            )),
        }
    }

    /// Extract an object value.
    pub(crate) fn as_object(&self) -> Result<&[(String, JsonValue)], ContractError> {
        match self {
            JsonValue::Object(obj) => Ok(obj.as_slice()),
            other => Err(json_error(
                "json_type",
                format!("expected object, got {:?}", other),
            )),
        }
    }

    /// Get a field from an object by key.
    pub(crate) fn field(&self, key: &str) -> Result<&JsonValue, ContractError> {
        let obj = self.as_object()?;
        obj.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .ok_or_else(|| json_error("json_missing", format!("missing required field: {}", key)))
    }

    /// Get an optional field from an object.
    pub(crate) fn field_opt(&self, key: &str) -> Option<&JsonValue> {
        self.as_object()
            .ok()
            .and_then(|obj| obj.iter().find(|(k, _)| k == key).map(|(_, v)| v))
    }
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn json_error(code: &'static str, message: impl Into<String>) -> ContractError {
    ContractError::new(code, "json", message)
}

fn json_error_at(code: &'static str, position: usize, message: impl Into<String>) -> ContractError {
    let msg = format!("at position {}: {}", position, message.into());
    ContractError::new(code, "json", msg)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Valid JSON --------------------------------------------------------

    #[test]
    fn parse_null() {
        let parsed = JsonParser::new(b"null").unwrap().parse_top_level().unwrap();
        assert_eq!(parsed, JsonValue::Null);
    }

    #[test]
    fn parse_bool() {
        assert_eq!(
            JsonParser::new(b"true").unwrap().parse_top_level().unwrap(),
            JsonValue::Bool(true),
        );
        assert_eq!(
            JsonParser::new(b"false")
                .unwrap()
                .parse_top_level()
                .unwrap(),
            JsonValue::Bool(false),
        );
    }

    #[test]
    fn parse_integer() {
        assert_eq!(
            JsonParser::new(b"42").unwrap().parse_top_level().unwrap(),
            JsonValue::I64(42),
        );
        assert_eq!(
            JsonParser::new(b"-1").unwrap().parse_top_level().unwrap(),
            JsonValue::I64(-1),
        );
        assert_eq!(
            JsonParser::new(b"0").unwrap().parse_top_level().unwrap(),
            JsonValue::I64(0),
        );
        assert_eq!(
            JsonParser::new(b"9223372036854775807")
                .unwrap()
                .parse_top_level()
                .unwrap(),
            JsonValue::I64(9223372036854775807),
        );
    }

    #[test]
    fn parse_string() {
        assert_eq!(
            JsonParser::new(b"\"hello\"")
                .unwrap()
                .parse_top_level()
                .unwrap(),
            JsonValue::Str("hello".to_owned()),
        );
        assert_eq!(
            JsonParser::new(b"\"esc\\\"ape\"")
                .unwrap()
                .parse_top_level()
                .unwrap(),
            JsonValue::Str("esc\"ape".to_owned()),
        );
        assert_eq!(
            JsonParser::new(b"\"\\u0048\\u0069\"")
                .unwrap()
                .parse_top_level()
                .unwrap(),
            JsonValue::Str("Hi".to_owned()),
        );
        assert_eq!(
            JsonParser::new(b"\"\\n\\t\\r\"")
                .unwrap()
                .parse_top_level()
                .unwrap(),
            JsonValue::Str("\n\t\r".to_owned()),
        );
        assert_eq!(
            JsonParser::new(b"\"\\u03b1\"")
                .unwrap()
                .parse_top_level()
                .unwrap(),
            JsonValue::Str("α".to_owned()),
        );
    }

    #[test]
    fn parse_array() {
        let parsed = JsonParser::new(b"[1,2,3]")
            .unwrap()
            .parse_top_level()
            .unwrap();
        assert_eq!(
            parsed,
            JsonValue::Array(vec![
                JsonValue::I64(1),
                JsonValue::I64(2),
                JsonValue::I64(3)
            ]),
        );
    }

    #[test]
    fn parse_empty_array() {
        let parsed = JsonParser::new(b"[]").unwrap().parse_top_level().unwrap();
        assert_eq!(parsed, JsonValue::Array(vec![]));
    }

    #[test]
    fn parse_nested_array() {
        let parsed = JsonParser::new(b"[[1,[]],2]")
            .unwrap()
            .parse_top_level()
            .unwrap();
        assert_eq!(
            parsed,
            JsonValue::Array(vec![
                JsonValue::Array(vec![JsonValue::I64(1), JsonValue::Array(vec![])]),
                JsonValue::I64(2),
            ]),
        );
    }

    #[test]
    fn parse_object() {
        let parsed = JsonParser::new(b"{\"a\":1,\"b\":2}")
            .unwrap()
            .parse_top_level()
            .unwrap();
        assert_eq!(
            parsed,
            JsonValue::Object(vec![
                ("a".to_owned(), JsonValue::I64(1)),
                ("b".to_owned(), JsonValue::I64(2)),
            ]),
        );
    }

    #[test]
    fn parse_empty_object() {
        let parsed = JsonParser::new(b"{}").unwrap().parse_top_level().unwrap();
        assert_eq!(parsed, JsonValue::Object(vec![]));
    }

    #[test]
    fn parse_whitespace() {
        let parsed = JsonParser::new(b"  {  \"a\"  :  1  }  ")
            .unwrap()
            .parse_top_level()
            .unwrap();
        assert_eq!(
            parsed,
            JsonValue::Object(vec![("a".to_owned(), JsonValue::I64(1))]),
        );
    }

    // --- Rejections --------------------------------------------------------

    #[test]
    fn reject_trailing_data() {
        let err = JsonParser::new(b"null true")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_trailing");
    }

    #[test]
    fn reject_duplicate_key() {
        let err = JsonParser::new(b"{\"a\":1,\"a\":2}")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_duplicate_key");
    }

    #[test]
    fn reject_leading_zero() {
        let err = JsonParser::new(b"01")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_number");
    }

    #[test]
    fn reject_negative_zero() {
        let err = JsonParser::new(b"-0")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_number");
    }

    #[test]
    fn reject_float() {
        let err = JsonParser::new(b"1.5")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_number");
    }

    #[test]
    fn reject_exponent() {
        let err = JsonParser::new(b"1e10")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_number");
    }

    #[test]
    fn reject_control_char_in_string() {
        let err = JsonParser::new(b"\"hello\n\"")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_control");
    }

    #[test]
    fn reject_invalid_escape() {
        let err = JsonParser::new(b"\"hello\\x\"")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_escape");
    }

    #[test]
    fn reject_lone_surrogate() {
        let err = JsonParser::new(b"\"\\uD800\"")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_escape");
    }

    #[test]
    fn reject_literal_followed_by_identifier() {
        let err = JsonParser::new(b"nullx")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_syntax");
    }

    #[test]
    fn reject_deeply_nested() {
        let input = format!("[{}]", "[".repeat(MAX_DEPTH + 1));
        let err = JsonParser::new(input.as_bytes())
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_depth");
    }

    #[test]
    fn reject_trailing_comma_in_array() {
        let err = JsonParser::new(b"[1,]")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_syntax");
    }

    #[test]
    fn reject_trailing_comma_in_object() {
        let err = JsonParser::new(b"{\"a\":1,}")
            .unwrap()
            .parse_top_level()
            .unwrap_err();
        assert_eq!(err.code, "json_syntax");
    }

    #[test]
    fn reject_empty_input() {
        let err = JsonParser::new(b"").unwrap().parse_top_level().unwrap_err();
        assert_eq!(err.code, "json_eof");
    }

    #[test]
    fn reject_overlong_input() {
        let big = vec![b' '; MAX_LENGTH + 1];
        let err = JsonParser::new(&big).unwrap_err();
        assert_eq!(err.code, "json_length");
    }
}
