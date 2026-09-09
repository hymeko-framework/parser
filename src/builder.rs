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
