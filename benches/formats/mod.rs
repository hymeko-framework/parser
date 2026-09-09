pub mod adapters;
pub mod model;

use adapters::Format;
use model::{Dataset, Pattern};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub pattern: String,
    pub nodes: usize,
    pub links: usize,
    pub format: String,
    pub bytes: usize,
    pub blake3: String,
    pub semantic_blake3: String,
    pub file: String,
    pub generate_ms_diagnostic: f64,
    pub serialize_ms_diagnostic: f64,
    pub default_decode_error: Option<String>,
}

impl Fixture {
    pub fn prepare(directory: &Path, pattern: Pattern, nodes: usize) -> Result<Vec<Self>, String> {
        let start = Instant::now();
        let data = Dataset::generate(pattern, nodes);
        let generate_ms = start.elapsed().as_secs_f64() * 1000.0;
        data.validate()?;
        let canonical = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
        let semantic_hash = blake3::hash(&canonical).to_hex().to_string();
        let records = data.records();
        assert_eq!(records.len(), nodes);
        let links = records.iter().map(|n| n.link.len()).sum();
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        Format::ALL
            .into_iter()
            .map(|format| {
                let start = Instant::now();
                let source = format.encode(&data)?;
                let serialize_ms = start.elapsed().as_secs_f64() * 1000.0;
                let default_decode_error = if matches!(format, Format::Yaml) {
                    serde_saphyr::from_str::<Dataset>(&source)
                        .err()
                        .map(|e| e.to_string())
                } else {
                    None
                };
                let decoded = format.decode(&source)?;
                if decoded != data {
                    return Err(format!(
                        "round-trip mismatch {} {}",
                        pattern.name(),
                        format.name()
                    ));
                }
                decoded.validate()?;
                format.document(&source)?;
                let file = format!("{}-{}.{}", pattern.name(), nodes, format.name());
                std::fs::write(directory.join(&file), &source).map_err(|e| e.to_string())?;
                Ok(Self {
                    pattern: pattern.name().into(),
                    nodes,
                    links,
                    format: format.name().into(),
                    bytes: source.len(),
                    blake3: blake3::hash(source.as_bytes()).to_hex().to_string(),
                    semantic_blake3: semantic_hash.clone(),
                    file,
                    generate_ms_diagnostic: generate_ms,
                    serialize_ms_diagnostic: serialize_ms,
                    default_decode_error,
                })
            })
            .collect()
    }
}
