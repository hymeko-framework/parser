#!cfg[(test)]
mod simd_lexer_tests {

    // ============================================================================
    // HELPER MACROS AND UTILITIES
    // ============================================================================

    use parser::lexer::Token;
    use parser::lexer::simd::{Avx2Lexer, CoreLexer, ScalarLexer, Sse2Lexer};

    /// Extract token from lexer result, panicking on error
    macro_rules! assert_token {
        ($lexer:expr, $expected:pat) => {
            match $lexer.next() {
                Some(Ok((_, $expected, _))) => {}
                Some(Ok((_, tok, _))) => panic!("Expected token pattern, got {:?}", tok),
                Some(Err(e)) => panic!("Lexer error: {}", e.msg),
                None => panic!("Expected token, got EOF"),
            }
        };
    }

    /// Assert string tokens by inspecting the inner Cow
    macro_rules! assert_token_str {
        ($lexer:expr, $expected:expr) => {
            match $lexer.next() {
                Some(Ok((_, Token::Str(ref s), _))) if s.as_ref() == $expected => {}
                Some(Ok((_, Token::Str(s), _))) => {
                    panic!("Expected string {:?}, got {:?}", $expected, s)
                }
                Some(Ok((_, tok, _))) => panic!("Expected Token::Str, got {:?}", tok),
                Some(Err(e)) => panic!("Lexer error: {}", e.msg),
                None => panic!("Expected token, got EOF"),
            }
        };
    }

    /// Assert we get exactly one token of the expected type
    macro_rules! single_token {
        ($input:expr, $expected:expr) => {{
            // Test AVX2 path
            #[cfg(target_arch = "x86_64")]
            {
                if std::is_x86_feature_detected!("avx2") {
                    let mut lexer = Avx2Lexer(CoreLexer::new($input));
                    assert_eq!(
                        lexer.next().map(|r| r.map(|(_, t, _)| t)),
                        Some(Ok($expected.clone()))
                    );
                    assert_eq!(lexer.next(), None, "AVX2: Expected only one token");
                }
            }

            // Test SSE2 path
            #[cfg(target_arch = "x86_64")]
            {
                if std::is_x86_feature_detected!("sse2") {
                    let mut lexer = Sse2Lexer(CoreLexer::new($input));
                    assert_eq!(
                        lexer.next().map(|r| r.map(|(_, t, _)| t)),
                        Some(Ok($expected.clone()))
                    );
                    assert_eq!(lexer.next(), None, "SSE2: Expected only one token");
                }
            }

            // Test Scalar fallback path (Always runs)
            {
                let mut lexer = ScalarLexer(CoreLexer::new($input));
                assert_eq!(
                    lexer.next().map(|r| r.map(|(_, t, _)| t)),
                    Some(Ok($expected.clone()))
                );
                assert_eq!(lexer.next(), None, "Scalar: Expected only one token");
            }
        }};
    }

    // ============================================================================
    // WHITESPACE HANDLING TESTS
    // ============================================================================

    fn run_complex_test<F>(input: &str, test_logic: F)
    where
        F: Fn(&mut dyn Iterator<Item = Result<(usize, Token<'_>, usize), parser::lexer::LexError>>),
    {
        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("avx2") {
            let mut lexer = Avx2Lexer(CoreLexer::new(input));
            test_logic(&mut lexer);
        }

        #[cfg(target_arch = "x86_64")]
        if std::is_x86_feature_detected!("sse2") {
            let mut lexer = Sse2Lexer(CoreLexer::new(input));
            test_logic(&mut lexer);
        }

        let mut lexer = ScalarLexer(CoreLexer::new(input));
        test_logic(&mut lexer);
    }

    #[test]
    fn test_simd_skip_simple_whitespace() {
        run_complex_test("  \t\n\r  ident", |lexer| match lexer.next() {
            Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "ident"),
            other => panic!("Expected ident, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_skip_tabs_and_spaces() {
        run_complex_test("    ident", |lexer| match lexer.next() {
            Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "ident"),
            other => panic!("Expected ident, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_skip_newlines() {
        run_complex_test("\n\n\nident", |lexer| match lexer.next() {
            Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "ident"),
            other => panic!("Expected ident, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_skip_carriage_returns() {
        run_complex_test("\r\r\rident", |lexer| match lexer.next() {
            Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "ident"),
            other => panic!("Expected ident, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_skip_mixed_whitespace() {
        run_complex_test(" \t\n\r \t\n\r ident", |lexer| match lexer.next() {
            Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "ident"),
            other => panic!("Expected ident, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_whitespace_long_sequence() {
        // Test SIMD path that processes 32+ bytes at once (AVX2)
        let long_spaces = " ".repeat(64);
        let input = format!("{}ident", long_spaces);
        run_complex_test(&input, |lexer| match lexer.next() {
            Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "ident"),
            other => panic!("Expected ident, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_whitespace_mixed_long_sequence() {
        // Test SIMD path with alternating whitespace
        let mut input = String::new();
        for _ in 0..16 {
            input.push_str(" \t\n\r");
        }
        input.push_str("ident");
        run_complex_test(&input, |lexer| match lexer.next() {
            Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "ident"),
            other => panic!("Expected ident, got {:?}", other),
        });
    }

    // ============================================================================
    // IDENTIFIER HANDLING TESTS
    // ============================================================================

    #[test]
    fn test_simd_ident_lowercase() {
        single_token!("hello", Token::Ident("hello"));
    }

    #[test]
    fn test_simd_ident_uppercase() {
        single_token!("HELLO", Token::Ident("HELLO"));
    }

    #[test]
    fn test_simd_ident_mixed_case() {
        single_token!("HeLLo", Token::Ident("HeLLo"));
    }

    #[test]
    fn test_simd_ident_with_digits() {
        single_token!("ident123", Token::Ident("ident123"));
    }

    #[test]
    fn test_simd_ident_with_underscores() {
        single_token!("ident_name_123", Token::Ident("ident_name_123"));
    }

    #[test]
    fn test_simd_ident_starts_with_underscore() {
        single_token!("_private", Token::Ident("_private"));
    }

    #[test]
    fn test_simd_ident_long_sequence() {
        // Test SIMD path for identifier scanning (32+ bytes)
        let long_ident = "a".repeat(64);
        single_token!(&long_ident, Token::Ident(&long_ident));
    }

    #[test]
    fn test_simd_ident_mixed_chars_long() {
        // Test SIMD path with mixed a-zA-Z0-9_
        let mut ident = String::new();
        for i in 0..20 {
            match i % 3 {
                0 => ident.push('a'),
                1 => ident.push('A'),
                _ => ident.push('1'),
            }
        }
        single_token!(&ident, Token::Ident(&ident));
    }

    #[test]
    fn test_simd_ident_underscore_only() {
        single_token!("__", Token::Ident("__"));
    }

    #[test]
    fn test_simd_ident_stops_at_non_ident() {
        run_complex_test("hello,world", |lexer| {
            match lexer.next() {
                Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "hello"),
                other => panic!("Expected 'hello', got {:?}", other),
            }
            match lexer.next() {
                Some(Ok((_, Token::Comma, _))) => {}
                other => panic!("Expected comma, got {:?}", other),
            }
        });
    }

    #[test]
    fn test_simd_ident_stops_at_space() {
        run_complex_test("hello world", |lexer| {
            match lexer.next() {
                Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "hello"),
                other => panic!("Expected 'hello', got {:?}", other),
            }
            match lexer.next() {
                Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "world"),
                other => panic!("Expected 'world', got {:?}", other),
            }
        });
    }

    // ============================================================================
    // PUNCTUATION AND OPERATOR TESTS
    // ============================================================================

    #[test]
    fn test_simd_braces() {
        single_token!("{", Token::LBrace);
        single_token!("}", Token::RBrace);
    }

    #[test]
    fn test_simd_parens() {
        single_token!("(", Token::LParen);
        single_token!(")", Token::RParen);
    }

    #[test]
    fn test_simd_brackets() {
        single_token!("[", Token::LBrack);
        single_token!("]", Token::RBrack);
    }

    #[test]
    fn test_simd_angles() {
        single_token!("<", Token::LAngle);
        single_token!(">", Token::RAngle);
    }

    #[test]
    fn test_simd_comma() {
        single_token!(",", Token::Comma);
    }

    #[test]
    fn test_simd_semicolon() {
        single_token!(";", Token::Semi);
    }

    #[test]
    fn test_simd_dot() {
        single_token!(".", Token::Dot);
    }

    #[test]
    fn test_simd_at() {
        single_token!("@", Token::At);
    }

    #[test]
    fn test_simd_plus() {
        single_token!("+", Token::Plus);
    }

    #[test]
    fn test_simd_minus() {
        single_token!("-", Token::Minus);
    }

    #[test]
    fn test_simd_tilde() {
        single_token!("~", Token::Tilde);
    }

    #[test]
    fn test_simd_arrow() {
        single_token!("->", Token::Arrow);
    }

    // ============================================================================
    // NUMBER HANDLING TESTS
    // ============================================================================

    #[test]
    fn test_simd_number_integer() {
        single_token!("123", Token::Number(123.0));
    }

    #[test]
    fn test_simd_number_zero() {
        single_token!("0", Token::Number(0.0));
    }

    #[test]
    fn test_simd_number_decimal() {
        single_token!("123.456", Token::Number(123.456));
    }

    #[test]
    fn test_simd_number_leading_zero() {
        single_token!("0.5", Token::Number(0.5));
    }

    #[test]
    fn test_simd_number_trailing_zero() {
        single_token!("5.0", Token::Number(5.0));
    }

    #[test]
    fn test_simd_number_large() {
        single_token!("999999999", Token::Number(999999999.0));
    }

    #[test]
    fn test_simd_number_many_decimals() {
        let pi_str = std::f64::consts::PI.to_string();
        single_token!(pi_str.as_str(), Token::Number(std::f64::consts::PI));
    }

    // ============================================================================
    // STRING HANDLING TESTS
    // ============================================================================

    #[test]
    fn test_simd_string_empty() {
        run_complex_test("\"\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, ""),
            other => panic!("Expected empty string, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_simple() {
        run_complex_test("\"hello\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, "hello"),
            other => panic!("Expected 'hello', got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_with_spaces() {
        run_complex_test("\"hello world\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, "hello world"),
            other => panic!("Expected 'hello world', got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_with_numbers() {
        run_complex_test("\"test123\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, "test123"),
            other => panic!("Expected 'test123', got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_with_escape_sequences() {
        run_complex_test("\"hello\\nworld\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, "hello\nworld"),
            other => panic!("Expected 'hello\\nworld', got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_escape_tab() {
        run_complex_test("\"hello\\tworld\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, "hello\tworld"),
            other => panic!("Expected escaped tab, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_escape_backslash() {
        run_complex_test("\"hello\\\\world\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, "hello\\world"),
            other => panic!("Expected escaped backslash, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_escape_quote() {
        run_complex_test("\"hello\\\"world\"", |lexer| match lexer.next() {
            Some(Ok((_, Token::Str(s), _))) => assert_eq!(s, "hello\"world"),
            other => panic!("Expected escaped quote, got {:?}", other),
        });
    }

    #[test]
    fn test_simd_string_unterminated() {
        run_complex_test("\"hello", |lexer| match lexer.next() {
            Some(Err(e)) => assert_eq!(e.msg, "Unterminated string literal"),
            other => panic!("Expected error, got {:?}", other),
        });
    }

    // ============================================================================
    // COMPLEX MULTI-TOKEN SEQUENCES
    // ============================================================================

    #[test]
    fn test_simd_tokens_simple_sequence() {
        run_complex_test("hello , world", |lexer| {
            match lexer.next() {
                Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "hello"),
                other => panic!("Expected 'hello', got {:?}", other),
            }

            match lexer.next() {
                Some(Ok((_, Token::Comma, _))) => {}
                other => panic!("Expected comma, got {:?}", other),
            }

            match lexer.next() {
                Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, "world"),
                other => panic!("Expected 'world', got {:?}", other),
            }

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_tokens_with_brackets() {
        run_complex_test("[ a , b ]", |lexer| {
            assert_token!(lexer, Token::LBrack);
            assert_token!(lexer, Token::Ident("a"));
            assert_token!(lexer, Token::Comma);
            assert_token!(lexer, Token::Ident("b"));
            assert_token!(lexer, Token::RBrack);

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_tokens_function_like() {
        run_complex_test("func ( arg1 , arg2 ) { }", |lexer| {
            assert_token!(lexer, Token::Ident("func"));
            assert_token!(lexer, Token::LParen);
            assert_token!(lexer, Token::Ident("arg1"));
            assert_token!(lexer, Token::Comma);
            assert_token!(lexer, Token::Ident("arg2"));
            assert_token!(lexer, Token::RParen);
            assert_token!(lexer, Token::LBrace);
            assert_token!(lexer, Token::RBrace);

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_tokens_arrow_sequence() {
        run_complex_test("a -> b", |lexer| {
            assert_token!(lexer, Token::Ident("a"));
            assert_token!(lexer, Token::Arrow);
            assert_token!(lexer, Token::Ident("b"));

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_tokens_complex_expression() {
        run_complex_test("func ( 123 , \"str\" ) @ < Type >", |lexer| {
            assert_token!(lexer, Token::Ident("func"));
            assert_token!(lexer, Token::LParen);
            assert_token!(lexer, Token::Number(123.0));
            assert_token!(lexer, Token::Comma);
            assert_token_str!(lexer, "str");
            assert_token!(lexer, Token::RParen);
            assert_token!(lexer, Token::At);
            assert_token!(lexer, Token::LAngle);
            assert_token!(lexer, Token::Ident("Type"));
            assert_token!(lexer, Token::RAngle);

            assert_eq!(lexer.next(), None);
        });
    }

    // ============================================================================
    // COMMENT HANDLING TESTS
    // ============================================================================

    #[test]
    fn test_simd_single_line_comment() {
        run_complex_test("hello // this is a comment\nworld", |lexer| {
            assert_token!(lexer, Token::Ident("hello"));
            assert_token!(lexer, Token::Ident("world"));

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_multiline_comment() {
        run_complex_test("hello /* comment */ world", |lexer| {
            assert_token!(lexer, Token::Ident("hello"));
            assert_token!(lexer, Token::Ident("world"));

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_multiline_comment_nested_text() {
        run_complex_test("a /* /* nested */ text */ b", |lexer| {
            assert_token!(lexer, Token::Ident("a"));
            // After first */, "text" is outside the comment
            assert_token!(lexer, Token::Ident("text"));

            // This behavior depends on implementation; adjust if different
        });
    }

    #[test]
    fn test_simd_unterminated_multiline_comment() {
        run_complex_test("hello /* unterminated", |lexer| {
            assert_token!(lexer, Token::Ident("hello"));

            match lexer.next() {
                Some(Err(e)) => assert_eq!(e.msg, "Unterminated multi-line comment"),
                other => panic!("Expected error, got {:?}", other),
            }
        });
    }

    // ============================================================================
    // LOCATION/SPAN TRACKING TESTS
    // ============================================================================

    #[test]
    fn test_simd_token_positions() {
        run_complex_test("a , b", |lexer| {
            match lexer.next() {
                Some(Ok((start, Token::Ident("a"), end))) => {
                    assert_eq!(start, 0);
                    assert_eq!(end, 1);
                }
                other => panic!("Expected positioned token, got {:?}", other),
            }

            match lexer.next() {
                Some(Ok((start, Token::Comma, end))) => {
                    assert_eq!(start, 2);
                    assert_eq!(end, 3);
                }
                other => panic!("Expected positioned token, got {:?}", other),
            }

            match lexer.next() {
                Some(Ok((start, Token::Ident("b"), end))) => {
                    assert_eq!(start, 4);
                    assert_eq!(end, 5);
                }
                other => panic!("Expected positioned token, got {:?}", other),
            }
        });
    }

    #[test]
    fn test_simd_position_with_whitespace() {
        run_complex_test("a    b", |lexer| {
            match lexer.next() {
                Some(Ok((_, Token::Ident("a"), end))) => assert_eq!(end, 1),
                other => panic!("Expected 'a', got {:?}", other),
            }

            match lexer.next() {
                Some(Ok((start, Token::Ident("b"), _))) => assert_eq!(start, 5),
                other => panic!("Expected 'b', got {:?}", other),
            }
        });
    }

    // ============================================================================
    // EDGE CASES AND STRESS TESTS
    // ============================================================================

    #[test]
    fn test_simd_empty_input() {
        run_complex_test("", |lexer| {
            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_only_whitespace() {
        run_complex_test("   \t\n\r   ", |lexer| {
            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_single_char_ident() {
        single_token!("a", Token::Ident("a"));
    }

    #[test]
    fn test_simd_single_digit() {
        single_token!("5", Token::Number(5.0));
    }

    #[test]
    fn test_simd_punct_no_spaces() {
        run_complex_test("(){},;", |lexer| {
            assert_token!(lexer, Token::LParen);
            assert_token!(lexer, Token::RParen);
            assert_token!(lexer, Token::LBrace);
            assert_token!(lexer, Token::RBrace);
            assert_token!(lexer, Token::Comma);
            assert_token!(lexer, Token::Semi);
            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_large_input() {
        let mut input = String::new();
        for i in 0..100 {
            input.push_str(&format!("token{} ", i));
        }

        run_complex_test(&input, |lexer| {
            for i in 0..100 {
                match lexer.next() {
                    Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, &format!("token{}", i)),
                    other => panic!("Expected token{}, got {:?}", i, other),
                }
            }
            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_minus_not_arrow() {
        run_complex_test("a - b", |lexer| {
            assert_token!(lexer, Token::Ident("a"));
            assert_token!(lexer, Token::Minus);
            assert_token!(lexer, Token::Ident("b"));

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_number_stops_at_space() {
        run_complex_test("123 hello", |lexer| {
            assert_token!(lexer, Token::Number(123.0));
            assert_token!(lexer, Token::Ident("hello"));

            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_number_stops_at_punct() {
        run_complex_test("123,456", |lexer| {
            assert_token!(lexer, Token::Number(123.0));
            assert_token!(lexer, Token::Comma);
            assert_token!(lexer, Token::Number(456.0));

            assert_eq!(lexer.next(), None);
        });
    }

    // ============================================================================
    // SIMD-SPECIFIC STRESS TESTS
    // ============================================================================

    #[test]
    fn test_simd_whitespace_exactly_16_bytes() {
        // Test SSE2 boundary (16 bytes)
        let input = format!("{}ident", " ".repeat(16));
        single_token!(&input, Token::Ident("ident"));
    }

    #[test]
    fn test_simd_whitespace_exactly_32_bytes() {
        // Test AVX2 boundary (32 bytes)
        let input = format!("{}ident", " ".repeat(32));
        single_token!(&input, Token::Ident("ident"));
    }

    #[test]
    fn test_simd_whitespace_between_boundaries() {
        // Test between 16 and 32 bytes
        let input = format!("{}ident", " ".repeat(24));
        single_token!(&input, Token::Ident("ident"));
    }

    #[test]
    fn test_simd_ident_exactly_16_bytes() {
        // Test SSE2 boundary for identifier scanning
        let ident = "a".repeat(16);
        single_token!(&ident, Token::Ident(&ident));
    }

    #[test]
    fn test_simd_ident_exactly_32_bytes() {
        // Test AVX2 boundary for identifier scanning
        let ident = "a".repeat(32);
        single_token!(&ident, Token::Ident(&ident));
    }

    #[test]
    fn test_simd_ident_between_boundaries() {
        // Test between 16 and 32 bytes for identifier
        let ident = "a".repeat(24);
        single_token!(&ident, Token::Ident(&ident));
    }

    #[test]
    fn test_simd_alternating_long_whitespace() {
        // Alternating pattern throughout SIMD chunk
        let input = format!("{}ident", " \t ".repeat(16));
        single_token!(&input, Token::Ident("ident"));
    }

    #[test]
    fn test_simd_complex_identifier_patterns() {
        // Test various identifier patterns that trigger SIMD codepaths
        let test_cases = vec![
            "abcdefghijklmnop",      // 16 lowercase
            "ABCDEFGHIJKLMNOP",      // 16 uppercase
            "aBcDeFgHiJkLmNoP",      // 16 mixed
            "a1b2c3d4e5f6g7h8",      // 16 with numbers
            "a_b_c_d_e_f_g_h_i_j",   // With underscores
            "_1_2_3_4_5_6_7_8_9_10", // Starting with underscore
        ];

        for test_case in test_cases {
            single_token!(test_case, Token::Ident(test_case));
        }
    }

    // ============================================================================
    // COMBINED STRESS TESTS
    // ============================================================================

    #[test]
    fn test_simd_combined_whitespace_and_idents() {
        let mut input = String::new();
        for i in 0..50 {
            input.push_str(&format!("{}id{}", " ".repeat(16), i));
        }

        run_complex_test(&input, |lexer| {
            for i in 0..50 {
                match lexer.next() {
                    Some(Ok((_, Token::Ident(s), _))) => assert_eq!(s, &format!("id{}", i)),
                    other => panic!("Expected id{}, got {:?}", i, other),
                }
            }
            assert_eq!(lexer.next(), None);
        });
    }

    #[test]
    fn test_simd_pathological_input() {
        // Mix of everything: long whitespace, identifiers, numbers, punctuation
        let input = format!(
            "{0}a {1}123 {2},{3}\"test\"{1}->{2}{4}<>",
            " ".repeat(32),
            " ".repeat(16),
            " ".repeat(20),
            " ".repeat(24),
            " ".repeat(16)
        );

        run_complex_test(&input, |lexer| {
            assert_token!(lexer, Token::Ident("a"));
            assert_token!(lexer, Token::Number(123.0));
            assert_token!(lexer, Token::Comma);
            assert_token_str!(lexer, "test");
            assert_token!(lexer, Token::Arrow);
            assert_token!(lexer, Token::LAngle);
            assert_token!(lexer, Token::RAngle);
            assert_eq!(lexer.next(), None);
        });
    }
}
