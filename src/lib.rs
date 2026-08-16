//! HyMeKo parser — LALRPOP grammar + SIMD-accelerated lexer.
//!
//! Public entry points:
//! - [`parse_description`] for full `.hymeko` source files.
//! - [`parse_query_var`] for single `?name` query-variable fragments
//!   (see `docs/examples/query_variables.md`).
//! - [`read_source_file`] for the common load-from-disk helper.

pub mod ast;
pub mod lexer;

// LALRPOP-generated parser. We expand what `lalrpop_mod!(pub hymeko)` does
// by hand — `include!` against `OUT_DIR/hymeko.rs` — so rust-analyzer can
// follow the path without needing the macro expander. `cargo check` /
// `cargo test` / `cargo build` all produce this file via `build.rs`; make
// sure you've run one of those at least once before the IDE tries to
// resolve `DescriptionParser` and friends.
#[allow(clippy::all, dead_code, unused_imports)]
pub mod hymeko {
    include!(concat!(env!("OUT_DIR"), "/hymeko.rs"));
}

use std::fs;
use std::io;
use std::path::Path;

use crate::ast::AstStr;
use crate::hymeko::{DescriptionParser, QueryVarParser};
use crate::lexer::simd::{CoreLexer, ScalarLexer};
#[cfg(target_arch = "x86_64")]
use crate::lexer::simd::{Avx2Lexer, Sse2Lexer};
use crate::lexer::{LexError, Token};

/// A tokenised `Spanned` item produced by any of the SIMD lexer tiers.
type SpannedLexItem<'a> = Result<(usize, Token<'a>, usize), LexError>;

/// A `ParseError` specialised to the parser's token and error types.
pub type ParseError<'a> = lalrpop_util::ParseError<usize, Token<'a>, LexError>;

// ---- Description (top-level .hymeko source) -------------------------------

/// Generic boundary — the compiler instantiates one optimized copy per lexer
/// tier (scalar / SSE2 / AVX2) so the SIMD dispatch stays monomorphic.
#[inline(always)]
fn parse_description_inner<'a, I>(iter: I) -> Result<AstStr<'a>, ParseError<'a>>
where
    I: Iterator<Item = SpannedLexItem<'a>>,
{
    DescriptionParser::new().parse(iter)
}

/// Parse a full `.hymeko` source into its AST, picking the best available
/// lexer tier at runtime (AVX2 → SSE2 → scalar fallback).
pub fn parse_description(input: &str) -> Result<AstStr<'_>, ParseError<'_>> {
    let core = CoreLexer::new(input);

    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("avx2") {
            return parse_description_inner(Avx2Lexer(core));
        }
        if std::is_x86_feature_detected!("sse2") {
            return parse_description_inner(Sse2Lexer(core));
        }
    }

    parse_description_inner(ScalarLexer(core))
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
