//! Output construction for the single HyMeKo grammar.

use crate::ast::{
    AstStr, Description, EdgeDecl, EdgeInner, HeaderBlock, HyperAnnotatedElement, HyperArc,
    HyperItem, NodeDecl, NodeInner, SignedRef,
};

/// A completed description whose children are already consumer outputs.
/// The header retains the original canonical syntax types and ordering.
pub struct DescriptionInput<'a, Item> {
    pub name: &'a str,
    pub header: HeaderBlock<'a>,
    pub items: Vec<Item>,
}

/// Construction input shared by nodes and edges; no semantic interpretation.
pub struct DeclarationBody<'a, Body> {
    pub name: &'a str,
    pub bases: Vec<SignedRef<'a, &'a str>>,
    pub body: Body,
}

pub type NodeInput<'a, Item> =
    HyperAnnotatedElement<'a, DeclarationBody<'a, Option<Vec<Item>>>, &'a str>;
pub type EdgeInput<'a, Item> = HyperAnnotatedElement<'a, DeclarationBody<'a, Vec<Item>>, &'a str>;

/// Constructs a consumer's output as syntactic declarations are reduced.
///
/// # Contract
/// Callbacks receive complete declarations with children already constructed,
/// in source order. All header, annotation, inheritance and reference information
/// is retained in the arguments; interpreting or rejecting it is the consumer's
/// responsibility. [`AstBuilder`] preserves it without interpretation.
///
/// Callbacks may run before a later syntax error. Mutations of the builder are
/// **not rolled back** on failure; use a fresh builder when transactional behavior
/// is needed. `description` runs only after a complete description is reduced;
/// successful parsing is established by the outer parse Result, not a callback.
/// Store consumer errors in `Item`/`Output` if syntax errors must take precedence.
/// No callback can recover from or bypass a lexer/grammar error.
pub trait OutputBuilder<'a> {
    type Item;
    type Output;

    fn node(&mut self, node: NodeInput<'a, Self::Item>) -> Self::Item;
    fn edge(&mut self, edge: EdgeInput<'a, Self::Item>) -> Self::Item;
    fn arc(&mut self, arc: HyperArc<'a, &'a str>) -> Self::Item;
    fn description(&mut self, description: DescriptionInput<'a, Self::Item>) -> Self::Output;
}

/// Incremental construction for consumers that do not need intermediate item lists.
///
/// The grammar calls `begin_node` / `begin_edge` after a block's opening brace,
/// then reduces its children in source order, and finally calls `finish_node` / `finish_edge`.
/// Statement nodes do not have a begin callback. Each list is assembled through
/// the item methods, which may use consumer-owned storage instead of a `Vec`.
/// All canonical syntax information is retained. As with [`OutputBuilder`],
/// callbacks are non-transactional and syntax errors take precedence over output.
/// Existing output builders implement this interface through the compatibility
/// adapter below, without any changes to their callback order or input types.
///
/// Block entry receives the complete name, annotations and inheritance list.
/// Its returned handle reaches the matching finish callback; a consumer may keep
/// the metadata there or in its own scope storage. `Path` receives identifiers
/// in order, and `canonical_path` must reconstruct that exact sequence for
/// canonical header and value syntax. Reference signs are always -1, 0 or 1.
/// Consumer-specific validation belongs in the returned items/output; it must
/// not bypass subsequent grammar validation. Use a fresh builder after failure.
pub trait ConstructionBuilder<'a> {
    type Item;
    type Items;
    type Reference;
    type Path;
    type Block;
    type Output;

    fn begin_node(
        &mut self,
        name: &'a str,
        anno: crate::ast::Anno<'a, &'a str>,
        bases: Vec<Self::Reference>,
    ) -> Self::Block;
    fn begin_edge(
        &mut self,
        name: &'a str,
        anno: crate::ast::Anno<'a, &'a str>,
        bases: Vec<Self::Reference>,
    ) -> Self::Block;
    fn empty_items(&mut self) -> Self::Items;
    fn one_item(&mut self, item: Self::Item) -> Self::Items;
    fn two_items(&mut self, first: Self::Item, second: Self::Item) -> Self::Items;
    fn push_item(&mut self, items: &mut Self::Items, item: Self::Item);
    fn finish_leaf(&mut self, name: &'a str, anno: crate::ast::Anno<'a, &'a str>) -> Self::Item;
    fn finish_node(&mut self, block: Self::Block, body: Self::Items) -> Self::Item;
    fn finish_edge(&mut self, block: Self::Block, body: Self::Items) -> Self::Item;
    fn path(&mut self, name: &'a str) -> Self::Path;
    fn path_push(&mut self, path: &mut Self::Path, name: &'a str);
    fn canonical_path(&mut self, path: Self::Path) -> crate::ast::Ref<&'a str>;
    fn finish_reference(
        &mut self,
        sign: i8,
        target: Self::Path,
        anno: crate::ast::Anno<'a, &'a str>,
    ) -> Self::Reference;
    fn finish_arc(
        &mut self,
        anno: crate::ast::Anno<'a, &'a str>,
        refs: Vec<Self::Reference>,
    ) -> Self::Item;
    fn finish_description(
        &mut self,
        name: &'a str,
        header: HeaderBlock<'a>,
        items: Self::Items,
    ) -> Self::Output;
}

impl<'a, B: OutputBuilder<'a>> ConstructionBuilder<'a> for B {
    type Item = B::Item;
    type Items = Vec<B::Item>;
    type Reference = SignedRef<'a, &'a str>;
    type Path = crate::ast::Ref<&'a str>;
    type Block = (&'a str, crate::ast::Anno<'a, &'a str>, Vec<Self::Reference>);
    type Output = B::Output;

    fn begin_node(
        &mut self,
        name: &'a str,
        anno: crate::ast::Anno<'a, &'a str>,
        bases: Vec<Self::Reference>,
    ) -> Self::Block {
        (name, anno, bases)
    }
    fn begin_edge(
        &mut self,
        name: &'a str,
        anno: crate::ast::Anno<'a, &'a str>,
        bases: Vec<Self::Reference>,
    ) -> Self::Block {
        (name, anno, bases)
    }
    fn empty_items(&mut self) -> Self::Items {
        Vec::new()
    }
    fn one_item(&mut self, item: Self::Item) -> Self::Items {
        vec![item]
    }
    fn two_items(&mut self, first: Self::Item, second: Self::Item) -> Self::Items {
        vec![first, second]
    }
    fn push_item(&mut self, items: &mut Self::Items, item: Self::Item) {
        items.push(item);
    }
    fn finish_leaf(&mut self, name: &'a str, anno: crate::ast::Anno<'a, &'a str>) -> Self::Item {
        OutputBuilder::node(
            self,
            NodeInput {
                anno,
                inner: DeclarationBody {
                    name,
                    bases: Vec::new(),
                    body: None,
                },
            },
        )
    }
    fn finish_node(&mut self, block: Self::Block, body: Self::Items) -> Self::Item {
        let (name, anno, bases) = block;
        OutputBuilder::node(
            self,
            NodeInput {
                anno,
                inner: DeclarationBody {
                    name,
                    bases,
                    body: Some(body),
                },
            },
        )
    }
    fn finish_edge(&mut self, block: Self::Block, body: Self::Items) -> Self::Item {
        let (name, anno, bases) = block;
        OutputBuilder::edge(
            self,
            EdgeInput {
                anno,
                inner: DeclarationBody { name, bases, body },
            },
        )
    }
    fn path(&mut self, name: &'a str) -> Self::Path {
        crate::ast::Ref { path: vec![name] }
    }
    fn path_push(&mut self, path: &mut Self::Path, name: &'a str) {
        path.path.push(name);
    }
    fn canonical_path(&mut self, path: Self::Path) -> crate::ast::Ref<&'a str> {
        path
    }
    fn finish_reference(
        &mut self,
        sign: i8,
        target: Self::Path,
        anno: crate::ast::Anno<'a, &'a str>,
    ) -> Self::Reference {
        let atom = crate::ast::RefAtom { target, anno };
        match sign {
            -1 => SignedRef::Minus(atom),
            0 => SignedRef::Neutral(atom),
            1 => SignedRef::Plus(atom),
            _ => unreachable!("grammar sign"),
        }
    }
    fn finish_arc(
        &mut self,
        anno: crate::ast::Anno<'a, &'a str>,
        refs: Vec<Self::Reference>,
    ) -> Self::Item {
        OutputBuilder::arc(
            self,
            HyperArc {
                anno,
                inner: crate::ast::ArcInner { refs },
            },
        )
    }
    fn finish_description(
        &mut self,
        name: &'a str,
        header: HeaderBlock<'a>,
        items: Self::Items,
    ) -> Self::Output {
        OutputBuilder::description(
            self,
            DescriptionInput {
                name,
                header,
                items,
            },
        )
    }
}

/// Compatibility output builder: constructs the original borrowed native AST.
#[derive(Default)]
pub struct AstBuilder;

impl<'a> OutputBuilder<'a> for AstBuilder {
    type Item = HyperItem<'a, &'a str>;
    type Output = AstStr<'a>;

    #[inline]
    fn node(&mut self, node: NodeInput<'a, Self::Item>) -> Self::Item {
        HyperItem::Node(NodeDecl {
            anno: node.anno,
            inner: NodeInner {
                name: node.inner.name,
                bases: node.inner.bases,
                body: node.inner.body,
            },
        })
    }

    #[inline]
    fn edge(&mut self, edge: EdgeInput<'a, Self::Item>) -> Self::Item {
        HyperItem::Edge(EdgeDecl {
            anno: edge.anno,
            inner: EdgeInner {
                name: edge.inner.name,
                bases: edge.inner.bases,
                body: edge.inner.body,
            },
        })
    }

    #[inline]
    fn arc(&mut self, arc: HyperArc<'a, &'a str>) -> Self::Item {
        HyperItem::Arc(arc)
    }

    #[inline]
    fn description(&mut self, description: DescriptionInput<'a, Self::Item>) -> Self::Output {
        let (imports, usings, header, consts) = description.header;
        Description {
            name: description.name,
            header,
            imports,
            usings,
            consts,
            items: description.items,
        }
    }
}
