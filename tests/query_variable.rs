//! Tests for the `?name` query-variable token — T10 in
//! `docs/plans/04_graph_query/T10_lalrpop_extension.md`.
//!
//! The `?` token and `QueryVar` grammar rule exist in
//! `parser/src/hymeko.lalrpop`; these tests exercise them via the
//! `parse_query_var` entry point in `parser/src/lib.rs`.
//!
//! Integration with the query/rewrite pattern interpreter is a future step;
//! for now this guards the grammar surface so it cannot silently regress.

use parser::parse_query_var;

#[test]
fn simple_query_variable_parses() {
    let name = parse_query_var("?x").expect("?x should parse");
    assert_eq!(name, "x");
}

#[test]
fn query_variable_with_underscore_suffix() {
    let name = parse_query_var("?link_name").expect("?link_name should parse");
    assert_eq!(name, "link_name");
}

#[test]
fn query_variable_with_mixed_case() {
    let name = parse_query_var("?MyVar").expect("?MyVar should parse");
    assert_eq!(name, "MyVar");
}

#[test]
fn bare_identifier_without_question_mark_rejected() {
    assert!(
        parse_query_var("x").is_err(),
        "`x` without `?` prefix must not be a valid query variable"
    );
}

#[test]
fn question_mark_without_identifier_rejected() {
    assert!(
        parse_query_var("?").is_err(),
        "lone `?` without a name must not parse"
    );
}

#[test]
fn whitespace_between_question_and_ident_is_tolerated() {
    // The lexer skips whitespace before tokens; this should parse.
    let name = parse_query_var("? spaced").expect("? spaced should parse");
    assert_eq!(name, "spaced");
}

#[test]
fn multiple_query_vars_not_accepted_by_single_parse() {
    // parse_query_var is single-shot. Two variables in sequence is not
    // accepted (the second `?y` is unexpected input). This guards the
    // entry-point contract — batch parsing needs its own rule.
    assert!(parse_query_var("?x ?y").is_err());
}

#[test]
fn query_variable_with_digits_in_name() {
    // Idents allow digits after the first char.
    let name = parse_query_var("?var123").expect("?var123 should parse");
    assert_eq!(name, "var123");
}

#[test]
fn query_variable_leading_digit_in_name_is_rejected() {
    // Leading digit is a number, not an ident.
    assert!(
        parse_query_var("?1var").is_err(),
        "leading-digit alias must not lex as an ident"
    );
}

#[test]
fn query_variable_with_leading_underscore() {
    // `_` is a valid ident-start character.
    let name = parse_query_var("?_hidden").expect("?_hidden should parse");
    assert_eq!(name, "_hidden");
}

#[test]
fn query_variable_with_comment_before_token() {
    // The lexer's skip_ws_and_comments handles line comments before `?`.
    let name = parse_query_var("// placeholder var\n?link").expect("comment + ?link");
    assert_eq!(name, "link");
}

#[test]
fn query_variable_with_block_comment_before_token() {
    let name = parse_query_var("/* placeholder */ ?joint").expect("block comment + ?joint");
    assert_eq!(name, "joint");
}

#[test]
fn query_variable_from_reserved_keyword_as_name_is_rejected() {
    // `using` and `as` are lexer keywords; they cannot bind as a variable.
    assert!(parse_query_var("?using").is_err());
    assert!(parse_query_var("?as").is_err());
}

#[test]
fn query_variable_single_char_is_accepted() {
    let name = parse_query_var("?q").expect("single-char var ?q should parse");
    assert_eq!(name, "q");
}

#[test]
fn query_variable_with_trailing_content_rejected() {
    // Extra tokens after the ident must terminate parsing cleanly with
    // an unexpected-token error — the parser does not silently truncate.
    assert!(parse_query_var("?x { extra }").is_err());
    assert!(parse_query_var("?x.y").is_err());
}
