# parser

The HyMeKo source parser: a LALRPOP grammar for `.hymeko` model descriptions
over a lexer that picks AVX2, SSE2 or a scalar path at runtime. It produces a
borrowed syntax tree. Module loading, name resolution and the canonical
intermediate representation live in [core](https://github.com/hymeko-framework/core).

## Use

```toml
[dependencies]
parser = { git = "https://github.com/hymeko-framework/parser.git" }
```

```rust
use parser::{builder::AstBuilder, parse_description_with};

let ast = parse_description_with("Demo{} root;", &mut AstBuilder).unwrap();
assert_eq!(ast.items.len(), 1);
```

Entry points:

- `parse_description`: a full `.hymeko` source into the syntax tree.
- `parse_description_with` / `parse_description_into`: the same grammar with
  a caller-supplied output or incremental builder (`builder` module).
- `parse_query_var`: a single `?name` query-variable fragment.
- `read_source_file`: load a source file from disk.

The returned tree borrows from the input, so the source text must outlive it.

## Build and test

```sh
cargo test
cargo bench --bench parser_bench
```

`build.rs` generates the grammar modules with LALRPOP during the build.
`format_bench` compares HyMeKo with JSON, XML and YAML encodings of the same
generated models. It reads its fixture directory and pattern from
`FORMAT_FIXTURES` and `FORMAT_PATTERN`; `scripts/format_campaign.py` sets them
and `scripts/format_report.py` summarises the results.

## License

MIT; see [LICENSE](LICENSE).
