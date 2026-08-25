//! Configuration object model for parsed mapfiles.
//!
//! This is an incremental Rust equivalent to MapServer's map/layer/class/style
//! object graph loaded by `src/mapfile.c`.

use std::collections::BTreeMap;

/// A primitive mapfile value.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    String(String),
    Number(f64),
    Bool(bool),
}

impl ConfigValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

/// Generic hierarchical block used by the first parser pass.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigBlock {
    pub kind: String,
    pub properties: BTreeMap<String, Vec<ConfigValue>>,
    pub children: Vec<ConfigBlock>,
}

impl ConfigBlock {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            properties: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    pub fn push_property(&mut self, key: &str, values: Vec<ConfigValue>) {
        self.properties
            .entry(key.to_ascii_uppercase())
            .or_default()
            .extend(values);
    }

    pub fn first_str(&self, key: &str) -> Option<&str> {
        self.properties
            .get(&key.to_ascii_uppercase())
            .and_then(|values| values.first())
            .and_then(ConfigValue::as_str)
    }

    pub fn children_of_kind(&self, kind: &str) -> impl Iterator<Item = &ConfigBlock> {
        let expected = kind.to_ascii_uppercase();
        self.children
            .iter()
            .filter(move |child| child.kind == expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_helpers_work() {
        let mut block = ConfigBlock::new("LAYER");
        block.push_property("NAME", vec![ConfigValue::String("roads".to_string())]);
        block.children.push(ConfigBlock::new("CLASS"));

        assert_eq!(block.first_str("name"), Some("roads"));
        assert_eq!(block.children_of_kind("class").count(), 1);
    }
}
