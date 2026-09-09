//! Benchmark-only common records. No HyMeKo compiler or graph algorithm lives here.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "dataset")]
pub struct Dataset {
    #[serde(default)]
    pub node: Vec<Record>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub value: f64,
    pub text: String,
    #[serde(default)]
    pub link: Vec<Link>,
    #[serde(default)]
    pub child: Vec<Record>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub target: String,
    pub sign: i8,
    pub weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Flat,
    Linked,
    Unicode,
    LongText,
    Nested,
}

impl Pattern {
    pub const ALL: [Self; 5] = [
        Self::Flat,
        Self::Linked,
        Self::Unicode,
        Self::LongText,
        Self::Nested,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Linked => "linked",
            Self::Unicode => "unicode",
            Self::LongText => "long_text",
            Self::Nested => "nested",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|p| p.name() == s)
            .ok_or_else(|| format!("unknown pattern {s}"))
    }
}

impl Dataset {
    pub fn generate(pattern: Pattern, count: usize) -> Self {
        let mut node = Vec::new();
        for start in (0..count).step_by(if pattern == Pattern::Nested { 16 } else { 1 }) {
            let end = if pattern == Pattern::Nested {
                (start + 16).min(count)
            } else {
                start + 1
            };
            let mut child = Vec::new();
            for i in (start..end).rev() {
                let text = match pattern {
                    Pattern::Unicode => format!("árvíztűrő tükörfúrógép 日本語 λ {i}\n\"quoted\""),
                    Pattern::LongText => "abcdefghij0123456789".repeat(50),
                    _ => format!("record {i}"),
                };
                let link = if pattern == Pattern::Linked {
                    (0..8)
                        .map(|k| Link {
                            target: format!("n{}", (i + k * 17) % count),
                            sign: (k % 3) as i8 - 1,
                            weight: (i % 64 + k) as f64 / 8.0,
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                child = vec![Record {
                    id: format!("n{i}"),
                    value: (i % 1024) as f64 / 8.0,
                    text,
                    link,
                    child,
                }];
            }
            node.extend(child);
        }
        Self { node }
    }

    pub fn records(&self) -> Vec<&Record> {
        let mut pending: Vec<_> = self.node.iter().collect();
        let mut records = Vec::new();
        while let Some(record) = pending.pop() {
            pending.extend(record.child.iter());
            records.push(record);
        }
        records
    }

    pub fn validate(&self) -> Result<(), String> {
        let records = self.records();
        let names: HashSet<_> = records.iter().map(|n| n.id.as_str()).collect();
        if names.len() != records.len() {
            return Err("duplicate node identifier".into());
        }
        for node in records {
            if !node.value.is_finite() {
                return Err("non-finite value outside common corpus".into());
            }
            for link in &node.link {
                if !names.contains(link.target.as_str())
                    || !(-1..=1).contains(&link.sign)
                    || !link.weight.is_finite()
                {
                    return Err("invalid benchmark link".into());
                }
            }
        }
        Ok(())
    }
}
