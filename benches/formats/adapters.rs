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
            Self::Hymeko => StreamingRecordBuilder::decode(text),
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
    fn append_owned(destination: &mut Vec<Self>, source: Vec<Self>) {
        if source.is_empty() {
            return;
        }
        // Reuse compact buffers without retaining more spare capacity than
        // the former extension into an empty Vec (minimum reserve: four Links).
        if destination.is_empty() && source.capacity() <= source.len().max(4) {
            *destination = source;
        } else {
            destination.extend(source);
        }
    }

    fn from_hymeko(r: &SignedRef<'_, &str>) -> Result<Self, String> {
        let (sign, atom) = match r {
            SignedRef::Minus(a) => (-1, a),
            SignedRef::Neutral(a) => (0, a),
            SignedRef::Plus(a) => (1, a),
        };
        Self::from_parts(sign, &atom.target, &atom.anno)
    }

    fn from_parts(
        sign: i8,
        target: &parser::ast::Ref<&str>,
        anno: &parser::ast::Anno<'_, &str>,
    ) -> Result<Self, String> {
        Ok(Self {
            target: target.path.join("."),
            sign,
            weight: Self::weight(anno)?,
        })
    }

    fn weight(anno: &parser::ast::Anno<'_, &str>) -> Result<f64, String> {
        let Some(Value::List(weights)) = anno.value.as_ref() else {
            return Err("missing link weight".into());
        };
        let [Value::Num(weight)] = weights.as_slice() else {
            return Err("expected scalar link weight".into());
        };
        Ok(*weight)
    }
}

/// Benchmark consumer of the production generic parser; no separate grammar.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "Compatibility oracle used by integration tests, also compiled by Clippy for the benchmark"
)]
pub struct RecordBuilder;

pub enum BuiltItem<'a> {
    Leaf(&'a str, Option<Value<'a, &'a str>>),
    Node(&'a str, Result<Record, String>),
    Edge(&'a str, Result<Vec<Link>, String>),
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "Compatibility oracle payload; streaming construction emits arcs directly"
    )]
    Arc(Result<Vec<Link>, String>),
    UnexpectedArc,
}

impl BuiltItem<'_> {
    fn record(self) -> Result<Record, String> {
        match self {
            Self::Node(_, record) => record,
            Self::Leaf(..) => Err("missing value".into()),
            _ => Err("expected benchmark record node".into()),
        }
    }
}

#[cfg(test)]
#[allow(
    dead_code,
    reason = "Compatibility oracle used by integration tests, also compiled by Clippy for the benchmark"
)]
impl RecordBuilder {
    pub fn decode(source: &str) -> Result<Dataset, String> {
        parser::parse_description_with(source, &mut Self).map_err(|e| format!("{e:?}"))?
    }

    fn record(name: &str, items: Vec<BuiltItem<'_>>) -> Result<Record, String> {
        let mut fields = RecordFields::new(name);
        for item in items {
            fields.accept(item)?;
        }
        fields.finish()
    }
}

struct RecordFields {
    record: Record,
    have_value: bool,
    have_text: bool,
}

impl RecordFields {
    fn new(name: &str) -> Self {
        Self {
            record: Record {
                id: name.into(),
                value: 0.0,
                text: String::new(),
                child: Vec::new(),
                link: Vec::new(),
            },
            have_value: false,
            have_text: false,
        }
    }

    fn accept(&mut self, item: BuiltItem<'_>) -> Result<(), String> {
        match item {
            BuiltItem::Leaf("value", value) => {
                let Some(Value::Num(value)) = value else {
                    return Err("expected numeric value".into());
                };
                if self.have_value {
                    return Err("duplicate value".into());
                }
                self.record.value = value;
                self.have_value = true;
            }
            BuiltItem::Leaf("text", value) => {
                let Some(Value::Str(value)) = value else {
                    return Err("expected text".into());
                };
                if self.have_text {
                    return Err("duplicate text".into());
                }
                self.record.text = value.into_owned();
                self.have_text = true;
            }
            // Block-node annotations cannot contain values in the grammar.
            // Preserve field interpretation before any nested record error.
            BuiltItem::Node("value", _) => return Err("expected numeric value".into()),
            BuiltItem::Node("text", _) => return Err("expected text".into()),
            node @ (BuiltItem::Node(..) | BuiltItem::Leaf(..)) => {
                self.record.child.push(node.record()?);
            }
            BuiltItem::Edge("refs", links) => Link::append_owned(&mut self.record.link, links?),
            _ => return Err("unexpected benchmark record field".into()),
        }
        Ok(())
    }

    fn finish(self) -> Result<Record, String> {
        if !self.have_value {
            return Err("missing value".into());
        }
        if !self.have_text {
            return Err("missing text".into());
        }
        Ok(self.record)
    }
}

#[cfg(test)]
impl<'a> parser::builder::OutputBuilder<'a> for RecordBuilder {
    type Item = BuiltItem<'a>;
    type Output = Result<Dataset, String>;

    fn node(&mut self, node: parser::builder::NodeInput<'a, Self::Item>) -> Self::Item {
        let name = node.inner.name;
        match node.inner.body {
            None => BuiltItem::Leaf(name, node.anno.value),
            Some(items) => BuiltItem::Node(name, Self::record(name, items)),
        }
    }

    fn edge(&mut self, edge: parser::builder::EdgeInput<'a, Self::Item>) -> Self::Item {
        let links = edge
            .inner
            .body
            .into_iter()
            .try_fold(Vec::new(), |mut links, item| {
                let BuiltItem::Arc(arc) = item else {
                    return Err("expected link arc".into());
                };
                Link::append_owned(&mut links, arc?);
                Ok(links)
            });
        BuiltItem::Edge(edge.inner.name, links)
    }

    fn arc(&mut self, arc: parser::ast::HyperArc<'a, &'a str>) -> Self::Item {
        BuiltItem::Arc(
            arc.inner
                .refs
                .into_iter()
                .map(|r| Link::from_hymeko(&r))
                .collect(),
        )
    }

    fn description(
        &mut self,
        description: parser::builder::DescriptionInput<'a, Self::Item>,
    ) -> Self::Output {
        description
            .items
            .into_iter()
            .map(BuiltItem::record)
            .collect::<Result<_, _>>()
            .map(|node| Dataset { node })
    }
}

// The production grammar owns syntax; this consumer owns only record semantics.
// Unit-valued reductions append directly to the currently open final container.
struct StreamingRecordBuilder {
    scopes: Vec<RecordScope>,
}

enum RecordPath<'a> {
    One(&'a str),
    Many(Vec<&'a str>),
}

enum RecordScope {
    Root(Result<Vec<Record>, String>),
    Node(Result<RecordFields, String>),
    Edge(Result<Vec<Link>, String>),
}

impl StreamingRecordBuilder {
    fn decode(source: &str) -> Result<Dataset, String> {
        let mut builder = Self {
            scopes: vec![RecordScope::Root(Ok(Vec::new()))],
        };
        parser::parse_description_into(source, &mut builder).map_err(|e| format!("{e:?}"))?
    }

    fn accept(&mut self, item: BuiltItem<'_>) {
        match self.scopes.last_mut().expect("active document") {
            RecordScope::Root(records) => {
                if let Ok(output) = records {
                    match item.record() {
                        Ok(record) => output.push(record),
                        Err(error) => *records = Err(error),
                    }
                }
            }
            RecordScope::Node(fields) => {
                if let Ok(output) = fields
                    && let Err(error) = output.accept(item)
                {
                    *fields = Err(error);
                }
            }
            RecordScope::Edge(links) => {
                if links.is_ok() {
                    *links = Err("expected link arc".into());
                }
            }
        }
    }
}

impl<'a> parser::builder::ConstructionBuilder<'a> for StreamingRecordBuilder {
    fn fuse_literal_blocks(&self) -> bool {
        true
    }

    type Item = ();
    type Items = ();
    type Reference = Result<Link, String>;
    type Path = RecordPath<'a>;
    type Block = &'a str;
    type Output = Result<Dataset, String>;

    fn fuse_literal_arcs(&self) -> bool {
        true
    }

    fn literal_arc(&mut self, refs: Vec<parser::builder::LiteralReference<'a>>) {
        if let Some(RecordScope::Edge(Ok(output))) = self.scopes.last_mut() {
            let links: Vec<Link> = refs
                .into_iter()
                .map(|r| Link {
                    target: r.target.into(),
                    sign: r.sign,
                    weight: r.weight,
                })
                .collect();
            Link::append_owned(output, links);
        } else if !matches!(self.scopes.last(), Some(RecordScope::Edge(_))) {
            self.accept(BuiltItem::UnexpectedArc);
        }
    }

    fn begin_node(
        &mut self,
        name: &'a str,
        _: parser::ast::Anno<'a, &'a str>,
        _: Vec<Self::Reference>,
    ) -> Self::Block {
        self.scopes
            .push(RecordScope::Node(Ok(RecordFields::new(name))));
        name
    }
    fn begin_edge(
        &mut self,
        name: &'a str,
        _: parser::ast::Anno<'a, &'a str>,
        _: Vec<Self::Reference>,
    ) -> Self::Block {
        self.scopes.push(RecordScope::Edge(Ok(Vec::new())));
        name
    }
    fn empty_items(&mut self) {}
    fn one_item(&mut self, _: ()) {}
    fn two_items(&mut self, _: (), _: ()) {}
    fn push_item(&mut self, _: &mut (), _: ()) {}

    fn finish_leaf(&mut self, name: &'a str, anno: parser::ast::Anno<'a, &'a str>) {
        self.accept(BuiltItem::Leaf(name, anno.value));
    }
    fn finish_node(&mut self, name: Self::Block, _: ()) {
        let RecordScope::Node(fields) = self.scopes.pop().expect("node scope") else {
            unreachable!()
        };
        self.accept(BuiltItem::Node(name, fields.and_then(RecordFields::finish)));
    }
    fn finish_edge(&mut self, name: Self::Block, _: ()) {
        let RecordScope::Edge(links) = self.scopes.pop().expect("edge scope") else {
            unreachable!()
        };
        self.accept(BuiltItem::Edge(name, links));
    }

    fn path(&mut self, name: &'a str) -> Self::Path {
        RecordPath::One(name)
    }
    fn path_push(&mut self, path: &mut Self::Path, name: &'a str) {
        match path {
            RecordPath::One(first) => *path = RecordPath::Many(vec![*first, name]),
            RecordPath::Many(parts) => parts.push(name),
        }
    }
    fn canonical_path(&mut self, path: Self::Path) -> parser::ast::Ref<&'a str> {
        parser::ast::Ref {
            path: match path {
                RecordPath::One(name) => vec![name],
                RecordPath::Many(parts) => parts,
            },
        }
    }

    fn finish_reference(
        &mut self,
        sign: i8,
        target: Self::Path,
        anno: parser::ast::Anno<'a, &'a str>,
    ) -> Self::Reference {
        let weight = Link::weight(&anno)?;
        let target = match target {
            RecordPath::One(name) => name.into(),
            RecordPath::Many(parts) => parts.join("."),
        };
        Ok(Link {
            sign,
            target,
            weight,
        })
    }

    fn finish_arc(&mut self, _: parser::ast::Anno<'a, &'a str>, refs: Vec<Self::Reference>) {
        if let Some(RecordScope::Edge(links)) = self.scopes.last_mut() {
            if let Ok(output) = links {
                for reference in refs {
                    match reference {
                        Ok(link) => output.push(link),
                        Err(error) => {
                            *links = Err(error);
                            break;
                        }
                    }
                }
            }
        } else {
            self.accept(BuiltItem::UnexpectedArc);
        }
    }

    fn finish_description(
        &mut self,
        _: &'a str,
        _: parser::ast::HeaderBlock<'a>,
        _: (),
    ) -> Self::Output {
        let RecordScope::Root(records) = self.scopes.pop().expect("document scope") else {
            unreachable!()
        };
        records.map(|mut node| {
            node.shrink_to_fit();
            Dataset { node }
        })
    }
}

#[cfg(test)]
mod literal_arc_tests {
    #[test]
    fn literal_arcs_reuse_compact_buffers_and_bound_retained_slack() {
        use super::*;
        use parser::builder::{ConstructionBuilder, LiteralReference};
        for count in [1, 2, 8, 257] {
            for slack in [0, 16] {
                let mut refs = Vec::with_capacity(count + slack);
                refs.extend((0..count).map(|_| LiteralReference {
                    target: "target",
                    target_offset: 0,
                    sign: -1,
                    weight: -0.0,
                }));
                let pointer = refs.as_ptr().cast::<u8>();
                let mut builder = StreamingRecordBuilder {
                    scopes: vec![RecordScope::Edge(Ok(Vec::new()))],
                };
                builder.literal_arc(refs);
                let RecordScope::Edge(Ok(links)) = &builder.scopes[0] else {
                    panic!("links")
                };
                assert_eq!(links.len(), count);
                assert!(links.capacity() <= count.max(4));
                if slack == 0
                    && std::mem::size_of::<LiteralReference>() == std::mem::size_of::<Link>()
                    && std::mem::align_of::<LiteralReference>() == std::mem::align_of::<Link>()
                {
                    assert_eq!(links.as_ptr().cast::<u8>(), pointer);
                }
                for link in links {
                    assert_eq!(link.target, "target");
                    assert_eq!(link.sign, -1);
                    assert_eq!(link.weight.to_bits(), (-0.0f64).to_bits());
                }
            }
        }
    }
}
