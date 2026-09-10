#[path = "../benches/formats/mod.rs"]
mod formats;

use formats::adapters::Format;
use formats::model::{Dataset, Pattern};

struct FusedAst<const ARCS: bool = true>;
impl<'a, const ARCS: bool> parser::builder::OutputBuilder<'a> for FusedAst<ARCS> {
    fn fuse_literal_arcs(&self) -> bool {
        ARCS
    }
    type Item = parser::ast::HyperItem<'a, &'a str>;
    type Output = parser::ast::AstStr<'a>;
    fn node(&mut self, node: parser::builder::NodeInput<'a, Self::Item>) -> Self::Item {
        parser::builder::AstBuilder.node(node)
    }
    fn edge(&mut self, edge: parser::builder::EdgeInput<'a, Self::Item>) -> Self::Item {
        parser::builder::AstBuilder.edge(edge)
    }
    fn arc(&mut self, arc: parser::ast::HyperArc<'a, &'a str>) -> Self::Item {
        parser::builder::AstBuilder.arc(arc)
    }
    fn description(
        &mut self,
        input: parser::builder::DescriptionInput<'a, Self::Item>,
    ) -> Self::Output {
        parser::builder::AstBuilder.description(input)
    }
}

#[test]
fn output_builder_remains_object_safe() {
    use parser::builder::{AstBuilder, OutputBuilder};
    let builder: &mut dyn OutputBuilder<
        '_,
        Item = parser::ast::HyperItem<'_, &str>,
        Output = parser::ast::AstStr<'_>,
    > = &mut AstBuilder;
    assert!(!builder.fuse_literal_arcs());
}

#[test]
fn canonical_error_validation_does_not_replay_builder_callbacks() {
    use parser::builder::{DescriptionInput, EdgeInput, NodeInput, OutputBuilder};
    struct Arcs(usize);
    impl<'a> OutputBuilder<'a> for Arcs {
        type Item = ();
        type Output = ();
        fn fuse_literal_arcs(&self) -> bool {
            true
        }
        fn node(&mut self, _: NodeInput<'a, ()>) {}
        fn edge(&mut self, _: EdgeInput<'a, ()>) {}
        fn arc(&mut self, _: parser::ast::HyperArc<'a, &'a str>) {
            self.0 += 1;
        }
        fn description(&mut self, _: DescriptionInput<'a, ()>) {}
    }
    let source = "T{} (+a[1]); @";
    let mut builder = Arcs(0);
    let actual = parser::parse_description_with(source, &mut builder).unwrap_err();
    assert_eq!(actual, parser::parse_description(source).unwrap_err());
    assert_eq!(builder.0, 1);
}

#[test]
fn fused_reference_shapes_match_canonical_ast_and_errors() {
    use parser::hymeko::DescriptionParser;
    use parser::lexer::simd::{CoreLexer, ScalarLexer};
    for sign in ["+", "-", "~"] {
        for path in ["a", "a.b", "<tag>a", "a<tag>"] {
            for weight in [
                "[1]", "[-0.0]", "[1,]", "[1,2]", "[]", "[[1]]", "[\"x\"]", "[", "[-", "[1",
            ] {
                let reference = format!("{sign}{path} /*gap*/ {weight}");
                for source in [
                    format!("T{{}} ({reference});"),
                    format!("T{{}} n:{reference}{{}}"),
                    format!("T{{}} n{{ @e{{({reference});}} }}"),
                    format!("T{{const x=(1{reference});}}"),
                    format!("T{{}} ({reference},+b[2]); @"),
                ] {
                    let expected =
                        DescriptionParser::new().parse(ScalarLexer(CoreLexer::new(&source)));
                    assert_eq!(
                        parser::parse_description_with(&source, &mut FusedAst::<true>),
                        expected,
                        "{source}"
                    );
                }
            }
        }
    }
}

#[test]
fn fused_arc_large_groups_and_malformed_tails_match_canonical() {
    for count in [1, 2, 8, 257, 4096] {
        let refs = (0..count)
            .map(|i| format!("+n{i}[1]"))
            .collect::<Vec<_>>()
            .join(",");
        for suffix in [");", ",);", ")<tag>;", ",+bad[", ",+bad[1]@);", "); @"] {
            let source = format!("T{{}} @links{{({refs}{suffix}}}");
            assert_eq!(
                parser::parse_description_with(&source, &mut FusedAst::<true>),
                parser::parse_description(&source),
                "count={count}, suffix={suffix}"
            );
        }
    }
}

#[test]
fn arc_opt_in_preserves_other_syntax_and_can_be_disabled() {
    for value in [
        "1",
        "-0.0",
        "-3.5",
        "1+2",
        "(1+2)",
        "[1]",
        "\"plain\"",
        "\"escaped\\n\\\"\"",
        "\"unterminated",
        "true",
        "?query",
        "->target",
    ] {
        for tags in ["", "<tag>", "<tag,tag>"] {
            for suffix in [";", "; @", "{}", "; child;", "; (+child[1]);"] {
                let source = format!("T{{}} leaf {tags} {value}{suffix}");
                let expected = parser::parse_description(&source);
                assert_eq!(
                    parser::parse_description_with(&source, &mut FusedAst::<false>),
                    expected
                );
                assert_eq!(
                    parser::parse_description_with(&source, &mut FusedAst::<true>),
                    expected
                );
            }
        }
    }
}

#[test]
fn incremental_construction_preserves_scope_order_and_deep_inputs() {
    use parser::ast::{Anno, HeaderBlock, SignedRef};
    use parser::builder::ConstructionBuilder;
    #[derive(Default)]
    struct Counter(Vec<String>);
    impl<'a> ConstructionBuilder<'a> for Counter {
        fn fuse_literal_arcs(&self) -> bool {
            true
        }
        type Item = usize;
        type Items = usize;
        type Reference = SignedRef<'a, &'a str>;
        type Path = parser::ast::Ref<&'a str>;
        type Block = &'a str;
        type Output = usize;
        fn begin_node(
            &mut self,
            name: &'a str,
            _: Anno<'a, &'a str>,
            _: Vec<Self::Reference>,
        ) -> Self::Block {
            self.0.push(format!("open node {name}"));
            name
        }
        fn begin_edge(
            &mut self,
            name: &'a str,
            _: Anno<'a, &'a str>,
            _: Vec<Self::Reference>,
        ) -> Self::Block {
            self.0.push(format!("open edge {name}"));
            name
        }
        fn empty_items(&mut self) -> usize {
            0
        }
        fn one_item(&mut self, item: usize) -> usize {
            item
        }
        fn two_items(&mut self, a: usize, b: usize) -> usize {
            a + b
        }
        fn push_item(&mut self, items: &mut usize, item: usize) {
            *items += item;
        }
        fn finish_leaf(&mut self, name: &'a str, _: Anno<'a, &'a str>) -> usize {
            self.0.push(format!("node {name}"));
            1
        }
        fn finish_node(&mut self, name: Self::Block, body: usize) -> usize {
            self.0.push(format!("node {name}"));
            1 + body
        }
        fn finish_edge(&mut self, name: Self::Block, body: usize) -> usize {
            self.0.push(format!("edge {name}"));
            1 + body
        }
        fn path(&mut self, name: &'a str) -> Self::Path {
            parser::builder::AstBuilder.path(name)
        }
        fn path_push(&mut self, path: &mut Self::Path, name: &'a str) {
            parser::builder::AstBuilder.path_push(path, name);
        }
        fn canonical_path(&mut self, path: Self::Path) -> parser::ast::Ref<&'a str> {
            path
        }
        fn finish_reference(
            &mut self,
            sign: i8,
            target: parser::ast::Ref<&'a str>,
            anno: Anno<'a, &'a str>,
        ) -> Self::Reference {
            parser::builder::AstBuilder.finish_reference(sign, target, anno)
        }
        fn finish_arc(&mut self, _: Anno<'a, &'a str>, _: Vec<Self::Reference>) -> usize {
            self.0.push("arc".into());
            1
        }
        fn finish_description(&mut self, _: &'a str, _: HeaderBlock<'a>, items: usize) -> usize {
            self.0.push("description".into());
            items
        }
    }
    let mut counter = Counter::default();
    assert_eq!(
        parser::parse_description_into("T{header;} a; n{b; @e{(+x[1]);}} c;", &mut counter),
        Ok(6)
    );
    assert_eq!(
        counter.0,
        [
            "node a",
            "open node n",
            "node b",
            "open edge e",
            "arc",
            "edge e",
            "node n",
            "node c",
            "description"
        ]
    );
    for count in 0..=4 {
        assert_eq!(
            parser::parse_description_into(
                &format!("T{{}} {}", "n;".repeat(count)),
                &mut Counter::default()
            ),
            Ok(count)
        );
    }
    let mut failed = Counter::default();
    assert!(parser::parse_description_into("T{} n{leaf; @", &mut failed).is_err());
    assert_eq!(failed.0, ["open node n", "node leaf"]);
    let deep = format!("T{{}} {}leaf;{}", "n{".repeat(1024), "}".repeat(1024));
    assert_eq!(
        parser::parse_description_into(&deep, &mut Counter::default()),
        Ok(1025)
    );
}

#[test]
fn incremental_values_preserve_headers_lists_and_reference_errors() {
    for value in [
        "0", "-0.0", "[]", "[1]", "[-0.0,]", "[1,2]", "[[1]]", "[[1],2]", "[1,[2]]", "a.b",
        "(1+2)", "\"x\"", "[\"x\"]",
    ] {
        for field in [
            format!("value {value}; text \"ok\";"),
            format!("value 1; text {value};"),
            format!("value 1; text \"ok\"; @refs{{(+a.b.c {value});}}"),
        ] {
            for header in [
                String::new(),
                format!("h<tag,tag> {value}; using a.b as c;"),
            ] {
                let source = format!("Test{{{header}}} root: +base [3] {{{field}}}");
                let expected = parser::parse_description(&source)
                    .map_err(|e| format!("{e:?}"))
                    .and_then(Dataset::from_hymeko);
                assert_eq!(Format::Hymeko.decode(&source), expected, "{source}");
            }
        }
    }
}

#[test]
fn recursive_shapes_match_legacy_parser() {
    use parser::hymeko::DescriptionParser;
    use parser::lexer::simd::{CoreLexer, ScalarLexer};
    for depth in [1, 2, 8, 16, 32, 64] {
        for (open, close, leaf) in [("n{", "}", "leaf;"), ("[", "]", "1")] {
            let body = format!("{}{}{}", open.repeat(depth), leaf, close.repeat(depth));
            let source = if open == "[" {
                format!("Test{{}} n {body};")
            } else {
                format!("Test{{}} {body}")
            };
            let legacy = DescriptionParser::new().parse(ScalarLexer(CoreLexer::new(&source)));
            assert_eq!(parser::parse_description(&source), legacy, "depth={depth}");
        }
    }
}

#[test]
fn record_builder_transfers_compact_link_buffers_without_retaining_slack() {
    use formats::adapters::{BuiltItem, RecordBuilder};
    use formats::model::Link;
    use parser::ast::{Anno, Value};
    use parser::builder::{DeclarationBody, EdgeInput, NodeInput, OutputBuilder};

    for count in [1, 2, 8, 17] {
        for slack in [0, 128] {
            let mut links = Vec::with_capacity(count + slack);
            links.extend((0..count).map(|i| Link {
                target: format!("n{i}"),
                sign: 1,
                weight: i as f64,
            }));
            let original = links.clone();
            let pointer = links.as_ptr();
            let mut builder = RecordBuilder;
            let edge = builder.edge(EdgeInput {
                anno: Anno::default(),
                inner: DeclarationBody {
                    name: "refs",
                    bases: Vec::new(),
                    body: vec![
                        BuiltItem::Arc(Ok(Vec::with_capacity(128))),
                        BuiltItem::Arc(Ok(links)),
                        BuiltItem::Arc(Ok(Vec::new())),
                    ],
                },
            });
            let BuiltItem::Edge(_, Ok(edge_links)) = &edge else {
                panic!("edge")
            };
            assert_eq!(edge_links, &original);
            if slack == 0 {
                assert_eq!(edge_links.as_ptr(), pointer, "arc buffer should transfer");
            }
            assert!(edge_links.capacity() <= count.max(4));
            let edge_pointer = edge_links.as_ptr();
            let node = builder.node(NodeInput {
                anno: Anno::default(),
                inner: DeclarationBody {
                    name: "root",
                    bases: Vec::new(),
                    body: Some(vec![
                        BuiltItem::Leaf("value", Some(Value::Num(1.0))),
                        BuiltItem::Leaf("text", Some(Value::Str("text".into()))),
                        edge,
                    ]),
                },
            });
            let BuiltItem::Node(_, Ok(record)) = node else {
                panic!("record")
            };
            assert_eq!(record.link, original);
            assert_eq!(
                record.link.as_ptr(),
                edge_pointer,
                "edge buffer should transfer"
            );
            assert!(record.link.capacity() <= count.max(4));
        }
    }
}

#[test]
fn direct_builder_matches_owned_records_and_error_precedence() {
    use formats::adapters::RecordBuilder;
    for pattern in Pattern::ALL {
        for count in [0, 1, 2, 17, 1000] {
            let expected = Dataset::generate(pattern, count);
            let source = Format::Hymeko.encode(&expected).unwrap();
            let direct = RecordBuilder::decode(&source).unwrap();
            assert_eq!(direct, expected);
            assert_eq!(Format::Hymeko.decode(&source).unwrap(), expected);
        }
    }
    let bodies = [
        "",
        "value 1;",
        "text \"a\";",
        "value 1; text \"a\";",
        "value 1; value 2; text \"a\";",
        "value \"bad\"; text 3;",
        "value 1; text \"a\"; text \"b\";",
        "value{} text \"a\";",
        "value 1; text{value 3; text \"b\";}",
        "child;",
        "child{}",
        "@refs{(+a [1],-b [-0.0]);}",
        "value 1; text \"a\"; @refs{(+a [1]);(-b [2]);} @refs{(~c [3]);}",
        "@refs{(+a);}",
        "value 1; text \"a\"; @refs{(+a.b.c [1], -<tag>x.y<other> [2]);}",
        "@refs{not_an_arc;} value \"bad\";",
        "@other{(+a);}",
        "value -0.0; text \"é\\n🙂\"; @refs{(+a [2]);} @refs{(-b [3]);}",
    ];
    for name in ["root", "value", "text"] {
        for body in bodies {
            for suffix in ["", " @", " \"unfinished", " other;"] {
                let source = format!("Test{{}} {name}{{{body}}}{suffix}");
                let original = parser::parse_description(&source)
                    .map_err(|e| format!("{e:?}"))
                    .and_then(Dataset::from_hymeko);
                assert_eq!(RecordBuilder::decode(&source), original, "{source}");
                assert_eq!(
                    Format::Hymeko.decode(&source),
                    original,
                    "streaming: {source}"
                );
            }
        }
    }
}

#[test]
#[ignore = "allocation profile; explicit opt-in with --ignored --exact"]
fn direct_builder_retained_storage_profile() {
    fn retained_bytes(data: &Dataset) -> usize {
        use formats::model::{Link, Record};
        let mut total = data.node.capacity() * std::mem::size_of::<Record>();
        let mut pending: Vec<_> = data.node.iter().collect();
        while let Some(record) = pending.pop() {
            total += record.id.capacity() + record.text.capacity();
            total += record.child.capacity() * std::mem::size_of::<Record>();
            total += record.link.capacity() * std::mem::size_of::<Link>();
            total += record
                .link
                .iter()
                .map(|link| link.target.capacity())
                .sum::<usize>();
            pending.extend(&record.child);
        }
        total
    }
    for pattern in Pattern::ALL {
        let expected = Dataset::generate(pattern, 1000);
        let source = Format::Hymeko.encode(&expected).unwrap();
        let direct = Format::Hymeko.decode(&source).unwrap();
        let original = Dataset::from_hymeko(parser::parse_description(&source).unwrap()).unwrap();
        assert_eq!(direct, expected);
        assert!(retained_bytes(&direct) <= retained_bytes(&original));
        println!(
            "RETAINED {pattern:?}: original={} direct={}",
            retained_bytes(&original),
            retained_bytes(&direct)
        );
    }
}

#[test]
fn custom_builder_and_legacy_token_callers_keep_their_contracts() {
    use parser::ast::{Description, HyperArc};
    use parser::builder::{DescriptionInput, EdgeInput, NodeInput, OutputBuilder};
    use parser::hymeko::{__ToTriple, DescriptionParser, QueryVarParser};
    use parser::lexer::{
        Token,
        simd::{CoreLexer, ScalarLexer},
    };
    let empty = Description {
        name: "Empty",
        header: Vec::new(),
        imports: Vec::new(),
        usings: Vec::new(),
        consts: Vec::new(),
        items: Vec::new(),
    };
    assert!(empty.items.is_empty());
    struct Count(usize);
    impl<'a> OutputBuilder<'a> for Count {
        type Item = usize;
        type Output = usize;
        fn node(&mut self, n: NodeInput<'a, usize>) -> usize {
            self.0 += 1;
            1 + n.inner.body.unwrap_or_default().into_iter().sum::<usize>()
        }
        fn edge(&mut self, e: EdgeInput<'a, usize>) -> usize {
            self.0 += 1;
            10 + e.inner.body.into_iter().sum::<usize>()
        }
        fn arc(&mut self, a: HyperArc<'a, &'a str>) -> usize {
            self.0 += 1;
            assert_eq!(a.inner.refs.len(), 1);
            100
        }
        fn description(&mut self, d: DescriptionInput<'a, usize>) -> usize {
            assert_eq!(d.header.2[0].inner.name, "h");
            d.items.into_iter().sum()
        }
    }
    let source = "Test{h;} root{child; @edge{(+child);}}";
    let mut builder = Count(0);
    assert_eq!(
        parser::parse_description_with(source, &mut builder),
        Ok(112)
    );
    assert_eq!(builder.0, 4);
    assert_eq!(
        DescriptionParser::new().parse(ScalarLexer(CoreLexer::new(source))),
        parser::parse_description(source)
    );
    assert_eq!(
        QueryVarParser::default().parse([(0, Token::Question, 1), (1, Token::Ident("x"), 2)]),
        Ok("x")
    );
    let mut incomplete = Count(0);
    assert!(parser::parse_description_with("Test{h;} a; b; @", &mut incomplete).is_err());
    assert!(
        incomplete.0 > 0,
        "callbacks before a syntax error are not rolled back"
    );
    assert_eq!(
        incomplete.0, 2,
        "diagnostic validation must not replay user callbacks"
    );
    struct CustomError;
    impl<'a> __ToTriple<'a> for CustomError {
        fn to_triple(self) -> Result<(usize, Token<'a>, usize), parser::ParseError<'a>> {
            Err(parser::ParseError::InvalidToken { location: 17 })
        }
    }
    assert_eq!(
        DescriptionParser::default().parse([CustomError]),
        Err(parser::ParseError::InvalidToken { location: 17 })
    );
}

#[test]
fn declaration_reductions_preserve_ast_and_diagnostics() {
    // Freeze the released frontend's complete Debug representation, including
    // expected-token sets and byte offsets on every UTF-8-safe truncation.
    // This oracle is independent of which reductions LALRPOP chooses to emit.
    let sources = [
        "Test{} leaf; block{} @edge{} (+leaf);",
        "Test{header<tag> [1,[-0.0],\"é\\n🙂\"]; const k=2*pi; using a.b as c; @\"é.hk\"->m;} n<one,two>:base,+other { x -> m.y; @e<tag>:~base {(+n,-<pre>m.y<post> [2]);} }",
        "Test{} a [[],[1,2,],(-2+3*exp(1)),path.to]; @e [1] {(+a [1],-a [2],~a [3],)<tag> 4;} ",
        "Test{} @outer{n { @inner{ (+n)<a,b> \"text\"; } } (+n);}",
        "Test{} n:base<before> [2],other, { field<int> -0.0; }",
        "Test{} n \"árvíz\\n日本語🙂\";",
    ];
    let mut digest = blake3::Hasher::new();
    let mut accepted = 0;
    let mut rejected = 0;
    for source in sources {
        assert!(parser::parse_description(source).is_ok(), "{source}");
        for end in (0..=source.len()).filter(|&i| source.is_char_boundary(i)) {
            for suffix in ["", ";", "}", "@", " [", " \"unterminated"] {
                let input = format!("{}{suffix}", &source[..end]);
                let result = parser::parse_description(&input);
                assert_eq!(
                    parser::parse_description_with(&input, &mut FusedAst::<true>),
                    result
                );
                accepted += usize::from(result.is_ok());
                rejected += usize::from(result.is_err());
                digest.update(format!("{input:?}\n{result:?}\n").as_bytes());
            }
        }
    }
    assert!(accepted > 20 && rejected > 1000);
    assert_eq!(
        digest.finalize().to_hex().as_str(),
        "48cb09b9be3d3afe1df9c12b8468f9f8a4f7d644e39b42fc10b60e8c9bf5798d"
    );
}

#[test]
fn owned_ast_strings_transfer_their_allocation() {
    use parser::ast::{HyperItem, Value};
    use std::borrow::Cow;

    for empty in [false, true] {
        let source = "FormatBench{} n{value -0.0;text \"line\\ntext\";}";
        let mut ast = parser::parse_description(source).unwrap();
        let HyperItem::Node(node) = &mut ast.items[0] else {
            panic!("record")
        };
        let HyperItem::Node(field) = &mut node.inner.body.as_mut().unwrap()[1] else {
            panic!("text field")
        };
        let Some(Value::Str(text)) = field.anno.value.as_mut() else {
            panic!("string")
        };
        if empty {
            *text = Cow::Owned(String::with_capacity(32));
        }
        let Cow::Owned(buffer) = text else {
            panic!("escaped string must be owned")
        };
        let pointer = buffer.as_ptr();
        let capacity = buffer.capacity();
        let records = Dataset::from_hymeko(ast).unwrap();
        assert_eq!(
            records.node[0].text.as_ptr(),
            pointer,
            "must move the existing buffer"
        );
        assert_eq!(records.node[0].text.capacity(), capacity);
        assert_eq!(records.node[0].text, if empty { "" } else { "line\ntext" });
        assert_eq!(records.node[0].value.to_bits(), (-0.0f64).to_bits());
    }
}

#[test]
fn record_buffers_preserve_ownership_through_nested_conversion() {
    use parser::ast::{HyperItem, Value};
    use std::borrow::Cow;
    use std::collections::HashMap;

    for pattern in Pattern::ALL {
        let expected = Dataset::generate(pattern, 17);
        let source = Format::Hymeko.encode(&expected).unwrap();
        let ast = parser::parse_description(&source).unwrap();
        let mut pending: Vec<_> = ast.items.iter().collect();
        let mut pointers = HashMap::new();
        while let Some(HyperItem::Node(node)) = pending.pop() {
            for item in node.inner.body.as_deref().unwrap_or_default() {
                if let HyperItem::Node(field) = item {
                    if field.inner.name == "text" {
                        let Some(Value::Str(text)) = &field.anno.value else {
                            panic!("text field")
                        };
                        pointers.insert(
                            node.inner.name.to_owned(),
                            (text.as_ptr(), matches!(text, Cow::Owned(_))),
                        );
                    } else if field.inner.name != "value" {
                        pending.push(item);
                    }
                }
            }
        }
        assert_eq!(pointers.len(), 17);
        let actual = Dataset::from_hymeko(ast).unwrap();
        for record in actual.records() {
            let (pointer, was_owned) = pointers[&record.id];
            assert_eq!(
                record.text.as_ptr() == pointer,
                was_owned,
                "{}: {}",
                pattern.name(),
                record.id
            );
        }
        drop(source);
        assert_eq!(actual, expected, "records must survive the source buffer");
    }
}

#[test]
fn preparsed_records_preserve_fields_and_reject_invalid_shapes() {
    for pattern in Pattern::ALL {
        let expected = Dataset::generate(pattern, 17);
        let source = Format::Hymeko.encode(&expected).unwrap();
        let ast = parser::parse_description(&source).unwrap();
        assert_eq!(Dataset::from_hymeko(ast).unwrap(), expected);
    }
    for body in [
        "@bad{}",
        "n{}",
        "n{value 1;}",
        "n{value 1;text 2;}",
        "n{value 1;value 2;text \"x\";}",
        "n{value 1;text \"x\";text \"y\";}",
        "n{value 1;text \"x\";@refs{(+n);}}",
        "n{value 1;text \"x\";@refs{(+n[1,2]);}}",
    ] {
        let source = format!("FormatBench{{}} {body}");
        let ast = parser::parse_description(&source).unwrap();
        assert!(Dataset::from_hymeko(ast).is_err(), "{body}");
    }
}

#[test]
fn formats_preserve_all_common_records() {
    for pattern in Pattern::ALL {
        for count in [0, 1, 17, 100] {
            let expected = Dataset::generate(pattern, count);
            assert_eq!(expected.records().len(), count);
            expected.validate().unwrap();
            for format in Format::ALL {
                let text = format.encode(&expected).unwrap();
                format.document(&text).unwrap();
                let actual = format.decode(&text).unwrap();
                assert_eq!(
                    actual,
                    expected,
                    "{} {} {count}",
                    format.name(),
                    pattern.name()
                );
                actual.validate().unwrap();
            }
        }
    }
}

#[test]
fn independent_fields_escapes_and_negative_zero_survive() {
    let mut expected = Dataset::generate(Pattern::Linked, 3);
    expected.node[0].text = "quotes: \"x\"; slash: \\; line:\n日本語 & < >".into();
    expected.node[0].value = -0.0;
    assert_eq!(expected.node[0].link.len(), 8);
    assert_eq!(expected.node[0].link[1].target, "n2");
    assert_eq!(expected.node[0].link[1].sign, 0);
    assert_eq!(expected.node[0].link[1].weight, 0.125);
    for format in Format::ALL {
        let actual = format.decode(&format.encode(&expected).unwrap()).unwrap();
        assert_eq!(actual, expected, "{}", format.name());
        assert_eq!(actual.node[0].value.to_bits(), (-0.0f64).to_bits());
    }
}

#[test]
fn malformed_documents_cannot_be_timed_as_success() {
    for (format, text) in [
        (Format::Hymeko, "FormatBench{} broken{"),
        (Format::Json, "{\"node\":["),
        (Format::Xml, "<dataset><node>"),
        (Format::Yaml, "node: [unterminated"),
    ] {
        assert!(format.document(text).is_err(), "{}", format.name());
        assert!(format.decode(text).is_err(), "{}", format.name());
    }
}

#[test]
fn semantic_oracle_detects_mutations_and_bad_links() {
    let expected = Dataset::generate(Pattern::Linked, 17);
    let mut changed = expected.clone();
    changed.node[0].link[0].weight += 1.0;
    assert_ne!(changed, expected);
    changed.node[0].link[0].target = "absent".into();
    assert!(changed.validate().is_err());
    changed = expected.clone();
    changed.node[1].id = changed.node[0].id.clone();
    assert!(changed.validate().is_err());
    assert!(Pattern::parse("bad").is_err());
    assert!(Format::parse_name("bad").is_err());
}

#[test]
fn fixture_preparation_records_checked_bytes() {
    let directory =
        std::env::temp_dir().join(format!("hymeko-format-fixtures-{}", std::process::id()));
    let fixtures = formats::Fixture::prepare(&directory, Pattern::Nested, 17).unwrap();
    assert_eq!(fixtures.len(), 4);
    for fixture in fixtures {
        let text = std::fs::read(directory.join(&fixture.file)).unwrap();
        assert_eq!(text.len(), fixture.bytes);
        assert_eq!(blake3::hash(&text).to_hex().as_str(), fixture.blake3);
        std::fs::remove_file(directory.join(fixture.file)).unwrap();
    }
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn escaped_unicode_agrees_across_lexer_tiers() {
    use parser::lexer::Token;
    use parser::lexer::simd::{Avx2Lexer, CoreLexer, ScalarLexer, Sse2Lexer};
    for padding in (0..72).chain([1023, 1024, 1025, 65536]) {
        for payload in [
            "",
            "plain text",
            "árvíz\n日本語",
            "\tλ🙂",
            "é\\quote\"z",
            "ASCII\r\n",
        ] {
            let expected = format!("{}{payload}", "a".repeat(padding));
            let source = serde_json::to_string(&expected).unwrap();
            let scalar = ScalarLexer(CoreLexer::new(&source))
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(scalar.len(), 1);
            assert_eq!((scalar[0].0, scalar[0].2), (0, source.len()));
            assert!(matches!(&scalar[0].1, Token::Str(s) if s == &expected));
            assert_eq!(
                matches!(&scalar[0].1, Token::Str(std::borrow::Cow::Borrowed(_))),
                !source.contains('\\')
            );
            #[cfg(target_arch = "x86_64")]
            {
                if std::is_x86_feature_detected!("sse2") {
                    let sse = Sse2Lexer(CoreLexer::new(&source))
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    assert_eq!(sse, scalar);
                }
                if std::is_x86_feature_detected!("avx2") {
                    let avx = Avx2Lexer(CoreLexer::new(&source))
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    assert_eq!(avx, scalar);
                }
            }
        }
    }
}

#[test]
fn separated_lists_preserve_order_and_trailing_separator_rules() {
    use parser::ast::{HyperItem, SignedRef, Value};
    for count in [1, 2, 3, 4, 7, 8, 9, 16, 17, 257, 1025] {
        let names: Vec<_> = (0..count).map(|i| format!("n{i}")).collect();
        let tags = names.join(",");
        let path = names.join(".");
        let refs = names
            .iter()
            .map(|n| format!("+{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let values = (0..count)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        for tail in ["", ","] {
            let source = format!(
                "Test{{using {path} as alias;}} node<{tags}{tail}>:{tags}{tail}{{\
                 value [{values}{tail}]; target -> {path};\
                 @edge:{refs}{tail}{{({refs}{tail});}}}}"
            );
            let ast = parser::parse_description(&source).unwrap();
            assert_eq!(ast.usings[0].path.path, names);
            let HyperItem::Node(node) = &ast.items[0] else {
                panic!("node")
            };
            assert_eq!(node.anno.tags, names);
            assert_eq!(node.inner.bases.len(), count);
            let body = node.inner.body.as_ref().unwrap();
            let HyperItem::Node(value) = &body[0] else {
                panic!("value")
            };
            assert_eq!(
                value.anno.value,
                Some(Value::List(
                    (0..count).map(|i| Value::Num(i as f64)).collect()
                ))
            );
            let HyperItem::Node(target) = &body[1] else {
                panic!("target")
            };
            assert!(matches!(&target.anno.value, Some(Value::Ref(r)) if r.path == names));
            let HyperItem::Edge(edge) = &body[2] else {
                panic!("edge")
            };
            let HyperItem::Arc(arc) = &edge.inner.body[0] else {
                panic!("arc")
            };
            assert_eq!(arc.inner.refs, edge.inner.bases);
            let expected_signed: Vec<_> = node
                .inner
                .bases
                .iter()
                .map(|base| match base {
                    SignedRef::Neutral(atom) => SignedRef::Plus(atom.clone()),
                    _ => panic!("unsigned base must stay neutral"),
                })
                .collect();
            assert_eq!(arc.inner.refs, expected_signed);
            assert_eq!(arc.inner.refs.len(), count);
            for (reference, name) in arc.inner.refs.iter().zip(&names) {
                assert!(
                    matches!(reference, SignedRef::Plus(r) if r.target.path == [name.as_str()])
                );
            }
        }
    }
    assert!(parser::parse_description("Test{} a []; b [[1,2,],[],[3],];").is_ok());
    for source in [
        "Test{} a [1,,2];",
        "Test{} a [,];",
        "Test{} a <>;",
        "Test{} a: {}",
        "Test{} ();",
        "Test{} (+a,,+b);",
        "Test{} a -> x.;",
        "Test{} a -> x..y;",
        "Test{} a<x,,y>;",
    ] {
        assert!(parser::parse_description(source).is_err(), "{source}");
    }
}

#[test]
fn optional_annotations_preserve_values_and_ordered_duplicate_tags() {
    use parser::ast::{HyperItem, SignedRef, Value};
    for case in 0..48 {
        let prefix = ["", "<pre,dup>"][case % 2];
        let suffix = ["", "<dup,post>"][case / 2 % 2];
        let value = ["", " 3", " [1,[2,3,],]", " \"ab\\ncd\""][case / 4 % 4];
        let sign = ["+", "-", "~"][case / 16];
        let source = format!(
            "Test{{header{suffix}{value};}} node{suffix}{value};\
                         @edge{suffix}{value}{{({sign}{prefix}node{suffix}{value});}}"
        );
        let ast = parser::parse_description(&source).unwrap();
        let expected_value = match value {
            "" => None,
            " 3" => Some(Value::Num(3.0)),
            " \"ab\\ncd\"" => Some(Value::Str("ab\ncd".into())),
            _ => Some(Value::List(vec![
                Value::Num(1.0),
                Value::List(vec![Value::Num(2.0), Value::Num(3.0)]),
            ])),
        };
        let expected_suffix = if suffix.is_empty() {
            vec![]
        } else {
            vec!["dup", "post"]
        };
        assert_eq!(ast.header[0].anno.tags, expected_suffix);
        assert_eq!(ast.header[0].anno.value, expected_value);
        let HyperItem::Node(node) = &ast.items[0] else {
            panic!("node")
        };
        let HyperItem::Edge(edge) = &ast.items[1] else {
            panic!("edge")
        };
        assert_eq!(node.anno, ast.header[0].anno);
        assert_eq!(edge.anno, node.anno);
        let HyperItem::Arc(arc) = &edge.inner.body[0] else {
            panic!("arc")
        };
        assert!(arc.anno.tags.is_empty());
        assert!(arc.anno.value.is_none());
        let atom = match &arc.inner.refs[0] {
            SignedRef::Plus(r) => {
                assert_eq!(sign, "+");
                r
            }
            SignedRef::Minus(r) => {
                assert_eq!(sign, "-");
                r
            }
            SignedRef::Neutral(r) => {
                assert_eq!(sign, "~");
                r
            }
        };
        let mut expected_tags = if prefix.is_empty() {
            vec![]
        } else {
            vec!["pre", "dup"]
        };
        expected_tags.extend(expected_suffix);
        assert_eq!(atom.anno.tags, expected_tags);
        assert_eq!(atom.anno.value, expected_value);
        assert_eq!(atom.target.path, ["node"]);
    }
}

#[test]
fn string_errors_keep_positions_and_unknown_escape_semantics() {
    use parser::lexer::Token;
    use parser::lexer::simd::{CoreLexer, ScalarLexer};
    for padding in 0..72 {
        for suffix in ["", "\\", "\\\"", "é\\"] {
            let source = format!("   \"{}{}", "a".repeat(padding), suffix);
            let mut lexer = ScalarLexer(CoreLexer::new(&source));
            let error = lexer.next().unwrap().unwrap_err();
            assert_eq!(error.at, 3);
            assert_eq!(error.msg, "Unterminated string literal");
            if padding > 0 {
                let next = lexer.next().unwrap().unwrap();
                assert_eq!((next.0, next.2), (4, 4 + padding));
            }
        }
    }
    for (source, expected) in [
        (r#""\q" tail;"#, "q"),
        (r#""\u0041" tail;"#, "u0041"),
        (r#""\🙂" tail;"#, "🙂"),
    ] {
        let tokens = ScalarLexer(CoreLexer::new(source))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(&tokens[0].1, Token::Str(s) if s == expected));
        assert!(matches!(&tokens[1].1, Token::Ident("tail")));
        assert_eq!(tokens[0].2 + 1, tokens[1].0);
        assert_eq!(tokens[2].2, source.len());
    }
}

#[test]
fn mixed_item_sequences_preserve_empty_single_pair_and_large_bodies() {
    use parser::ast::HyperItem;
    for count in [0, 1, 2, 3, 4, 5, 8, 17, 257] {
        let source = (0..count)
            .map(|i| match i % 3 {
                0 => format!("n{i} {i};"),
                1 => format!("@e{i}{{}}"),
                _ => format!("(+n{});", i - 2),
            })
            .collect::<String>();
        let document = format!("Test{{}} {source}");
        let expected = parser::parse_description(&document).unwrap().items;
        assert_eq!(expected.len(), count);
        for (index, item) in expected.iter().enumerate() {
            match item {
                HyperItem::Node(n) => {
                    assert_eq!(index % 3, 0);
                    assert_eq!(n.inner.name, format!("n{index}"));
                    assert_eq!(n.anno.value, Some(parser::ast::Value::Num(index as f64)));
                }
                HyperItem::Edge(e) => {
                    assert_eq!(index % 3, 1);
                    assert_eq!(e.inner.name, format!("e{index}"));
                    assert!(e.inner.body.is_empty());
                }
                HyperItem::Arc(a) => {
                    assert_eq!(index % 3, 2);
                    assert_eq!(a.inner.refs.len(), 1);
                }
            }
        }
        for prefix in ["wrapper", "@wrapper"] {
            let wrapped = format!("Test{{}} {prefix}{{{source}}}");
            let ast = parser::parse_description(&wrapped).unwrap();
            let body = match &ast.items[0] {
                HyperItem::Node(n) => n.inner.body.as_ref().unwrap(),
                HyperItem::Edge(e) => &e.inner.body,
                HyperItem::Arc(_) => panic!("expected wrapper"),
            };
            assert_eq!(body, &expected);
            assert!(parser::parse_description(wrapped.strip_suffix('}').unwrap()).is_err());
        }
    }
}
