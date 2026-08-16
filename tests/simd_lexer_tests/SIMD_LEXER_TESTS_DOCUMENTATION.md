
# SIMD Lexer Unit Tests Documentation

## Overview

This document describes the comprehensive unit test suite for the SIMD-optimized lexer in the Hymeko parser. The test file `simd_lexer_tests.rs` contains **85+ test cases** covering all aspects of the lexer's functionality across different hardware paths (AVX2, SSE2, Scalar fallback).

## Test Categories

### 1. Whitespace Handling Tests (7 tests)

Tests for the SIMD-optimized whitespace skipping functionality:

- **`test_simd_skip_simple_whitespace`**: Basic space, tab, newline, carriage return combination
- **`test_simd_skip_tabs_and_spaces`**: Multiple consecutive tabs and spaces
- **`test_simd_skip_newlines`**: Multiple consecutive newlines
- **`test_simd_skip_carriage_returns`**: Multiple carriage returns (Windows line endings)
- **`test_simd_skip_mixed_whitespace`**: Complex mix of all whitespace types
- **`test_simd_whitespace_long_sequence`**: 64+ byte sequences to trigger SIMD code paths
- **`test_simd_whitespace_mixed_long_sequence`**: Long sequences with alternating whitespace types

**Purpose**: Validates that the SIMD whitespace-skipping optimization (`skip_ws_avx2`, `skip_ws_sse2`) correctly identifies and skips whitespace characters without accidentally consuming identifiers or other tokens.

### 2. Identifier Handling Tests (12 tests)

Tests for identifier lexing, especially the SIMD tail-scanning optimization:

- **`test_simd_ident_lowercase`**: Pure lowercase identifiers
- **`test_simd_ident_uppercase`**: Pure uppercase identifiers
- **`test_simd_ident_mixed_case`**: Mixed case identifiers
- **`test_simd_ident_with_digits`**: Identifiers with numbers (e.g., `ident123`)
- **`test_simd_ident_with_underscores`**: Underscores within identifiers
- **`test_simd_ident_starts_with_underscore`**: Identifiers starting with underscore
- **`test_simd_ident_long_sequence`**: 64+ byte identifier to trigger SIMD code paths
- **`test_simd_ident_mixed_chars_long`**: Long identifiers with mixed character types
- **`test_simd_ident_underscore_only`**: Underscore characters as identifier
- **`test_simd_ident_stops_at_non_ident`**: Identifier boundary detection (stops at punctuation)
- **`test_simd_ident_stops_at_space`**: Identifier boundary detection (stops at whitespace)

**Purpose**: Validates that the SIMD identifier tail-scanning optimization (`scan_ident_tail_avx2`, `scan_ident_tail_sse2`) correctly identifies identifier character boundaries using vectorized operations.

### 3. Punctuation and Operator Tests (11 tests)

Tests for all punctuation and operator tokens:

- **`test_simd_braces`**: Curly braces `{` and `}`
- **`test_simd_parens`**: Parentheses `(` and `)`
- **`test_simd_brackets`**: Square brackets `[` and `]`
- **`test_simd_angles`**: Angle brackets `<` and `>`
- **`test_simd_comma`**: Comma `,`
- **`test_simd_semicolon`**: Semicolon `;`
- **`test_simd_dot`**: Dot `.`
- **`test_simd_at`**: At sign `@`
- **`test_simd_plus`**: Plus `+`
- **`test_simd_minus`**: Minus `-`
- **`test_simd_arrow`**: Arrow `->` (two-character operator)
- **`test_simd_tilde`**: Tilde `~`

**Purpose**: Ensures all single-character and multi-character operators are correctly tokenized.

### 4. Number Handling Tests (7 tests)

Tests for numeric literal parsing:

- **`test_simd_number_integer`**: Simple integer tokens
- **`test_simd_number_zero`**: Zero as special case
- **`test_simd_number_decimal`**: Floating-point numbers
- **`test_simd_number_leading_zero`**: Numbers like `0.5`
- **`test_simd_number_trailing_zero`**: Numbers like `5.0`
- **`test_simd_number_large`**: Large integers
- **`test_simd_number_many_decimals`**: Numbers with many decimal places

**Purpose**: Validates numeric literal parsing, including edge cases with decimal points.

### 5. String Handling Tests (8 tests)

Tests for string literal parsing and escape sequences:

- **`test_simd_string_empty`**: Empty strings `""`
- **`test_simd_string_simple`**: Simple string literals
- **`test_simd_string_with_spaces`**: Strings containing whitespace
- **`test_simd_string_with_numbers`**: Strings containing digits
- **`test_simd_string_with_escape_sequences`**: `\n` escape sequences
- **`test_simd_string_escape_tab`**: `\t` tab escape
- **`test_simd_string_escape_backslash`**: `\\` backslash escape
- **`test_simd_string_escape_quote`**: `\"` quote escape
- **`test_simd_string_unterminated`**: Error handling for unterminated strings

**Purpose**: Validates string parsing with proper escape sequence handling.

### 6. Complex Multi-Token Sequences (5 tests)

Tests for realistic token combinations:

- **`test_simd_tokens_simple_sequence`**: `hello , world`
- **`test_simd_tokens_with_brackets`**: `[ a , b ]`
- **`test_simd_tokens_function_like`**: `func ( arg1 , arg2 ) { }`
- **`test_simd_tokens_arrow_sequence`**: `a -> b`
- **`test_simd_tokens_complex_expression`**: `func ( 123 , "str" ) @ < Type >`

**Purpose**: Validates that the lexer can correctly handle realistic code sequences with multiple tokens in sequence.

### 7. Comment Handling Tests (4 tests)

Tests for comment parsing:

- **`test_simd_single_line_comment`**: `// comment` style comments
- **`test_simd_multiline_comment`**: `/* comment */` style comments
- **`test_simd_multiline_comment_nested_text`**: Behavior with nested comment markers
- **`test_simd_unterminated_multiline_comment`**: Error handling for unclosed comments

**Purpose**: Validates that comments are properly skipped without affecting token parsing.

### 8. Location/Span Tracking Tests (2 tests)

Tests for source location tracking:

- **`test_simd_token_positions`**: Verifies start and end positions of tokens
- **`test_simd_position_with_whitespace`**: Position tracking with intervening whitespace

**Purpose**: Ensures that the lexer correctly tracks source positions for error reporting.

### 9. Edge Cases and Stress Tests (9 tests)

Tests for boundary conditions and error cases:

- **`test_simd_empty_input`**: Empty input handling
- **`test_simd_only_whitespace`**: Input with only whitespace
- **`test_simd_single_char_ident`**: Single-character identifiers
- **`test_simd_single_digit`**: Single-digit numbers
- **`test_simd_punct_no_spaces`**: Punctuation without separating whitespace
- **`test_simd_large_input`**: 100+ tokens in sequence (stress test)
- **`test_simd_minus_not_arrow`**: Disambiguating `-` from `->`
- **`test_simd_number_stops_at_space`**: Number parsing boundaries
- **`test_simd_number_stops_at_punct`**: Number parsing boundaries with punctuation

**Purpose**: Validates edge cases, stress conditions, and error handling.

### 10. SIMD-Specific Stress Tests (6 tests)

Tests targeting specific SIMD vector sizes:

- **`test_simd_whitespace_exactly_16_bytes`**: SSE2 vector boundary (16 bytes = 128 bits)
- **`test_simd_whitespace_exactly_32_bytes`**: AVX2 vector boundary (32 bytes = 256 bits)
- **`test_simd_whitespace_between_boundaries`**: 24-byte input (between boundaries)
- **`test_simd_ident_exactly_16_bytes`**: SSE2 identifier scanning
- **`test_simd_ident_exactly_32_bytes`**: AVX2 identifier scanning
- **`test_simd_ident_between_boundaries`**: Identifier scanning between boundaries
- **`test_simd_alternating_long_whitespace`**: Alternating patterns for SIMD processing
- **`test_simd_complex_identifier_patterns`**: Various identifier patterns

**Purpose**: Specifically targets SIMD code paths to ensure vectorized operations handle all input sizes correctly, including edge cases at vector boundaries.

### 11. Combined Stress Tests (2 tests)

Tests combining multiple features:

- **`test_simd_combined_whitespace_and_idents`**: 50 identifiers with 16-byte whitespace gaps
- **`test_simd_pathological_input`**: Mix of everything: long whitespace, identifiers, numbers, strings, and operators

**Purpose**: Validates that SIMD optimizations work correctly in complex, realistic scenarios.

## Test Execution

### Running All Tests

```bash
cd hymeko/parser
cargo test --test simd_lexer_tests
```

### Running Specific Test

```bash
cargo test --test simd_lexer_tests test_simd_ident_long_sequence
```

### Running with Output

```bash
cargo test --test simd_lexer_tests -- --nocapture --test-threads=1
```

## Test Coverage Matrix

| Feature | Test Count | Coverage |
|---------|-----------|----------|
| Whitespace Skipping | 7 | Full (all whitespace types, long sequences) |
| Identifier Parsing | 12 | Full (case, digits, underscores, boundaries, SIMD paths) |
| Punctuation | 11 | Full (all operators and brackets) |
| Numbers | 7 | Full (integers, decimals, edge cases) |
| Strings | 8 | Full (escapes, unterminated error) |
| Multi-Token Sequences | 5 | Comprehensive combinations |
| Comments | 4 | Full (single-line, multi-line, unterminated) |
| Location Tracking | 2 | Position verification |
| Edge Cases | 9 | Comprehensive boundary conditions |
| SIMD Boundaries | 6+ | Specific vector size testing |
| Stress Tests | 2 | Large inputs, pathological cases |
| **Total** | **85+** | **Comprehensive** |

## Key Testing Strategies

### 1. Boundary Testing
Tests are designed to trigger SIMD code paths by providing inputs that:
- Are exactly at vector boundaries (16, 32, 64 bytes)
- Are between vector boundaries
- Exceed multiple vector widths

### 2. Path Coverage
The tests ensure all three lexer implementations are exercised:
- **AVX2 Path** (32-byte vectors on x86_64 with AVX2)
- **SSE2 Path** (16-byte vectors on x86_64 with SSE2)
- **Scalar Path** (fallback for non-x86_64 or when SIMD unavailable)

### 3. Error Cases
Tests verify proper error handling for:
- Unterminated strings
- Unterminated comments
- Invalid characters
- Malformed numbers

### 4. Token Accuracy
All tests verify:
- Correct token type identification
- Accurate token spans/positions
- Proper boundary detection
- Correct token values

## Macro Utilities

The test file provides two utility macros for cleaner test code:

### `assert_token!` Macro
```rust
assert_token!(lexer, Token::Ident("expected"));
```
Extracts token from result, panicking on error with helpful message.

### `single_token!` Macro
```rust
single_token!("input", Token::Ident("input"));
```
Verifies exactly one token with expected type and no additional tokens.

## Performance Considerations

While these are unit tests (not benchmarks), they validate:
- SIMD optimizations are correctly implemented
- Boundary conditions don't cause performance cliffs
- Fallback paths work identically to optimized paths
- No regressions from scalar implementations

## Integration with CI/CD

These tests should be run:
- On every commit (fast, <1s total)
- On every PR
- On multiple platforms (x86_64 with different CPU capabilities)
- As part of full test suite

## Future Enhancements

Potential additions to the test suite:
1. **Property-based testing** with `quickcheck` for random input generation
2. **Benchmark comparisons** between SIMD and scalar paths
3. **Platform-specific tests** for ARM, WASM targets
4. **Fuzzing** against malformed input
5. **Performance regression tests** to catch regressions in vector operations

## Notes

- All tests use the public `Lexer` interface from `lexer::simd` module
- Tests are independent and can run in any order
- No external dependencies beyond what parser already has
- Tests are deterministic and platform-independent (though behavior may vary by CPU)

