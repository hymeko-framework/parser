use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq)]
pub struct Description<'a, Id> {
    pub name: Id,
    pub header: Vec<NodeDecl<'a, Id>>,
    pub imports: Vec<ImportStmt<'a, Id>>,
    pub usings: Vec<UsingStmt<Id>>,
    /// Tier B: top-level `const` decls in the description header.
    pub consts: Vec<ConstDecl<Id>>,
    pub items: Vec<HyperItem<'a, Id>>,
}

pub enum HeaderStmt<'a, S> {
    Node(NodeDecl<'a, S>),
    Using(UsingStmt<S>),
    Import(ImportStmt<'a, S>),
    Const(ConstDecl<S>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsingStmt<Id> {
    pub path: Ref<Id>,
    pub alias: Id,
}


#[derive(Debug, Clone, PartialEq)]
pub enum HyperItem<'a, Id> {
    Node(NodeDecl<'a, Id>),
    Edge(EdgeDecl<'a, Id>),
    Arc(HyperArc<'a, Id>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Anno<'a, Id> {
    pub tags: Vec<Id>,        // Zero-copy or minimal allocation
    pub value: Option<Value<'a, Id>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ref<Id> {
    pub path: Vec<Id>
}



#[derive(Debug, Clone, PartialEq)]
pub struct HyperAnnotatedElement<'a, T, Id> {
    pub anno: Anno<'a, Id>,
    pub inner: T,
}

pub type NodeDecl<'a, Id>  = HyperAnnotatedElement<'a, NodeInner<'a, Id>, Id>;
pub type EdgeDecl<'a, Id>  = HyperAnnotatedElement<'a, EdgeInner<'a, Id>, Id>;
pub type HyperArc<'a, Id>  = HyperAnnotatedElement<'a, ArcInner<'a, Id>, Id>;
pub type HeaderBlock<'a> = (
    Vec<ImportStmt<'a, &'a str>>,
    Vec<UsingStmt<&'a str>>,
    Vec<NodeDecl<'a, &'a str>>,
    Vec<ConstDecl<&'a str>>,
);

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInner<'a, Id> {
    pub name: Id,
    pub bases: Vec<SignedRef<'a, Id>>,
    pub body: Option<Vec<HyperItem<'a, Id>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeInner<'a, Id> {
    pub name: Id,
    pub bases: Vec<SignedRef<'a, Id>>,
    pub body: Vec<HyperItem<'a, Id>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArcInner<'a, Id> {
    pub refs: Vec<SignedRef<'a, Id>>,
}

//----
// References
//----






#[derive(Debug, Clone, PartialEq)]
pub struct RefAtom<'a, Id> {
    pub target: Ref<Id>,
    pub anno: Anno<'a, Id>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignedRef<'a, Id> {
    Plus(RefAtom<'a, Id>),
    Minus(RefAtom<'a, Id>),
    Neutral(RefAtom<'a, Id>)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a, Id> {
    Str(Cow<'a, str>), // Zero-copy for fast-path strings
    Num(f64),
    List(Vec<Value<'a, Id>>),
    Ref(Ref<Id>),
    /// A compile-time numeric expression — Tier B: evaluates to f64
    /// during lowering. Allows `[BASE_RADIUS, LINK_LEN * 2]`-style
    /// list elements without losing the literal-only fast path for
    /// ordinary numbers.
    Expr(ConstExpr<Id>),
}

// ─── Compile-time numeric expressions (Tier B) ───────────────────────────

/// Top-level `const NAME = <expr>;` declaration in a description header.
/// Bindings are forward-referenceable (a two-pass resolver collects
/// names first, then evaluates each `value` with the others in scope).
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl<Id> {
    pub name: Id,
    pub value: ConstExpr<Id>,
}

/// Pure numeric expression tree. No side effects, no IO; evaluation is
/// total apart from division-by-zero (a checked error) and undefined
/// identifier references (a checked error).
#[derive(Debug, Clone, PartialEq)]
pub enum ConstExpr<Id> {
    Lit(f64),
    Ref(Id),
    Neg(Box<ConstExpr<Id>>),
    Bin(BinOp, Box<ConstExpr<Id>>, Box<ConstExpr<Id>>),
    /// `pi` — nullary constant. Spelled as a builtin rather than a
    /// reserved identifier so users can still name a `const PI = ...`
    /// of their own if they prefer a different precision.
    Pi,
    /// `exp(x)` — natural exponential.
    Exp(Box<ConstExpr<Id>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

// The core type alias now strictly binds to the file buffer's lifetime
pub type AstStr<'a> = Description<'a, &'a str>;


#[derive(Debug, Clone, PartialEq)]
pub struct ImportStmt<'a, Id> {
    pub path: Cow<'a, str>,
    pub alias: Option<Id>,
}