//! String/scalar helpers ported from `src/mapstring.cpp`.

use std::borrow::Cow;

pub const HONOUR_STRINGS: u32 = 0x0001;
pub const ALLOW_EMPTY_TOKENS: u32 = 0x0002;
pub const PRESERVE_QUOTES: u32 = 0x0004;
pub const PRESERVE_ESCAPES: u32 = 0x0008;
pub const STRIP_LEAD_SPACES: u32 = 0x0010;
pub const STRIP_END_SPACES: u32 = 0x0020;

pub fn string_to_int(input: &str, base: u32) -> Option<i32> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }

    let (sign, digits) = if let Some(rest) = s.strip_prefix('-') {
        (-1i64, rest)
    } else if let Some(rest) = s.strip_prefix('+') {
        (1i64, rest)
    } else {
        (1i64, s)
    };

    let value = i64::from_str_radix(digits, base).ok()?;
    i32::try_from(sign * value).ok()
}

pub fn string_to_double(input: &str) -> Option<f64> {
    input.trim_start().parse::<f64>().ok()
}

pub fn string_split(input: &str, delimiter: char) -> Vec<String> {
    let mut tokens = Vec::with_capacity(1);
    let mut current = String::new();
    let mut last_was_delim = false;

    for c in input.chars() {
        if c == delimiter {
            if !last_was_delim {
                tokens.push(std::mem::take(&mut current));
            }
            last_was_delim = true;
        } else {
            current.push(c);
            last_was_delim = false;
        }
    }

    tokens.push(current);
    tokens
}

pub fn string_split_complex(input: &str, delimiters: &str, flags: u32) -> Vec<String> {
    let bytes = input.as_bytes();
    let delims = delimiters.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < bytes.len() {
        let mut token = Vec::<u8>::new();
        let mut in_string = false;
        let mut start_string = true;

        while i < bytes.len() {
            let b = bytes[i];
            if !in_string && delims.contains(&b) {
                i += 1;
                break;
            }

            if (flags & HONOUR_STRINGS) != 0 && b == b'"' {
                if (flags & PRESERVE_QUOTES) != 0 {
                    token.push(b'"');
                }
                in_string = !in_string;
                i += 1;
                continue;
            }

            if in_string
                && b == b'\\'
                && i + 1 < bytes.len()
                && (bytes[i + 1] == b'"' || bytes[i + 1] == b'\\')
            {
                if (flags & PRESERVE_ESCAPES) != 0 {
                    token.push(b'\\');
                }
                i += 1;
            }

            if !in_string
                && (flags & STRIP_LEAD_SPACES) != 0
                && start_string
                && (bytes[i] as char).is_ascii_whitespace()
            {
                i += 1;
                continue;
            }

            start_string = false;
            token.push(bytes[i]);
            i += 1;
        }

        if !in_string && (flags & STRIP_END_SPACES) != 0 {
            while token
                .last()
                .is_some_and(|c| (*c as char).is_ascii_whitespace())
            {
                token.pop();
            }
        }

        if !token.is_empty() || (flags & ALLOW_EMPTY_TOKENS) != 0 {
            out.push(String::from_utf8(token).expect("token stays valid UTF-8"));
        }
    }

    if !bytes.is_empty()
        && (flags & ALLOW_EMPTY_TOKENS) != 0
        && out.last().is_some()
        && delims.contains(bytes.last().expect("checked non-empty"))
    {
        out.push(String::new());
    }

    out
}

pub fn string_tokenize(input: &str, delimiter: &str, preserve_quote: bool) -> Vec<String> {
    let bytes = input.as_bytes();
    let delim = delimiter.as_bytes();
    let mut out = Vec::new();
    let mut token = Vec::<u8>::new();
    let mut i = 0usize;
    let mut in_quotes = false;

    while i < bytes.len() {
        if in_quotes && i + 1 < bytes.len() && bytes[i] == b'"' && bytes[i + 1] == b'"' {
            if preserve_quote {
                token.push(b'"');
            }
            token.push(b'"');
            i += 2;
            continue;
        }

        if bytes[i] == b'"' {
            if preserve_quote {
                token.push(b'"');
            }
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }

        if !in_quotes && i + delim.len() <= bytes.len() && &bytes[i..i + delim.len()] == delim {
            out.push(
                String::from_utf8(std::mem::take(&mut token)).expect("token stays valid UTF-8"),
            );
            i += delim.len();
            continue;
        }

        token.push(bytes[i]);
        i += 1;
    }

    out.push(String::from_utf8(token).expect("token stays valid UTF-8"));
    out
}

pub fn needs_url_encoding(c: u8) -> bool {
    !((0x61..=0x7A).contains(&c)
        || (0x41..=0x5A).contains(&c)
        || (0x30..=0x39).contains(&c)
        || (0x27..=0x2A).contains(&c)
        || (0x2D..=0x2E).contains(&c)
        || c == 0x5F
        || c == 0x21
        || c == 0x7E)
}

/// Direct port naming of `msEncodeChar()`: returns `true` when `c` should be
/// percent-encoded, and `false` when it can be emitted as-is.
pub fn encode_char(c: u8) -> bool {
    needs_url_encoding(c)
}

pub fn encode_url(data: &str) -> String {
    encode_url_except(data, None)
}

pub fn encode_url_except(data: &str, except: Option<u8>) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(data.len());

    for &b in data.as_bytes() {
        if except == Some(b) {
            out.push(b as char);
        } else if needs_url_encoding(b) {
            out.push('%');
            out.push(HEX[(b / 16) as usize] as char);
            out.push(HEX[(b % 16) as usize] as char);
        } else {
            out.push(b as char);
        }
    }

    out
}

pub fn escape_json_like_string(input: &str, quote: char) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 32 => {
                out.push_str("\\u00");
                out.push_str(&format!("{:02X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

pub fn escape_json_string(input: &str) -> String {
    escape_json_like_string(input, '"')
}

pub fn encode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Mirrors `msStringIsInteger()`, which accepts only non-empty digit-only
/// strings (`0-9`) and rejects signs/whitespace.
pub fn string_is_integer(input: &str) -> bool {
    !input.is_empty() && input.chars().all(|c| c.is_ascii_digit())
}

pub fn string_escape(input: &str) -> Cow<'_, str> {
    let needs_escape = input.chars().any(|c| c == '"' || c == '\'');
    if !needs_escape {
        return Cow::Borrowed(input);
    }

    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if c == '"' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    Cow::Owned(out)
}

pub fn string_unescape(input: &str, escape_char: char) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == escape_char && chars.peek() == Some(&escape_char) {
            out.push(escape_char);
            chars.next();
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbers_like_mapstring() {
        assert_eq!(string_to_int("10", 10), Some(10));
        assert_eq!(string_to_int("  -10", 10), Some(-10));
        assert_eq!(string_to_int("A", 16), Some(10));
        assert_eq!(string_to_int("10x", 10), None);
        assert_eq!(string_to_double("  3.15"), Some(3.15));
        assert_eq!(string_to_double("3.14x"), None);
    }

    #[test]
    fn split_and_tokenize_behave_like_c() {
        assert_eq!(string_split("a,,b,", ','), vec!["a", "b", ""]);
        assert_eq!(
            string_split_complex("a,\"b,c\",d", ",", HONOUR_STRINGS),
            vec!["a", "b,c", "d"]
        );
        assert_eq!(
            string_split_complex("å,\"é,ö\",ç", ",", HONOUR_STRINGS),
            vec!["å", "é,ö", "ç"]
        );
        assert_eq!(
            string_split_complex(",a,,", ",", ALLOW_EMPTY_TOKENS),
            vec!["", "a", "", ""]
        );
        assert_eq!(
            string_tokenize("a||\"b||c\"||d", "||", false),
            vec!["a", "b||c", "d"]
        );
        assert_eq!(
            string_tokenize("å||\"é||ö\"||ç", "||", false),
            vec!["å", "é||ö", "ç"]
        );
    }

    #[test]
    fn supports_encoding_and_escaping_helpers() {
        assert_eq!(encode_url("a b"), "a%20b");
        assert_eq!(encode_url_except("a/b", Some(b'/')), "a/b");
        assert_eq!(escape_json_string("x\n\"\\"), "x\\n\\\"\\\\");
        assert_eq!(encode_html_entities("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn integer_escape_unescape_helpers() {
        assert!(string_is_integer("0123"));
        assert!(!string_is_integer("12a"));
        assert_eq!(string_escape("a\"b'c"), "a\\\"b\\'c");
        assert_eq!(string_unescape("a..b....c", '.'), "a.b..c");
    }
}
