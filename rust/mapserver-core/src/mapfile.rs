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

        let keyword_upper = tokens[0].to_ascii_uppercase();

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

        let values = tokens[1..].iter().map(|value| parse_value(value)).collect();
        current.push_property(&tokens[0], values);
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

fn tokenize_line(input: &str) -> Result<Vec<String>> {
    let mut cleaned = String::with_capacity(input.len());
    let mut in_quotes = false;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            cleaned.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' && in_quotes {
            cleaned.push(ch);
            escape = true;
            continue;
        }

        if ch == '"' {
            in_quotes = !in_quotes;
            cleaned.push(ch);
            continue;
        }

        if ch == '#' && !in_quotes {
            break;
        }

        cleaned.push(ch);
    }

    if in_quotes {
        return Err(MapServerError::new(
            12,
            "tokenize_line",
            "unterminated quoted string",
        ));
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escape = false;

    for ch in cleaned.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' && in_quotes {
            escape = true;
            continue;
        }

        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }

        if ch.is_ascii_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }

        current.push(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
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
}
