//! Symbology, style, class, and label object model.
//!
//! Incremental Rust port of the data model (not rendering) defined by
//! `src/mapfile.h`'s `styleObj`/`classObj`/`labelObj` structs, plus the
//! expression evaluation logic in `msEvalExpression()`
//! (`src/maputil.c`) used to classify a feature against a layer's list of
//! classes (`msShapeGetClass()`/`msShapeGetNextClass()`).
//!
//! Rendering (symbol drawing, label placement, ...) is explicitly out of
//! scope here; only the object model and classification/matching logic are
//! ported.

use std::collections::BTreeMap;

use regex::Regex;

use crate::color::Color;
use crate::config::{ConfigBlock, ConfigValue};
use crate::datasource::AttributeValue;
use crate::error::{MapServerError, Result};
use crate::primitive::ShapeType;

// ---------------------------------------------------------------------------
// Expressions (`expressionObj`, `msEvalExpression()`)
// ---------------------------------------------------------------------------

/// Rust equivalent of `expressionObj`/`msEvalExpression()`.
///
/// An expression can come from a mapfile in one of five forms (matching
/// `src/maplexer.l`'s `MS_STRING`/`MS_ISTRING`/`MS_REGEX`/`MS_IREGEX`/
/// `MS_LIST`/`MS_EXPRESSION` tokens): a plain string, an optionally
/// case-insensitive comma-separated list, a regular expression, or a full
/// logical expression.
#[derive(Debug, Clone, Default)]
pub enum Expression {
    /// No expression set: `msEvalExpression()` treats this as always true.
    #[default]
    None,
    /// A plain string compared for equality against an item's value
    /// (`MS_STRING`/`MS_ISTRING`).
    String {
        value: String,
        case_insensitive: bool,
    },
    /// A comma-separated list of candidate values (`MS_LIST`).
    List {
        values: Vec<String>,
        case_insensitive: bool,
    },
    /// A regular expression matched against an item's value
    /// (`MS_REGEX`/`MS_IREGEX`).
    Regex {
        source: String,
        regex: Regex,
        case_insensitive: bool,
    },
    /// A full logical expression (`MS_EXPRESSION`), e.g.
    /// `([population] > 50000 AND [type] = "city")`.
    Logical(LogicalExpr),
}

impl Expression {
    /// Build an [`Expression`] from a single mapfile [`ConfigValue`],
    /// mirroring `loadExpression()`/`loadExpressionString()` in
    /// `src/mapfile.c`.
    pub fn from_config_value(value: &ConfigValue) -> Result<Self> {
        match value {
            ConfigValue::String(text) => Ok(Expression::String {
                value: text.clone(),
                case_insensitive: false,
            }),
            ConfigValue::CaseInsensitiveString(text) => Ok(Expression::String {
                value: text.clone(),
                case_insensitive: true,
            }),
            ConfigValue::List(values) => Ok(Expression::List {
                values: values.clone(),
                case_insensitive: false,
            }),
            ConfigValue::Regex {
                pattern,
                case_insensitive,
            } => {
                let compiled = compile_regex(pattern, *case_insensitive)?;
                Ok(Expression::Regex {
                    source: pattern.clone(),
                    regex: compiled,
                    case_insensitive: *case_insensitive,
                })
            }
            ConfigValue::Expression(source) => Ok(Expression::Logical(LogicalExpr::parse(source)?)),
            ConfigValue::Number(n) => Ok(Expression::String {
                value: format_config_number(*n),
                case_insensitive: false,
            }),
            ConfigValue::Bool(b) => Ok(Expression::String {
                value: if *b { "1".to_string() } else { "0".to_string() },
                case_insensitive: false,
            }),
        }
    }

    /// Evaluate the expression against a feature's attribute map, mirroring
    /// `msEvalExpression()`. `item` names the attribute item that
    /// `MS_STRING`/`MS_LIST`/`MS_REGEX` expressions compare against (the
    /// equivalent of `layer->classitem`); it is ignored for
    /// [`Expression::Logical`] and [`Expression::None`].
    pub fn evaluate(
        &self,
        item: Option<&str>,
        attributes: &BTreeMap<String, AttributeValue>,
    ) -> bool {
        match self {
            Expression::None => true,
            Expression::String {
                value,
                case_insensitive,
            } => {
                let Some(actual) = item.and_then(|name| attribute_as_string(attributes, name))
                else {
                    return false;
                };
                if *case_insensitive {
                    actual.eq_ignore_ascii_case(value)
                } else {
                    actual == *value
                }
            }
            Expression::List {
                values,
                case_insensitive,
            } => {
                let Some(actual) = item.and_then(|name| attribute_as_string(attributes, name))
                else {
                    return false;
                };
                values.iter().any(|candidate| {
                    if *case_insensitive {
                        actual.eq_ignore_ascii_case(candidate)
                    } else {
                        actual == *candidate
                    }
                })
            }
            Expression::Regex { regex, .. } => {
                let Some(actual) = item.and_then(|name| attribute_as_string(attributes, name))
                else {
                    return false;
                };
                if actual.is_empty() {
                    return false;
                }
                regex.is_match(&actual)
            }
            Expression::Logical(expr) => expr.evaluate(attributes),
        }
    }
}

impl PartialEq for Expression {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Expression::None, Expression::None) => true,
            (
                Expression::String {
                    value: v1,
                    case_insensitive: c1,
                },
                Expression::String {
                    value: v2,
                    case_insensitive: c2,
                },
            ) => v1 == v2 && c1 == c2,
            (
                Expression::List {
                    values: v1,
                    case_insensitive: c1,
                },
                Expression::List {
                    values: v2,
                    case_insensitive: c2,
                },
            ) => v1 == v2 && c1 == c2,
            (
                Expression::Regex {
                    source: s1,
                    case_insensitive: c1,
                    ..
                },
                Expression::Regex {
                    source: s2,
                    case_insensitive: c2,
                    ..
                },
            ) => s1 == s2 && c1 == c2,
            (Expression::Logical(a), Expression::Logical(b)) => a == b,
            _ => false,
        }
    }
}

fn compile_regex(pattern: &str, case_insensitive: bool) -> Result<Regex> {
    let effective = if case_insensitive {
        format!("(?i){pattern}")
    } else {
        pattern.to_string()
    };
    Regex::new(&effective).map_err(|e| {
        MapServerError::new(
            20,
            "Expression::from_config_value",
            format!("failed to compile regular expression /{pattern}/: {e}"),
        )
    })
}

fn format_config_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

fn attribute_as_string(
    attributes: &BTreeMap<String, AttributeValue>,
    name: &str,
) -> Option<String> {
    attributes.get(name).map(|value| match value {
        AttributeValue::String(s) => s.clone(),
        AttributeValue::Number(n) => format_config_number(*n),
        AttributeValue::Bool(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Logical expressions (`MS_EXPRESSION`)
// ---------------------------------------------------------------------------

/// A parsed logical expression AST, supporting the common subset of
/// MapServer's expression language used by class/style/label expressions:
/// attribute bindings (`[item]`), string/number literals, regular
/// expression literals, comparisons (`=`, `==`, `!=`, `<>`, `<`, `<=`, `>`,
/// `>=`, `=~`, `!~`), and logical combinators (`AND`, `OR`, `NOT`) with
/// parentheses for grouping.
///
/// This is not a full reimplementation of `src/mapparser.y`; it covers the
/// class-matching expressions found in typical mapfiles.
#[derive(Debug, Clone)]
pub enum LogicalExpr {
    Binding(String),
    StringLiteral(String),
    NumberLiteral(f64),
    RegexLiteral(String, Regex),
    Not(Box<LogicalExpr>),
    And(Box<LogicalExpr>, Box<LogicalExpr>),
    Or(Box<LogicalExpr>, Box<LogicalExpr>),
    Compare(CompareOp, Box<LogicalExpr>, Box<LogicalExpr>),
}

impl PartialEq for LogicalExpr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (LogicalExpr::Binding(a), LogicalExpr::Binding(b)) => a == b,
            (LogicalExpr::StringLiteral(a), LogicalExpr::StringLiteral(b)) => a == b,
            (LogicalExpr::NumberLiteral(a), LogicalExpr::NumberLiteral(b)) => a == b,
            (LogicalExpr::RegexLiteral(a, _), LogicalExpr::RegexLiteral(b, _)) => a == b,
            (LogicalExpr::Not(a), LogicalExpr::Not(b)) => a == b,
            (LogicalExpr::And(a1, b1), LogicalExpr::And(a2, b2)) => a1 == a2 && b1 == b2,
            (LogicalExpr::Or(a1, b1), LogicalExpr::Or(a2, b2)) => a1 == a2 && b1 == b2,
            (LogicalExpr::Compare(op1, a1, b1), LogicalExpr::Compare(op2, a2, b2)) => {
                op1 == op2 && a1 == a2 && b1 == b2
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    RegexMatch,
    RegexNotMatch,
}

impl LogicalExpr {
    pub fn parse(source: &str) -> Result<Self> {
        let tokens = lex_logical_expr(source)?;
        let mut parser = ExprParser {
            tokens: &tokens,
            pos: 0,
        };
        let expr = parser.parse_or()?;
        if parser.pos != parser.tokens.len() {
            return Err(MapServerError::new(
                20,
                "LogicalExpr::parse",
                format!("unexpected trailing tokens in expression: {source}"),
            ));
        }
        Ok(expr)
    }

    /// Evaluate the expression to a boolean against the given attributes.
    pub fn evaluate(&self, attributes: &BTreeMap<String, AttributeValue>) -> bool {
        eval_bool(self, attributes)
    }
}

/// A resolved value used while evaluating a [`LogicalExpr`].
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Bool(bool),
    Number(f64),
    String(String),
}

fn eval_bool(expr: &LogicalExpr, attributes: &BTreeMap<String, AttributeValue>) -> bool {
    match eval_value(expr, attributes) {
        Value::Bool(b) => b,
        Value::Number(n) => n != 0.0,
        Value::String(s) => !s.is_empty(),
    }
}

fn eval_value(expr: &LogicalExpr, attributes: &BTreeMap<String, AttributeValue>) -> Value {
    match expr {
        LogicalExpr::Binding(name) => match attributes.get(name) {
            Some(AttributeValue::Number(n)) => Value::Number(*n),
            Some(AttributeValue::Bool(b)) => Value::Bool(*b),
            Some(AttributeValue::String(s)) => Value::String(s.clone()),
            None => Value::String(String::new()),
        },
        LogicalExpr::StringLiteral(s) => Value::String(s.clone()),
        LogicalExpr::NumberLiteral(n) => Value::Number(*n),
        LogicalExpr::RegexLiteral(source, _) => Value::String(source.clone()),
        LogicalExpr::Not(inner) => Value::Bool(!eval_bool(inner, attributes)),
        LogicalExpr::And(a, b) => Value::Bool(eval_bool(a, attributes) && eval_bool(b, attributes)),
        LogicalExpr::Or(a, b) => Value::Bool(eval_bool(a, attributes) || eval_bool(b, attributes)),
        LogicalExpr::Compare(op, a, b) => Value::Bool(eval_compare(*op, a, b, attributes)),
    }
}

fn eval_compare(
    op: CompareOp,
    a: &LogicalExpr,
    b: &LogicalExpr,
    attributes: &BTreeMap<String, AttributeValue>,
) -> bool {
    if matches!(op, CompareOp::RegexMatch | CompareOp::RegexNotMatch) {
        let subject = value_as_string(&eval_value(a, attributes));
        let matched = match b {
            LogicalExpr::RegexLiteral(_, regex) => regex.is_match(&subject),
            _ => false,
        };
        return if op == CompareOp::RegexMatch {
            matched
        } else {
            !matched
        };
    }

    let left = eval_value(a, attributes);
    let right = eval_value(b, attributes);

    // Numeric comparison when both sides are numbers; otherwise fall back
    // to (case-sensitive) string comparison, mirroring the C parser's
    // dynamic typing of tokens.
    if let (Value::Number(l), Value::Number(r)) = (&left, &right) {
        return compare_ordering(op, l.partial_cmp(r));
    }

    let l = value_as_string(&left);
    let r = value_as_string(&right);
    compare_ordering(op, l.partial_cmp(&r))
}

fn compare_ordering(op: CompareOp, ordering: Option<std::cmp::Ordering>) -> bool {
    use std::cmp::Ordering::*;
    match (op, ordering) {
        (CompareOp::Eq, Some(Equal)) => true,
        (CompareOp::Ne, Some(o)) if o != Equal => true,
        (CompareOp::Lt, Some(Less)) => true,
        (CompareOp::Le, Some(Less | Equal)) => true,
        (CompareOp::Gt, Some(Greater)) => true,
        (CompareOp::Ge, Some(Greater | Equal)) => true,
        _ => false,
    }
}

fn value_as_string(value: &Value) -> String {
    match value {
        Value::Bool(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        Value::Number(n) => format_config_number(*n),
        Value::String(s) => s.clone(),
    }
}

// --- Logical expression tokenizer/parser -----------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Binding(String),
    String(String),
    Number(f64),
    Regex(String, bool),
    And,
    Or,
    Not,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    RegexMatch,
    RegexNotMatch,
    LParen,
    RParen,
}

fn lex_logical_expr(source: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = source.chars().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < n {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '[' => {
                let start = i + 1;
                let end = source_find(&chars, start, ']').ok_or_else(|| {
                    MapServerError::new(20, "LogicalExpr::parse", "unterminated binding '['")
                })?;
                tokens.push(Token::Binding(chars[start..end].iter().collect()));
                i = end + 1;
            }
            '"' | '\'' => {
                let quote = c;
                let mut content = String::new();
                i += 1;
                let mut closed = false;
                while i < n {
                    if chars[i] == quote {
                        closed = true;
                        i += 1;
                        break;
                    }
                    content.push(chars[i]);
                    i += 1;
                }
                if !closed {
                    return Err(MapServerError::new(
                        20,
                        "LogicalExpr::parse",
                        "unterminated string literal",
                    ));
                }
                tokens.push(Token::String(content));
            }
            '/' => {
                let start = i + 1;
                let end = source_find(&chars, start, '/').ok_or_else(|| {
                    MapServerError::new(20, "LogicalExpr::parse", "unterminated regex literal")
                })?;
                let pattern: String = chars[start..end].iter().collect();
                let mut pos = end + 1;
                let case_insensitive = pos < n && chars[pos] == 'i';
                if case_insensitive {
                    pos += 1;
                }
                tokens.push(Token::Regex(pattern, case_insensitive));
                i = pos;
            }
            '=' => {
                if i + 1 < n && chars[i + 1] == '~' {
                    tokens.push(Token::RegexMatch);
                    i += 2;
                } else if i + 1 < n && chars[i + 1] == '=' {
                    tokens.push(Token::Eq);
                    i += 2;
                } else {
                    tokens.push(Token::Eq);
                    i += 1;
                }
            }
            '!' => {
                if i + 1 < n && chars[i + 1] == '~' {
                    tokens.push(Token::RegexNotMatch);
                    i += 2;
                } else if i + 1 < n && chars[i + 1] == '=' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else {
                    return Err(MapServerError::new(
                        20,
                        "LogicalExpr::parse",
                        "unexpected '!' (expected '!=' or '!~')",
                    ));
                }
            }
            '<' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    tokens.push(Token::Le);
                    i += 2;
                } else if i + 1 < n && chars[i + 1] == '>' {
                    tokens.push(Token::Ne);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < n && chars[i + 1] == '=' {
                    tokens.push(Token::Ge);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            _ if c.is_ascii_digit() || (c == '-' && i + 1 < n && chars[i + 1].is_ascii_digit()) => {
                let start = i;
                i += 1;
                while i < n && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let number = text.parse::<f64>().map_err(|_| {
                    MapServerError::new(
                        20,
                        "LogicalExpr::parse",
                        format!("invalid numeric literal: {text}"),
                    )
                })?;
                tokens.push(Token::Number(number));
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                match word.to_ascii_uppercase().as_str() {
                    "AND" => tokens.push(Token::And),
                    "OR" => tokens.push(Token::Or),
                    "NOT" => tokens.push(Token::Not),
                    _ => {
                        return Err(MapServerError::new(
                            20,
                            "LogicalExpr::parse",
                            format!("unexpected identifier '{word}' in expression"),
                        ))
                    }
                }
            }
            other => {
                return Err(MapServerError::new(
                    20,
                    "LogicalExpr::parse",
                    format!("unexpected character '{other}' in expression"),
                ))
            }
        }
    }

    Ok(tokens)
}

fn source_find(chars: &[char], from: usize, needle: char) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, c)| **c == needle)
        .map(|(i, _)| i)
}

struct ExprParser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> ExprParser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos);
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn parse_or(&mut self) -> Result<LogicalExpr> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.advance();
            let right = self.parse_and()?;
            left = LogicalExpr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<LogicalExpr> {
        let mut left = self.parse_not()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.advance();
            let right = self.parse_not()?;
            left = LogicalExpr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<LogicalExpr> {
        if matches!(self.peek(), Some(Token::Not)) {
            self.advance();
            let inner = self.parse_not()?;
            return Ok(LogicalExpr::Not(Box::new(inner)));
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<LogicalExpr> {
        let left = self.parse_primary()?;
        let op = match self.peek() {
            Some(Token::Eq) => CompareOp::Eq,
            Some(Token::Ne) => CompareOp::Ne,
            Some(Token::Lt) => CompareOp::Lt,
            Some(Token::Le) => CompareOp::Le,
            Some(Token::Gt) => CompareOp::Gt,
            Some(Token::Ge) => CompareOp::Ge,
            Some(Token::RegexMatch) => CompareOp::RegexMatch,
            Some(Token::RegexNotMatch) => CompareOp::RegexNotMatch,
            _ => return Ok(left),
        };
        self.advance();
        let right = self.parse_primary()?;
        Ok(LogicalExpr::Compare(op, Box::new(left), Box::new(right)))
    }

    fn parse_primary(&mut self) -> Result<LogicalExpr> {
        match self.advance().cloned() {
            Some(Token::LParen) => {
                let inner = self.parse_or()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(inner),
                    _ => Err(MapServerError::new(
                        20,
                        "LogicalExpr::parse",
                        "expected closing ')'",
                    )),
                }
            }
            Some(Token::Binding(name)) => Ok(LogicalExpr::Binding(name)),
            Some(Token::String(value)) => Ok(LogicalExpr::StringLiteral(value)),
            Some(Token::Number(value)) => Ok(LogicalExpr::NumberLiteral(value)),
            Some(Token::Regex(pattern, case_insensitive)) => {
                let regex = compile_regex(&pattern, case_insensitive)?;
                Ok(LogicalExpr::RegexLiteral(pattern, regex))
            }
            Some(Token::Not) => {
                let inner = self.parse_not()?;
                Ok(LogicalExpr::Not(Box::new(inner)))
            }
            other => Err(MapServerError::new(
                20,
                "LogicalExpr::parse",
                format!("unexpected token in expression: {other:?}"),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Style / class / label object model
// ---------------------------------------------------------------------------

/// Whether a class/style is active, mirrors `MS_ON`/`MS_OFF`/`MS_DELETE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    On,
    Off,
    Delete,
}

/// Rust equivalent of the subset of `styleObj` (`src/mapserver.h`) covering
/// symbology (not rendering).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyleObj {
    pub symbol_name: Option<String>,
    pub color: Option<Color>,
    pub outline_color: Option<Color>,
    pub opacity: i32,
    pub size: f64,
    pub min_size: f64,
    pub max_size: f64,
    pub width: f64,
    pub outline_width: f64,
    pub angle: f64,
}

/// Rust equivalent of the subset of `labelObj` (`src/mapserver.h`) covering
/// the text expression and common presentation fields.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LabelObj {
    pub text: Option<Expression>,
    pub font: Option<String>,
    pub size: f64,
    pub color: Option<Color>,
    pub outline_color: Option<Color>,
    pub angle: f64,
}

/// Rust equivalent of the classification-relevant subset of `classObj`
/// (`src/mapserver.h`), plus the `EXPRESSION`-based matching logic from
/// `msShapeGetNextClass()`/`msEvalExpression()` (`src/maputil.c`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClassObj {
    pub name: Option<String>,
    pub group: Option<String>,
    pub expression: Expression,
    pub status: Status,
    pub min_scale_denom: Option<f64>,
    pub max_scale_denom: Option<f64>,
    /// Mirrors `class->isfallback`, set for classes representing an SLD
    /// `<ElseFilter/>`: only applicable if no earlier class matched.
    pub is_fallback: bool,
    pub styles: Vec<StyleObj>,
    pub labels: Vec<LabelObj>,
}

/// Parse a `CLASS` [`ConfigBlock`] (as produced by
/// [`crate::mapfile::parse_mapfile`]) into a [`ClassObj`].
///
/// `classitem` is the layer's `CLASSITEM` attribute name (the equivalent of
/// `layer->classitem`), used to resolve `MS_STRING`/`MS_LIST`/`MS_REGEX`
/// class expressions against feature attributes.
pub fn parse_class(block: &ConfigBlock) -> Result<ClassObj> {
    let mut class = ClassObj {
        name: block.first_str("NAME").map(str::to_string),
        group: block.first_str("GROUP").map(str::to_string),
        ..Default::default()
    };

    if let Some(values) = block.properties.get("EXPRESSION") {
        if let Some(value) = values.first() {
            class.expression = Expression::from_config_value(value)?;
        }
    }

    class.status = match block.first_str("STATUS") {
        Some(s) if s.eq_ignore_ascii_case("OFF") => Status::Off,
        Some(s) if s.eq_ignore_ascii_case("DELETE") => Status::Delete,
        _ => Status::On,
    };

    class.min_scale_denom = block
        .properties
        .get("MINSCALEDENOM")
        .and_then(|v| v.first())
        .and_then(ConfigValue::as_f64);
    class.max_scale_denom = block
        .properties
        .get("MAXSCALEDENOM")
        .and_then(|v| v.first())
        .and_then(ConfigValue::as_f64);

    for style_block in block.children_of_kind("STYLE") {
        class.styles.push(parse_style(style_block));
    }
    for label_block in block.children_of_kind("LABEL") {
        class.labels.push(parse_label(label_block)?);
    }

    Ok(class)
}

fn parse_style(block: &ConfigBlock) -> StyleObj {
    StyleObj {
        symbol_name: block.first_str("SYMBOL").map(str::to_string),
        color: block
            .properties
            .get("COLOR")
            .and_then(|values| color_from_rgb_values(values)),
        outline_color: block
            .properties
            .get("OUTLINECOLOR")
            .and_then(|values| color_from_rgb_values(values)),
        opacity: block
            .properties
            .get("OPACITY")
            .and_then(|v| v.first())
            .and_then(ConfigValue::as_f64)
            .map(|n| n as i32)
            .unwrap_or(100),
        size: number_or(block, "SIZE", 1.0),
        min_size: number_or(block, "MINSIZE", 0.0),
        max_size: number_or(block, "MAXSIZE", 0.0),
        width: number_or(block, "WIDTH", 1.0),
        outline_width: number_or(block, "OUTLINEWIDTH", 0.0),
        angle: number_or(block, "ANGLE", 0.0),
    }
}

fn parse_label(block: &ConfigBlock) -> Result<LabelObj> {
    let text = match block.properties.get("TEXT").and_then(|v| v.first()) {
        Some(value) => Some(Expression::from_config_value(value)?),
        None => None,
    };
    Ok(LabelObj {
        text,
        font: block.first_str("FONT").map(str::to_string),
        size: number_or(block, "SIZE", 0.0),
        color: block
            .properties
            .get("COLOR")
            .and_then(|values| color_from_rgb_values(values)),
        outline_color: block
            .properties
            .get("OUTLINECOLOR")
            .and_then(|values| color_from_rgb_values(values)),
        angle: number_or(block, "ANGLE", 0.0),
    })
}

fn number_or(block: &ConfigBlock, key: &str, default: f64) -> f64 {
    block
        .properties
        .get(key)
        .and_then(|v| v.first())
        .and_then(ConfigValue::as_f64)
        .unwrap_or(default)
}

fn color_from_rgb_values(values: &[ConfigValue]) -> Option<Color> {
    if values.len() >= 3 {
        let r = values[0].as_f64()? as i32;
        let g = values[1].as_f64()? as i32;
        let b = values[2].as_f64()? as i32;
        return Some(Color {
            red: r,
            green: g,
            blue: b,
        });
    }
    None
}

// ---------------------------------------------------------------------------
// Classification (`msShapeGetClass()` / `msShapeGetNextClass()`)
// ---------------------------------------------------------------------------

/// Returns `true` when `scaledenom` (if any) is within `[min, max]`
/// (treating `None`/non-positive bounds as unset), mirroring
/// `msScaleInBounds()`.
fn scale_in_bounds(scaledenom: Option<f64>, min: Option<f64>, max: Option<f64>) -> bool {
    let Some(scale) = scaledenom else {
        return true;
    };
    if let Some(min) = min {
        if min > 0.0 && scale < min {
            return false;
        }
    }
    if let Some(max) = max {
        if max > 0.0 && scale > max {
            return false;
        }
    }
    true
}

/// Find the index of the next class (starting after `current`) whose scale
/// bounds and expression match the given shape/attributes.
///
/// This is a Rust port of `msShapeGetNextClass()` (`src/maputil.c`), scoped
/// to the classification data model (the `minfeaturesize` check, which
/// depends on the map's pixel-to-georeferenced-unit conversion, is left to
/// the future rendering-pipeline port).
pub fn get_next_class(
    classes: &[ClassObj],
    current: Option<usize>,
    classitem: Option<&str>,
    scaledenom: Option<f64>,
    _shape_type: ShapeType,
    attributes: &BTreeMap<String, AttributeValue>,
) -> Option<usize> {
    let start = current.map(|c| c + 1).unwrap_or(0);

    for (iclass, class) in classes.iter().enumerate().skip(start) {
        if !scale_in_bounds(scaledenom, class.min_scale_denom, class.max_scale_denom) {
            continue;
        }

        if class.status != Status::Delete && class.expression.evaluate(classitem, attributes) {
            if class.is_fallback && current.is_some() {
                return None;
            }
            return Some(iclass);
        }
    }

    None
}

/// Convenience wrapper matching `msShapeGetClass()` (classification from
/// the beginning of the class list).
pub fn get_class(
    classes: &[ClassObj],
    classitem: Option<&str>,
    scaledenom: Option<f64>,
    shape_type: ShapeType,
    attributes: &BTreeMap<String, AttributeValue>,
) -> Option<usize> {
    get_next_class(classes, None, classitem, scaledenom, shape_type, attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapfile::parse_mapfile;

    fn attrs(pairs: &[(&str, AttributeValue)]) -> BTreeMap<String, AttributeValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn parse_first_layer_classes(mapfile: &str) -> Vec<ClassObj> {
        let map = parse_mapfile(mapfile).expect("map should parse");
        let layer = map.children_of_kind("LAYER").next().expect("layer");
        layer
            .children_of_kind("CLASS")
            .map(|c| parse_class(c).expect("class should parse"))
            .collect()
    }

    #[test]
    fn string_expression_matches_exact_item_value() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASSITEM "kind"
    CLASS
      NAME "roads"
      EXPRESSION "road"
    END
    CLASS
      NAME "water"
      EXPRESSION "water"
    END
  END
END
"#,
        );

        let attributes = attrs(&[("kind", AttributeValue::String("water".to_string()))]);
        let matched = get_class(
            &classes,
            Some("kind"),
            None,
            ShapeType::Polygon,
            &attributes,
        );
        assert_eq!(matched, Some(1));
        assert_eq!(classes[1].name.as_deref(), Some("water"));
    }

    #[test]
    fn no_class_matches_returns_none() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      EXPRESSION "road"
    END
  END
END
"#,
        );
        let attributes = attrs(&[("kind", AttributeValue::String("water".to_string()))]);
        assert_eq!(
            get_class(
                &classes,
                Some("kind"),
                None,
                ShapeType::Polygon,
                &attributes
            ),
            None
        );
    }

    #[test]
    fn list_expression_matches_any_candidate() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      EXPRESSION {road,path,track}
    END
  END
END
"#,
        );
        let attributes = attrs(&[("kind", AttributeValue::String("path".to_string()))]);
        assert_eq!(
            get_class(&classes, Some("kind"), None, ShapeType::Line, &attributes),
            Some(0)
        );
    }

    #[test]
    fn regex_expression_matches_pattern() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      EXPRESSION /^A/i
    END
  END
END
"#,
        );
        let matching = attrs(&[("name", AttributeValue::String("alpha".to_string()))]);
        let non_matching = attrs(&[("name", AttributeValue::String("beta".to_string()))]);

        assert_eq!(
            get_class(&classes, Some("name"), None, ShapeType::Point, &matching),
            Some(0)
        );
        assert_eq!(
            get_class(
                &classes,
                Some("name"),
                None,
                ShapeType::Point,
                &non_matching
            ),
            None
        );
    }

    #[test]
    fn logical_expression_evaluates_comparisons_and_boolean_operators() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      EXPRESSION ([population] > 50000 AND [type] = "city")
    END
    CLASS
      EXPRESSION ([population] <= 50000 OR [type] != "city")
    END
  END
END
"#,
        );

        let big_city = attrs(&[
            ("population", AttributeValue::Number(1_000_000.0)),
            ("type", AttributeValue::String("city".to_string())),
        ]);
        assert_eq!(
            get_class(&classes, None, None, ShapeType::Point, &big_city),
            Some(0)
        );

        let village = attrs(&[
            ("population", AttributeValue::Number(200.0)),
            ("type", AttributeValue::String("village".to_string())),
        ]);
        assert_eq!(
            get_class(&classes, None, None, ShapeType::Point, &village),
            Some(1)
        );
    }

    #[test]
    fn logical_expression_supports_not_and_regex_match() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      EXPRESSION (NOT [name] =~ /^Z/)
    END
  END
END
"#,
        );
        let attributes = attrs(&[("name", AttributeValue::String("Alpha".to_string()))]);
        assert_eq!(
            get_class(&classes, None, None, ShapeType::Point, &attributes),
            Some(0)
        );

        let attributes = attrs(&[("name", AttributeValue::String("Zeta".to_string()))]);
        assert_eq!(
            get_class(&classes, None, None, ShapeType::Point, &attributes),
            None
        );
    }

    #[test]
    fn scale_bounds_exclude_out_of_range_classes() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      MINSCALEDENOM 1000
      MAXSCALEDENOM 5000
      EXPRESSION "road"
    END
  END
END
"#,
        );
        let attributes = attrs(&[("kind", AttributeValue::String("road".to_string()))]);

        assert_eq!(
            get_class(
                &classes,
                Some("kind"),
                Some(2000.0),
                ShapeType::Line,
                &attributes
            ),
            Some(0)
        );
        assert_eq!(
            get_class(
                &classes,
                Some("kind"),
                Some(10000.0),
                ShapeType::Line,
                &attributes
            ),
            None
        );
    }

    #[test]
    fn deleted_class_is_skipped() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      STATUS DELETE
      EXPRESSION "road"
    END
    CLASS
      EXPRESSION "road"
    END
  END
END
"#,
        );
        let attributes = attrs(&[("kind", AttributeValue::String("road".to_string()))]);
        assert_eq!(
            get_class(&classes, Some("kind"), None, ShapeType::Line, &attributes),
            Some(1)
        );
    }

    #[test]
    fn fallback_class_only_applies_when_no_earlier_class_matched() {
        let mut classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      EXPRESSION "road"
    END
    CLASS
      EXPRESSION "water"
    END
  END
END
"#,
        );
        classes[1].is_fallback = true;

        let water = attrs(&[("kind", AttributeValue::String("water".to_string()))]);
        // Starting fresh (current = None): fallback class is reachable.
        assert_eq!(
            get_next_class(&classes, None, Some("kind"), None, ShapeType::Point, &water),
            Some(1)
        );
        // Once a previous class has already been tried (current = Some(0)),
        // a fallback class must not match.
        assert_eq!(
            get_next_class(
                &classes,
                Some(0),
                Some("kind"),
                None,
                ShapeType::Point,
                &water
            ),
            None
        );
    }

    #[test]
    fn parses_style_and_label_fields() {
        let classes = parse_first_layer_classes(
            r#"
MAP
  LAYER
    CLASS
      NAME "roads"
      STYLE
        COLOR 255 0 0
        WIDTH 2
        SIZE 4
      END
      LABEL
        TEXT "[name]"
        COLOR 0 0 0
        SIZE 12
      END
    END
  END
END
"#,
        );

        assert_eq!(classes.len(), 1);
        let class = &classes[0];
        assert_eq!(class.styles.len(), 1);
        let style = &class.styles[0];
        assert_eq!(
            style.color,
            Some(Color {
                red: 255,
                green: 0,
                blue: 0
            })
        );
        assert_eq!(style.width, 2.0);
        assert_eq!(style.size, 4.0);

        assert_eq!(class.labels.len(), 1);
        assert_eq!(class.labels[0].size, 12.0);
    }
}
