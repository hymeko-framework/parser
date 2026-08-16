# SIMD Lexer Unit Tests - Quick Reference

## Quick Start

```bash
# Run all SIMD lexer tests
cd hymeko/parser
cargo test --test simd_lexer_tests

# Run specific test
cargo test --test simd_lexer_tests test_simd_ident_long_sequence

# Run all whitespace tests
cargo test --test simd_lexer_tests test_simd_skip

# Show test output
cargo test --test simd_lexer_tests -- --nocapture
```

## Test Organization

### By Feature
- **Whitespace**: `test_simd_skip*` (7 tests)
- **Identifiers**: `test_simd_ident*` (12 tests)
- **Operators**: `test_simd_*` in punctuation section (11 tests)
- **Numbers**: `test_simd_number*` (7 tests)
- **Strings**: `test_simd_string*` (8+ tests)
- **Comments**: `test_simd_*comment*` (4 tests)
- **SIMD Specific**: `test_simd_*boundary*` or `test_simd_*16*` or `test_simd_*32*` (6+ tests)

### By Complexity
- **Basic**: Single token tests (`test_simd_ident_lowercase`, `test_simd_number_zero`)
- **Intermediate**: Multi-token sequences (`test_simd_tokens_simple_sequence`)
- **Advanced**: Stress tests (`test_simd_combined_whitespace_and_idents`, `test_simd_pathological_input`)

## Understanding Test Names

```
test_simd_[feature]_[scenario]

Examples:
- test_simd_skip_simple_whitespace      → Whitespace, basic
- test_simd_ident_long_sequence          → Identifier, SIMD path (32+ bytes)
- test_simd_number_stops_at_punct        → Numbers, boundary condition
- test_simd_whitespace_exactly_16_bytes  → Whitespace, SSE2 boundary
- test_simd_tokens_complex_expression    → Multi-token, realistic
```

## Common Test Patterns

### Pattern 1: Single Token Verification
```rust
single_token!("hello", Token::Ident("hello"));
```

### Pattern 2: Multi-Token Sequence
```rust
let mut lexer = Lexer::new("hello , world");
assert_token!(lexer, Token::Ident("hello"));
assert_token!(lexer, Token::Comma);
assert_token!(lexer, Token::Ident("world"));
assert_eq!(lexer.next(), None);
```

### Pattern 3: Error Case
```rust
match Lexer::new("\"unterminated").next() {
    Some(Err(e)) => assert_eq!(e.msg, "Unterminated string literal"),
    other => panic!("Expected error, got {:?}", other),
}
```

### Pattern 4: Position/Span Tracking
```rust
match lexer.next() {
    Some(Ok((start, Token::Ident(s), end))) => {
        assert_eq!(s, "hello");
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }
    _ => panic!("Unexpected"),
}
```

## SIMD Path Targeting

### To trigger AVX2 (32-byte vectors):
- Use identifiers with 32+ characters
- Use 32+ spaces before next token
- Tests with `exactly_32` in name

### To trigger SSE2 (16-byte vectors):
- Use identifiers with 16+ characters
- Use 16+ spaces before next token
- Tests with `exactly_16` in name or `_16_` patterns

### To test scalar fallback:
- All tests work on scalar path
- Run on non-x86_64 systems
- Tests with boundaries test all paths

## Debugging Failed Tests

### If a test fails:

1. **Check the error message** - Usually quite descriptive
2. **Run with output**: 
   ```bash
   cargo test --test simd_lexer_tests test_name -- --nocapture
   ```
3. **Run single-threaded** for clearer output:
   ```bash
   cargo test --test simd_lexer_tests test_name -- --test-threads=1
   ```
4. **Check related tests** - See if category has issues

### Common issues:

| Issue | Solution |
|-------|----------|
| "Expected token, got EOF" | Token not recognized, check syntax |
| "Lexer error: Unexpected char" | Invalid character in input |
| Position mismatch | Whitespace counting issue |
| Identifier too short | Boundary condition error |

## Test Statistics

```
Total Tests:        85+
Passing:            All (target)
Categories:         11
SIMD-specific:      6+
Stress tests:       2
Lines of code:      1000+
Execution time:     < 1 second (typical)
```

## Key Test Cases by Importance

| Priority | Test | Why Important |
|----------|------|---------------|
| Critical | `test_simd_ident_long_sequence` | Tests AVX2 path |
| Critical | `test_simd_whitespace_long_sequence` | Tests AVX2 optimization |
| High | `test_simd_tokens_complex_expression` | Realistic scenario |
| High | `test_simd_string_with_escape_sequences` | Common feature |
| Medium | `test_simd_unterminated_multiline_comment` | Error handling |
| Medium | `test_simd_large_input` | Stress/performance |

## Extending the Test Suite

To add new tests:

1. **Identify the category** (whitespace, identifier, etc.)
2. **Find the section** in `simd_lexer_tests.rs`
3. **Add test function**:
   ```rust
   #[test]
   fn test_simd_category_scenario() {
       // Your test code here
   }
   ```
4. **Run it**: `cargo test --test simd_lexer_tests test_simd_category_scenario`
5. **Verify it passes** before committing

## Integration with CI/CD

These tests are designed to:
- Run fast (< 1 second total)
- Be platform-independent (but behavior varies by CPU)
- Require no external dependencies
- Run in parallel safely
- Provide clear error messages

## Files Modified

```
hymeko/parser/tests/SIMD_LEXER_TESTS/
├── simd_lexer_tests.rs                      [Main test file, 752 lines]
└── Documentation files (4 total)
```

## Related Code

- **Implementation**: `hymeko/parser/src/lexer/simd.rs`
- **Common Lexer**: `hymeko/parser/src/lexer/common.rs`
- **Token Types**: `hymeko/parser/src/lexer/token.rs`
- **Module Root**: `hymeko/parser/src/lexer/mod.rs`

## Performance Notes

While these are functional tests:
- They validate SIMD optimizations work correctly
- They ensure no performance regressions
- They can be used as baselines for benchmarks

## Support

For issues or questions about the tests:
1. Check the full documentation: `SIMD_LEXER_TESTS_DOCUMENTATION.md`
2. Review the test implementation in `simd_lexer_tests.rs`
3. Consult the lexer implementation for expected behavior
4. Run tests in isolation to debug specific issues

---

**Last Updated**: 2026-02-21
**Test Suite Version**: 1.0

