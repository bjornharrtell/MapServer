//! Mapfile parser (incremental Rust port of `src/mapfile.c`).
//!
//! This first iteration parses the common hierarchical block structure
//! (`MAP`, `LAYER`, `CLASS`, `STYLE`, etc.) and scalar properties into the
//! generic `ConfigBlock` model.

use crate::config::{ConfigBlock, ConfigValue};
use crate::error::{MapServerError, Result};

const BLOCK_KEYWORDS: &[&str] = &[
    "MAP",
    "LAYER",
    "CLASS",
    "STYLE",
    "WEB",
    "METADATA",
    "PROJECTION",
    "OUTPUTFORMAT",
    "LEGEND",
    "REFERENCE",
    "SCALEBAR",
    "SYMBOL",
    "JOIN",
    "LABEL",
    "FEATURE",
    "GRID",
    "QUERYMAP",
    "CLUSTER",
    "COMPOSITE",
    "LEADER",
    "VALIDATION",
    "PATTERN",
];

pub fn parse_mapfile(input: &str) -> Result<ConfigBlock> {
    let mut stack: Vec<ConfigBlock> = Vec::new();
    let mut root_closed = false;

    for (line_no, raw_line) in input.lines().enumerate() {
        let tokens = tokenize_line(raw_line)?;
        if tokens.is_empty() {
            continue;
        }

        if root_closed {
            return Err(MapServerError::new(
                12,
                "parse_mapfile",
                format!("unexpected tokens after MAP END at line {}", line_no + 1),
            ));
        }

        let keyword = tokens[0].as_bare_str().ok_or_else(|| {
            MapServerError::new(
                12,
                "parse_mapfile",
                format!("expected a bare keyword at line {}", line_no + 1),
            )
        })?;
        let keyword_upper = keyword.to_ascii_uppercase();

        if keyword_upper == "END" {
            if stack.is_empty() {
                return Err(MapServerError::new(
                    12,
                    "parse_mapfile",
                    format!("unexpected END at line {}", line_no + 1),
                ));
            }

            if stack.len() == 1 {
                root_closed = true;
                continue;
            }

            let child = stack.pop().expect("checked stack length");
            stack
                .last_mut()
                .expect("checked parent existence")
                .children
                .push(child);
            continue;
        }

        let starts_block = tokens.len() == 1 && BLOCK_KEYWORDS.contains(&keyword_upper.as_str());
        if starts_block {
            stack.push(ConfigBlock::new(keyword_upper));
            continue;
        }

        let current = stack.last_mut().ok_or_else(|| {
            MapServerError::new(
                12,
                "parse_mapfile",
                format!("property outside of block at line {}", line_no + 1),
            )
        })?;

        let values = tokens[1..]
            .iter()
            .cloned()
            .map(token_to_config_value)
            .collect();
        current.push_property(keyword, values);
    }

    while stack.len() > 1 {
        let child = stack
            .pop()
            .expect("stack has at least two elements while unwinding");
        stack
            .last_mut()
            .expect("stack has parent while unwinding")
            .children
            .push(child);
    }

    let root = stack.pop().ok_or_else(|| {
        MapServerError::new(12, "parse_mapfile", "mapfile did not define any block")
    })?;

    if !root_closed {
        return Err(MapServerError::new(
            12,
            "parse_mapfile",
            "mapfile is missing END for MAP block",
        ));
    }

    if root.kind != "MAP" {
        return Err(MapServerError::new(
            12,
            "parse_mapfile",
            format!("top-level block must be MAP, got {}", root.kind),
        ));
    }

    Ok(root)
}

/// A single lexical token from a mapfile line, tagged with enough
/// information to distinguish the different `expressionObj` source forms
/// recognized by MapServer's lexer (`src/maplexer.l`) before they are turned
/// into a [`ConfigValue`].
#[derive(Debug, Clone, PartialEq)]
enum RawToken {
    /// An unquoted, undelimited word (identifier, number, ON/OFF, ...).
    Bare(String),
    /// A `'...'` or `"..."` quoted string, with a flag for a trailing `i`
    /// suffix (`MS_ISTRING`, case-insensitive string).
    Quoted(String, bool),
    /// A `/pattern/` or `/pattern/i` token (`MS_REGEX`/`MS_IREGEX`).
    Regex(String, bool),
    /// A `{a,b,c}` token (`MS_LIST`).
    List(Vec<String>),
    /// A `(...)` token (`MS_EXPRESSION`), holding the raw source between
    /// the outer parentheses.
    Expression(String),
}

impl RawToken {
    fn as_bare_str(&self) -> Option<&str> {
        match self {
            Self::Bare(text) => Some(text),
            _ => None,
        }
    }
}

fn parse_value(token: &str) -> ConfigValue {
    if token.eq_ignore_ascii_case("ON") || token.eq_ignore_ascii_case("TRUE") {
        return ConfigValue::Bool(true);
    }
    if token.eq_ignore_ascii_case("OFF") || token.eq_ignore_ascii_case("FALSE") {
        return ConfigValue::Bool(false);
    }
    if let Ok(number) = token.parse::<f64>() {
        return ConfigValue::Number(number);
    }
    ConfigValue::String(token.to_string())
}

fn token_to_config_value(token: RawToken) -> ConfigValue {
    match token {
        RawToken::Bare(text) => parse_value(&text),
        RawToken::Quoted(text, false) => ConfigValue::String(text),
        RawToken::Quoted(text, true) => ConfigValue::CaseInsensitiveString(text),
        RawToken::Regex(pattern, case_insensitive) => ConfigValue::Regex {
            pattern,
            case_insensitive,
        },
        RawToken::List(values) => ConfigValue::List(values),
        RawToken::Expression(source) => ConfigValue::Expression(source),
    }
}

/// Find the right-most occurrence of `needle` in `chars[from..]`, mirroring
/// the greedy `\(.*\)`/`\{.*\}` lexer patterns which match up to the last
/// closing delimiter on the line.
fn rfind_from(chars: &[char], from: usize, needle: char) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .skip(from)
        .rfind(|(_, c)| **c == needle)
        .map(|(i, _)| i)
}

/// Find the left-most occurrence of `needle` in `chars[from..]`, mirroring
/// the non-greedy `\/[^*]{1}[^\/]*\/` regex-literal lexer pattern.
fn find_from(chars: &[char], from: usize, needle: char) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, c)| **c == needle)
        .map(|(i, _)| i)
}

/// Tokenize a single mapfile line into [`RawToken`]s.
///
/// This mirrors the relevant subset of `src/maplexer.l`: quoted strings
/// (with optional trailing `i` for case-insensitivity), `/regex/` and
/// `/regex/i` regular expressions, `{a,b,c}` lists, `(...)` logical
/// expressions, and bare (unquoted) words. `#` outside of any of the above
/// starts a line comment.
fn tokenize_line(input: &str) -> Result<Vec<RawToken>> {
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < n {
        let ch = chars[i];

        if ch.is_whitespace() {
            i += 1;
            continue;
        }

        if ch == '#' {
            break;
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut content = String::new();
            i += 1;
            let mut closed = false;
            while i < n {
                let c = chars[i];
                if c == '\\' && i + 1 < n {
                    content.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == quote {
                    i += 1;
                    closed = true;
                    break;
                }
                content.push(c);
                i += 1;
            }
            if !closed {
                return Err(MapServerError::new(
                    12,
                    "tokenize_line",
                    "unterminated quoted string",
                ));
            }
            let case_insensitive = i < n && chars[i] == 'i';
            if case_insensitive {
                i += 1;
            }
            tokens.push(RawToken::Quoted(content, case_insensitive));
            continue;
        }

        if ch == '(' {
            if let Some(close) = rfind_from(&chars, i + 1, ')') {
                let content: String = chars[i + 1..close].iter().collect();
                tokens.push(RawToken::Expression(content));
                i = close + 1;
                continue;
            }
        }

        if ch == '{' {
            if let Some(close) = rfind_from(&chars, i + 1, '}') {
                let content: String = chars[i + 1..close].iter().collect();
                let values = content.split(',').map(|s| s.trim().to_string()).collect();
                tokens.push(RawToken::List(values));
                i = close + 1;
                continue;
            }
        }

        if ch == '/' && i + 1 < n && chars[i + 1] != '/' {
            if let Some(close) = find_from(&chars, i + 2, '/') {
                let content: String = chars[i + 1..close].iter().collect();
                let mut end = close + 1;
                let case_insensitive = end < n && chars[end] == 'i';
                if case_insensitive {
                    end += 1;
                }
                tokens.push(RawToken::Regex(content, case_insensitive));
                i = end;
                continue;
            }
        }

        let start = i;
        while i < n && !chars[i].is_whitespace() && chars[i] != '#' {
            i += 1;
        }
        tokens.push(RawToken::Bare(chars[start..i].iter().collect()));
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_blocks() {
        let input = r#"
MAP
  NAME "demo map"
  STATUS ON
  EXTENT -180 -90 180 90
  LAYER
    NAME roads
    TYPE LINE
    CLASS
      NAME "primary roads"
      STYLE
        WIDTH 2
        COLOR 255 0 0
      END
    END
  END
END
"#;

        let map = parse_mapfile(input).expect("map should parse");
        assert_eq!(map.kind, "MAP");
        assert_eq!(map.first_str("NAME"), Some("demo map"));
        assert_eq!(map.children_of_kind("LAYER").count(), 1);

        let layer = map
            .children_of_kind("LAYER")
            .next()
            .expect("layer should exist");
        assert_eq!(layer.first_str("NAME"), Some("roads"));
        assert_eq!(layer.children_of_kind("CLASS").count(), 1);

        let class = layer
            .children_of_kind("CLASS")
            .next()
            .expect("class should exist");
        assert_eq!(class.first_str("NAME"), Some("primary roads"));
    }

    #[test]
    fn ignores_comments_and_parses_quoted_hash() {
        let input = r#"
MAP
  NAME "map #1" # trailing comment
END
"#;
        let map = parse_mapfile(input).expect("map should parse");
        assert_eq!(map.first_str("NAME"), Some("map #1"));
    }

    #[test]
    fn rejects_unexpected_end() {
        let input = "END";
        let error = parse_mapfile(input).expect_err("unexpected END should error");
        assert_eq!(error.routine, "parse_mapfile");
    }

    #[test]
    fn rejects_missing_map_root() {
        let input = r#"
LAYER
  NAME roads
END
"#;
        let error = parse_mapfile(input).expect_err("root must be MAP");
        assert_eq!(error.routine, "parse_mapfile");
    }

    #[test]
    fn rejects_tokens_after_root_end() {
        let input = r#"
MAP
END
NAME late
"#;
        let error = parse_mapfile(input).expect_err("tokens after MAP END should fail");
        assert_eq!(error.routine, "parse_mapfile");
    }

    #[test]
    fn parses_expression_regex_list_and_istring_tokens() {
        let input = r#"
MAP
  LAYER
    CLASS
      EXPRESSION ([population] > 50000 AND [type] = "city")
      GROUP {a,b,c}
      TEXT "unmatched"i
    END
  END
END
"#;
        let map = parse_mapfile(input).expect("map should parse");
        let layer = map.children_of_kind("LAYER").next().unwrap();
        let class = layer.children_of_kind("CLASS").next().unwrap();

        assert_eq!(
            class.properties.get("EXPRESSION"),
            Some(&vec![ConfigValue::Expression(
                "[population] > 50000 AND [type] = \"city\"".to_string()
            )])
        );
        assert_eq!(
            class.properties.get("GROUP"),
            Some(&vec![ConfigValue::List(vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string()
            ])])
        );
        assert_eq!(
            class.properties.get("TEXT"),
            Some(&vec![ConfigValue::CaseInsensitiveString(
                "unmatched".to_string()
            )])
        );
    }

    #[test]
    fn parses_regex_tokens_with_and_without_icase() {
        let map = parse_mapfile(
            r#"
MAP
  LAYER
    CLASS
      EXPRESSION /^A/
      EXPRESSION2 /^b/i
    END
  END
END
"#,
        )
        .expect("map should parse");
        let layer = map.children_of_kind("LAYER").next().unwrap();
        let class = layer.children_of_kind("CLASS").next().unwrap();

        assert_eq!(
            class.properties.get("EXPRESSION"),
            Some(&vec![ConfigValue::Regex {
                pattern: "^A".to_string(),
                case_insensitive: false
            }])
        );
        assert_eq!(
            class.properties.get("EXPRESSION2"),
            Some(&vec![ConfigValue::Regex {
                pattern: "^b".to_string(),
                case_insensitive: true
            }])
        );
    }

    #[test]
    fn quoted_numeric_strings_stay_strings() {
        let map = parse_mapfile(
            r#"
MAP
  NAME "5"
END
"#,
        )
        .expect("map should parse");
        assert_eq!(
            map.properties.get("NAME"),
            Some(&vec![ConfigValue::String("5".to_string())])
        );
    }
}
