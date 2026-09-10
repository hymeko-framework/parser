//! HyMeKo parser — LALRPOP grammar + SIMD-accelerated lexer.
//!
//! Public entry points:
//! - [`parse_description`] for full `.hymeko` source files.
//! - [`parse_description_with`] for a consumer-selected output builder.
//! - [`parse_query_var`] for single `?name` query-variable fragments
//!   (see `docs/examples/query_variables.md`).
//! - [`read_source_file`] for the common load-from-disk helper.

pub mod ast;
pub mod builder;
mod fusion;
pub mod lexer;

// LALRPOP-generated parser. We expand what `lalrpop_mod!(pub hymeko)` does
// by hand — `include!` against `OUT_DIR/hymeko.rs` — so rust-analyzer can
// follow the path without needing the macro expander. `cargo check` /
// `cargo test` / `cargo build` all produce this file via `build.rs`; make
// sure you've run one of those at least once before the IDE tries to
// resolve `DescriptionParser` and friends.
#[allow(clippy::all, dead_code, unused_imports)]
mod grammar {
    include!(concat!(env!("OUT_DIR"), "/hymeko.rs"));
}

#[allow(clippy::all, dead_code, unused_imports)]
mod fast_grammar {
    include!(concat!(env!("OUT_DIR"), "/fast.rs"));
}

/// Compatibility facades for callers supplying their own token iterator.
pub mod hymeko {
    use crate::{ParseError, ast::AstStr, builder::AstBuilder, grammar, lexer::Token};

    /// Original generated-parser token conversion contract.
    pub trait __ToTriple<'a> {
        fn to_triple(self) -> Result<(usize, Token<'a>, usize), ParseError<'a>>;
    }

    impl<'a, T: grammar::__ToTriple<'a, AstBuilder>> __ToTriple<'a> for T {
        fn to_triple(self) -> Result<(usize, Token<'a>, usize), ParseError<'a>> {
            grammar::__ToTriple::<'a, AstBuilder>::to_triple(self)
        }
    }

    struct TokenInput<T>(T);

    impl<'a, B: crate::builder::ConstructionBuilder<'a>, T: __ToTriple<'a>>
        grammar::__ToTriple<'a, B> for TokenInput<T>
    {
        fn to_triple(self) -> Result<(usize, Token<'a>, usize), ParseError<'a>> {
            self.0.to_triple()
        }
    }

    /// Parses a description from the existing spanned-token interface.
    #[derive(Default)]
    pub struct DescriptionParser {
        _private: (),
    }

    /// Parses one query variable from the existing spanned-token interface.
    #[derive(Default)]
    pub struct QueryVarParser {
        _private: (),
    }

    macro_rules! facade {
        ($parser:ident, $output:ty) => {
            impl $parser {
                pub fn new() -> Self {
                    Self::default()
                }

                /// Preserves token order, lexical errors and grammar diagnostics.
                pub fn parse<'a, T: __ToTriple<'a>, I: IntoIterator<Item = T>>(
                    &self,
                    tokens: I,
                ) -> Result<$output, ParseError<'a>> {
                    grammar::$parser::new()
                        .parse(&mut AstBuilder, tokens.into_iter().map(TokenInput))
                }
            }
        };
    }

    facade!(DescriptionParser, AstStr<'a>);
    facade!(QueryVarParser, &'a str);
}

use std::fs;
use std::io;
use std::path::Path;

use crate::ast::AstStr;
use crate::builder::{AstBuilder, ConstructionBuilder, OutputBuilder};
use crate::hymeko::QueryVarParser;
#[cfg(target_arch = "x86_64")]
use crate::lexer::simd::{Avx2Lexer, Sse2Lexer};
use crate::lexer::simd::{CoreLexer, ScalarLexer};
use crate::lexer::{LexError, Token};

/// A tokenised `Spanned` item produced by any of the SIMD lexer tiers.
type SpannedLexItem<'a> = Result<(usize, Token<'a>, usize), LexError>;

/// A `ParseError` specialised to the parser's token and error types.
pub type ParseError<'a> = lalrpop_util::ParseError<usize, Token<'a>, LexError>;

// ---- Description (top-level .hymeko source) -------------------------------

/// Generic boundary — the compiler instantiates one optimized copy per lexer
/// tier (scalar / SSE2 / AVX2) so the SIMD dispatch stays monomorphic.
#[inline(always)]
fn parse_description_inner<'a, I, B>(
    iter: I,
    input: &'a str,
    builder: &mut B,
) -> Result<B::Output, ParseError<'a>>
where
    I: Iterator<Item = SpannedLexItem<'a>> + lexer::common::CommonLexer<'a>,
    B: ConstructionBuilder<'a>,
{
    if !builder.fuse_literal_arcs() || !fusion::contains_arc_start(input.as_bytes()) {
        return grammar::DescriptionParser::new().parse(builder, iter);
    }
    match fast_grammar::DescriptionParser::new().parse(builder, fusion::Fuser::new(iter)) {
        Ok(output) => Ok(output),
        Err(_) => Err(grammar::DescriptionParser::new()
            .parse(
                &mut builder::SyntaxValidator,
                ScalarLexer(CoreLexer::new(input)),
            )
            .expect_err("token fusion preserves grammar acceptance")),
    }
}

/// Parse a full `.hymeko` source into its AST, picking the best available
/// lexer tier at runtime (AVX2 → SSE2 → scalar fallback).
pub fn parse_description(input: &str) -> Result<AstStr<'_>, ParseError<'_>> {
    parse_description_with(input, &mut AstBuilder)
}

/// Parse the full language while constructing a consumer-selected output.
///
/// Uses the same grammar and SIMD dispatch as [`parse_description`]. Syntax
/// errors are returned unchanged. Builder callbacks can have run before an
/// error; see [`OutputBuilder`] for the non-transactional callback contract.
/// The input must outlive any borrowed data retained by the builder/output.
///
/// ```
/// use parser::{builder::AstBuilder, parse_description_with};
/// let ast = parse_description_with("Demo{} root;", &mut AstBuilder).unwrap();
/// assert_eq!(ast.items.len(), 1);
/// ```
pub fn parse_description_with<'a, B: OutputBuilder<'a>>(
    input: &'a str,
    builder: &mut B,
) -> Result<B::Output, ParseError<'a>> {
    parse_description_into(input, builder)
}

/// Parse the full language with consumer-owned incremental construction.
///
/// See [`ConstructionBuilder`] for block-entry and non-transactional callbacks.
/// Syntax errors remain authoritative even when a consumer accumulates an error.
pub fn parse_description_into<'a, B: ConstructionBuilder<'a>>(
    input: &'a str,
    builder: &mut B,
) -> Result<B::Output, ParseError<'a>> {
    let core = CoreLexer::new(input);

    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx2") {
            return parse_description_inner(Avx2Lexer(core), input, builder);
        }
        if std::is_x86_feature_detected!("sse2") {
            return parse_description_inner(Sse2Lexer(core), input, builder);
        }
    }

    parse_description_inner(ScalarLexer(core), input, builder)
}

// ---- Query-variable fragment ---------------------------------------------

/// Same SIMD-dispatch pattern as [`parse_description_inner`], but for the
/// single-token `?name` fragment.
#[inline(always)]
fn parse_query_var_inner<'a, I>(iter: I) -> Result<&'a str, ParseError<'a>>
where
    I: Iterator<Item = SpannedLexItem<'a>>,
{
    QueryVarParser::new().parse(iter)
}

/// Parse a single `?name` query-variable binding.
///
/// This is the minimal entry point for exercising the `?` token in isolation.
/// Full integration into query/rewrite patterns is tracked in
/// `docs/plans/04_graph_query/T10_lalrpop_extension.md`; until that lands,
/// this parser is useful for tooling (e.g. pattern authoring tools) and
/// grammar-level regression tests.
///
/// # Examples
///
/// ```
/// let name = parser::parse_query_var("?x").unwrap();
/// assert_eq!(name, "x");
/// ```
pub fn parse_query_var(input: &str) -> Result<&str, ParseError<'_>> {
    let core = CoreLexer::new(input);

    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx2") {
            return parse_query_var_inner(Avx2Lexer(core));
        }
        if std::is_x86_feature_detected!("sse2") {
            return parse_query_var_inner(Sse2Lexer(core));
        }
    }

    parse_query_var_inner(ScalarLexer(core))
}

// ---- Disk helper ----------------------------------------------------------

/// Read a `.hymeko` source file from disk. Thin wrapper over
/// [`std::fs::read_to_string`] so callers have a single import path.
pub fn read_source_file<P: AsRef<Path>>(path: P) -> io::Result<String> {
    fs::read_to_string(path)
}
