//! Parser-level tests for the `using <path> as <alias>;` namespace-alias syntax.
//!
//! Guards the `UsingStmt` grammar rule in `parser/src/hymeko.lalrpop` so that
//! future grammar changes cannot silently drop or rewrite the alias header.
//! Complements the end-to-end alias-parity tests in
//! `hymeko_query/tests/test_transform_ecosystem.rs::alias_parity`.

use parser::parse_description;

#[test]
fn single_using_alias_is_captured_in_ast() {
    let src = r#"
        demo {
          using kinematics.elements as el;
        }

        robot: el {}
    "#;

    let ast = parse_description(src).expect("parse_description should succeed");
    assert_eq!(ast.usings.len(), 1, "exactly one UsingStmt expected");
    let u = &ast.usings[0];
    assert_eq!(u.alias, "el");
    assert_eq!(u.path.path, vec!["kinematics", "elements"]);
}

#[test]
fn multiple_using_aliases_are_captured_in_order() {
    let src = r#"
        multi {
          using kinematics.elements as el;
          using kinematics.geometry as geo;
          using kinematics.axes as ax;
        }

        robot: el, geo, ax {}
    "#;

    let ast = parse_description(src).expect("parse_description should succeed");
    assert_eq!(ast.usings.len(), 3);

    let aliases: Vec<_> = ast.usings.iter().map(|u| u.alias).collect();
    assert_eq!(aliases, vec!["el", "geo", "ax"]);

    let paths: Vec<Vec<_>> = ast.usings.iter()
        .map(|u| u.path.path.clone())
        .collect();
    assert_eq!(
        paths,
        vec![
            vec!["kinematics", "elements"],
            vec!["kinematics", "geometry"],
            vec!["kinematics", "axes"],
        ]
    );
}

#[test]
fn using_alias_coexists_with_imports() {
    let src = r#"
        mixed {
          @"meta_kinematics.hymeko";
          using kinematics.elements as el;
        }

        robot: el {}
    "#;

    let ast = parse_description(src).expect("parse_description should succeed");
    assert_eq!(ast.imports.len(), 1, "import survived");
    assert_eq!(ast.usings.len(), 1, "using survived");
    assert_eq!(ast.usings[0].alias, "el");
}

#[test]
fn aliased_reference_is_usable_in_node_inheritance() {
    let src = r#"
        inherit_demo {
          using kinematics.elements as el;
        }

        robot: el {
          base: el.link {}
        }
    "#;

    let ast = parse_description(src).expect("aliased inheritance should parse");
    // Confirms the grammar accepts alias-qualified paths where a full
    // namespace reference would otherwise appear (node bases, item types).
    assert_eq!(ast.usings.len(), 1);
}

#[test]
fn using_without_semicolon_is_rejected() {
    let src = r#"
        bad {
          using kinematics.elements as el
        }

        robot: el {}
    "#;
    assert!(
        parse_description(src).is_err(),
        "missing `;` after using-alias must not parse"
    );
}

#[test]
fn using_without_as_clause_is_rejected() {
    let src = r#"
        bad {
          using kinematics.elements;
        }

        robot: el {}
    "#;
    assert!(
        parse_description(src).is_err(),
        "using-without-as must not parse"
    );
}

#[test]
fn using_without_trailing_ident_is_rejected() {
    let src = r#"
        bad {
          using kinematics.elements as ;
        }

        robot: el {}
    "#;
    assert!(
        parse_description(src).is_err(),
        "using with empty alias must not parse"
    );
}

#[test]
fn single_segment_alias_target_is_captured() {
    // Path may be a single identifier — grammar allows it even if the
    // resolution layer might complain later.
    let src = r#"
        single {
          using kinematics as k;
        }

        robot: k {}
    "#;
    let ast = parse_description(src).expect("single-segment alias should parse");
    assert_eq!(ast.usings.len(), 1);
    assert_eq!(ast.usings[0].alias, "k");
    assert_eq!(ast.usings[0].path.path, vec!["kinematics"]);
}

#[test]
fn deep_path_alias_is_captured() {
    let src = r#"
        deep {
          using a.b.c.d.e as e;
        }

        robot: e {}
    "#;
    let ast = parse_description(src).expect("5-segment alias should parse");
    assert_eq!(ast.usings[0].path.path, vec!["a", "b", "c", "d", "e"]);
}

#[test]
fn alias_matching_reserved_word_is_rejected() {
    // `as` and `using` are lexer-keyword tokens, not idents, so re-using
    // them as the alias name must not lex as an Ident.
    let src_as = r#"
        bad {
          using kinematics as as;
        }

        robot: el {}
    "#;
    assert!(
        parse_description(src_as).is_err(),
        "alias name `as` (reserved keyword) must not parse"
    );

    let src_using = r#"
        bad {
          using kinematics as using;
        }

        robot: el {}
    "#;
    assert!(
        parse_description(src_using).is_err(),
        "alias name `using` (reserved keyword) must not parse"
    );
}

#[test]
fn alias_declarations_with_comments_between_parse() {
    let src = r#"
        commented {
          // pull in the shared element catalog
          using kinematics.elements as el;
          /* and the geometry primitives */
          using kinematics.geometry as geo;
        }

        robot: el, geo {}
    "#;
    let ast = parse_description(src).expect("using + comments should parse");
    assert_eq!(ast.usings.len(), 2);
    assert_eq!(ast.usings[0].alias, "el");
    assert_eq!(ast.usings[1].alias, "geo");
}
