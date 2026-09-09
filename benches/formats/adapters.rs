use super::model::{Dataset, Link, Record};
use parser::ast::{AstStr, HyperItem, NodeDecl, SignedRef, Value};
use std::fmt::Write;

#[derive(Clone, Copy, Debug)]
pub enum Format {
    Hymeko,
    Json,
    Xml,
    Yaml,
}

// Own the parser output until Criterion stops timing; final destruction is excluded.
#[allow(dead_code)]
pub enum Document<'a> {
    Hymeko(AstStr<'a>),
    Json(serde_json::Value),
    Xml(roxmltree::Document<'a>),
    Yaml(serde_json::Value),
}

impl Format {
    pub const ALL: [Self; 4] = [Self::Hymeko, Self::Json, Self::Xml, Self::Yaml];

    pub fn name(self) -> &'static str {
        match self {
            Self::Hymeko => "hymeko",
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Yaml => "yaml",
        }
    }

    pub fn parse_name(s: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|f| f.name() == s)
            .ok_or_else(|| format!("unknown format {s}"))
    }

    fn yaml_options() -> serde_saphyr::Options {
        // Finite common-corpus budget; default-policy rejections are recorded separately.
        serde_saphyr::options! { budget: serde_saphyr::budget! { max_nodes: 5_000_000, max_events: 10_000_000 } }
    }

    pub fn encode(self, data: &Dataset) -> Result<String, String> {
        match self {
            Self::Json => serde_json::to_string(data).map_err(|e| e.to_string()),
            Self::Xml => quick_xml::se::to_string(data).map_err(|e| e.to_string()),
            Self::Yaml => serde_saphyr::to_string(data).map_err(|e| e.to_string()),
            Self::Hymeko => {
                let mut text = String::from("FormatBench{}\n");
                for record in &data.node {
                    record.emit_hymeko(&mut text)?;
                }
                Ok(text)
            }
        }
    }

    pub fn document(self, text: &str) -> Result<Document<'_>, String> {
        match self {
            Self::Hymeko => parser::parse_description(text)
                .map(Document::Hymeko)
                .map_err(|e| format!("{e:?}")),
            Self::Json => serde_json::from_str(text)
                .map(Document::Json)
                .map_err(|e| e.to_string()),
            Self::Xml => roxmltree::Document::parse(text)
                .map(Document::Xml)
                .map_err(|e| e.to_string()),
            Self::Yaml => serde_saphyr::from_str_with_options(text, Self::yaml_options())
                .map(Document::Yaml)
                .map_err(|e| e.to_string()),
        }
    }

    pub fn decode(self, text: &str) -> Result<Dataset, String> {
        match self {
            Self::Json => serde_json::from_str(text).map_err(|e| e.to_string()),
            Self::Xml => quick_xml::de::from_str(text).map_err(|e| e.to_string()),
            Self::Yaml => serde_saphyr::from_str_with_options(text, Self::yaml_options())
                .map_err(|e| e.to_string()),
            Self::Hymeko => {
                let ast = parser::parse_description(text).map_err(|e| format!("{e:?}"))?;
                Dataset::from_hymeko(ast)
            }
        }
    }
}

impl Dataset {
    /// Consume a parsed benchmark document; reject non-record items and invalid fields.
    /// Preserves record order and values without semantic IR resolution. Already-owned
    /// text buffers transfer to the result; borrowed text becomes independently owned.
    pub fn from_hymeko(ast: AstStr<'_>) -> Result<Self, String> {
        let node = ast
            .items
            .into_iter()
            .map(|item| match item {
                HyperItem::Node(n) => Record::from_hymeko(n),
                _ => Err("expected benchmark record node".into()),
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { node })
    }
}

impl Record {
    fn emit_hymeko(&self, text: &mut String) -> Result<(), String> {
        let quoted = serde_json::to_string(&self.text).map_err(|e| e.to_string())?;
        write!(text, "{}{{value {:?};text {};", self.id, self.value, quoted)
            .map_err(|e| e.to_string())?;
        if !self.link.is_empty() {
            text.push_str("@refs{(");
            for (i, link) in self.link.iter().enumerate() {
                if i != 0 {
                    text.push(',');
                }
                let sign = match link.sign {
                    -1 => '-',
                    0 => '~',
                    1 => '+',
                    _ => return Err("invalid sign".into()),
                };
                write!(text, "{sign}{}[{:?}]", link.target, link.weight)
                    .map_err(|e| e.to_string())?;
            }
            text.push_str(");}");
        }
        for child in &self.child {
            child.emit_hymeko(text)?;
        }
        text.push_str("}\n");
        Ok(())
    }

    fn from_hymeko(node: NodeDecl<'_, &str>) -> Result<Self, String> {
        let mut value = None;
        let mut text = None;
        let mut child = Vec::new();
        let mut link = Vec::new();
        for item in node.inner.body.unwrap_or_default() {
            match item {
                HyperItem::Node(n) if n.inner.name == "value" => {
                    let Some(Value::Num(v)) = n.anno.value else {
                        return Err("expected numeric value".into());
                    };
                    if value.replace(v).is_some() {
                        return Err("duplicate value".into());
                    }
                }
                HyperItem::Node(n) if n.inner.name == "text" => {
                    let Some(Value::Str(v)) = n.anno.value else {
                        return Err("expected text".into());
                    };
                    if text.replace(v.into_owned()).is_some() {
                        return Err("duplicate text".into());
                    }
                }
                HyperItem::Node(n) => child.push(Self::from_hymeko(n)?),
                HyperItem::Edge(e) if e.inner.name == "refs" => {
                    for arc in e.inner.body {
                        let HyperItem::Arc(a) = arc else {
                            return Err("expected link arc".into());
                        };
                        for r in a.inner.refs {
                            link.push(Link::from_hymeko(&r)?);
                        }
                    }
                }
                _ => return Err("unexpected benchmark record field".into()),
            }
        }
        Ok(Self {
            id: node.inner.name.into(),
            value: value.ok_or("missing value")?,
            text: text.ok_or("missing text")?,
            child,
            link,
        })
    }
}

impl Link {
    fn from_hymeko(r: &SignedRef<'_, &str>) -> Result<Self, String> {
        let (sign, atom) = match r {
            SignedRef::Minus(a) => (-1, a),
            SignedRef::Neutral(a) => (0, a),
            SignedRef::Plus(a) => (1, a),
        };
        let Some(Value::List(weights)) = atom.anno.value.as_ref() else {
            return Err("missing link weight".into());
        };
        let [Value::Num(weight)] = weights.as_slice() else {
            return Err("expected scalar link weight".into());
        };
        Ok(Self {
            target: atom.target.path.join("."),
            sign,
            weight: *weight,
        })
    }
}
