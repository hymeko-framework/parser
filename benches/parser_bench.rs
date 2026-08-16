// parser/benches/parser_bench.rs
//
#![allow(dead_code)]
// Generators and bench groups are wired through Criterion macros; `cargo clippy
// --all-targets` still type-checks the full module graph.
//
// Standalone parser benchmarks measuring:
// 1. Lexer throughput per SIMD tier (AVX2 vs SSE2 vs Scalar)
// 2. Full parse throughput (lexer + LALRPOP AST construction)
// 3. Input pattern sensitivity (dense, whitespace-heavy, comment-heavy, long idents)
// 4. Scaling behavior at 100 → 50,000 nodes
//
// Run:  cargo bench -p parser
//
// IMPORTANT: The "whitespace_heavy" benchmark will expose whether SIMD skip_ws
// is active. If AVX2 and Scalar show identical throughput on whitespace_heavy
// input but different throughput on long_idents, it means skip_ws SIMD is dead
// and only scan_ident_tail SIMD is active.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

use parser::lexer::simd::{Avx2Lexer, CoreLexer, ScalarLexer, Sse2Lexer};
use parser::parse_description;

// ============================================================
// Input generators
// ============================================================

/// Dense hypergraph: many nodes with edges and signed arcs.
/// Identifier-heavy — tests scan_ident_tail SIMD.
fn gen_dense(nodes: usize) -> String {
    let mut s = String::with_capacity(nodes * 120);
    s.push_str("BenchDesc {} BenchGraph {\n");
    for i in 0..nodes {
        let next = (i + 1) % nodes;
        s.push_str(&format!(
            "  node_{i}: base_type {{\n\
             \x20\x20\x20\x20mass {i}.0;\n\
             \x20\x20\x20\x20@edge_{i} {{ (+node_{i}, [{i}.5] -node_{next}); }}\n\
             \x20\x20}}\n"
        ));
    }
    s.push('}');
    s
}

/// Whitespace-heavy: deep indentation + blank lines between nodes.
/// This DIRECTLY tests skip_ws throughput.
fn gen_whitespace_heavy(nodes: usize) -> String {
    let mut s = String::with_capacity(nodes * 200);
    s.push_str("BenchDesc {} BenchGraph {\n");
    let indent = " ".repeat(32); // 32-space indentation (one full AVX2 register)
    for i in 0..nodes {
        s.push_str("\n\n\n\n"); // 4 blank lines
        s.push_str(&indent);
        s.push_str(&format!("node_{i} {{ mass {i}.0; }}\n"));
    }
    s.push('}');
    s
}

/// Comment-heavy: alternating comment lines and node declarations.
fn gen_comment_heavy(nodes: usize) -> String {
    let mut s = String::with_capacity(nodes * 100);
    s.push_str("BenchDesc {} BenchGraph {\n");
    for i in 0..nodes {
        s.push_str(&format!("  // Node {i}: properties and connections\n"));
        s.push_str(&format!("  node_{i} {{ mass {i}.0; }}\n"));
    }
    s.push('}');
    s
}

/// Long identifiers: 60+ character names (stress scan_ident_tail SIMD).
fn gen_long_idents(nodes: usize) -> String {
    let mut s = String::with_capacity(nodes * 150);
    s.push_str("BenchDesc {} BenchGraph {\n");
    for i in 0..nodes {
        let name = format!("very_long_identifier_name_for_benchmark_node_number_{i:06}");
        s.push_str(&format!("  {name} {{ value {i}.0; }}\n"));
    }
    s.push('}');
    s
}

/// Robot-like: resembles a real kinematic description with signed refs + weights.
fn gen_robot_like(joints: usize) -> String {
    let mut s = String::with_capacity(joints * 200);
    s.push_str("RobotBench {} robot {\n  base_link: link { mass 25.0; }\n");
    for i in 0..joints {
        s.push_str(&format!(
            "  link_{i}: link {{ mass {}.0; }}\n\
             \x20\x20@joint_{i}: rev_joint {{\n\
             \x20\x20\x20\x20+base_link, [[0.{i}, 0.0, 0.05], [-90.0, 0.0, 0.0]] -link_{i},\n\
             \x20\x20\x20\x20-AXIS_Z\n\
             \x20\x20}}\n",
            i + 1,
        ));
    }
    s.push('}');
    s
}

// ============================================================
// Group 1: Lexer tier comparison (same input, different SIMD)
// ============================================================

fn bench_lexer_tiers(c: &mut Criterion) {
    let input = gen_dense(5_000);
    let mut group = c.benchmark_group("lexer_tiers");
    group.throughput(Throughput::Bytes(input.len() as u64));

    group.bench_function("scalar", |b| {
        b.iter(|| {
            let lex = ScalarLexer(CoreLexer::new(black_box(&input)));
            for tok in lex {
                let _ = black_box(tok);
            }
        })
    });

    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("sse2") {
            group.bench_function("sse2", |b| {
                b.iter(|| {
                    let lex = Sse2Lexer(CoreLexer::new(black_box(&input)));
                    for tok in lex {
                        let _ = black_box(tok);
                    }
                })
            });
        }
        if std::is_x86_feature_detected!("avx2") {
            group.bench_function("avx2", |b| {
                b.iter(|| {
                    let lex = Avx2Lexer(CoreLexer::new(black_box(&input)));
                    for tok in lex {
                        let _ = black_box(tok);
                    }
                })
            });
        }
    }

    group.finish();
}

// ============================================================
// Group 2: Input pattern sensitivity
// ============================================================

fn bench_input_patterns(c: &mut Criterion) {
    let n = 2_000;
    let patterns: Vec<(&str, String)> = vec![
        ("dense", gen_dense(n)),
        ("whitespace_heavy", gen_whitespace_heavy(n)),
        ("comment_heavy", gen_comment_heavy(n)),
        ("long_idents", gen_long_idents(n)),
        ("robot_like", gen_robot_like(n)),
    ];

    let mut group = c.benchmark_group("input_patterns");

    for (name, input) in &patterns {
        group.throughput(Throughput::Bytes(input.len() as u64));

        // Lex only (no AST)
        group.bench_with_input(BenchmarkId::new("lex", name), input, |b, input| {
            b.iter(|| {
                let core = CoreLexer::new(black_box(input));
                #[cfg(target_arch = "x86_64")]
                {
                    if std::is_x86_feature_detected!("avx2") {
                        for tok in Avx2Lexer(core) {
                            let _ = black_box(tok);
                        }
                        return;
                    }
                }
                for tok in ScalarLexer(core) {
                    let _ = black_box(tok);
                }
            })
        });

        // Full parse (lex + AST)
        group.bench_with_input(BenchmarkId::new("parse", name), input, |b, input| {
            b.iter(|| {
                let _ = black_box(parse_description(black_box(input)));
            })
        });
    }

    group.finish();
}

// ============================================================
// Group 3: Scaling (parse throughput at increasing sizes)
// ============================================================

fn bench_scaling(c: &mut Criterion) {
    let sizes = [100, 500, 1_000, 5_000, 10_000, 50_000];
    let mut group = c.benchmark_group("parse_scaling");

    for &n in &sizes {
        let input = gen_dense(n);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &input, |b, input| {
            b.iter(|| {
                let _ = black_box(parse_description(black_box(input)));
            })
        });
    }

    group.finish();
}

// ============================================================
// Group 4: Micro-benchmarks (isolated operations)
// ============================================================

fn bench_micro(c: &mut Criterion) {
    let mut group = c.benchmark_group("micro");

    // Pure whitespace skipping: 1MB of spaces
    let ws_input = format!(
        "BenchDesc {{}} Bench {{\n{}\nnode_0 {{}}\n}}",
        " ".repeat(1_000_000)
    );
    group.throughput(Throughput::Bytes(ws_input.len() as u64));
    group.bench_function("skip_1mb_whitespace", |b| {
        b.iter(|| {
            let _ = black_box(parse_description(black_box(&ws_input)));
        })
    });

    // Pure identifier scanning: one very long identifier
    let long_id = format!("BenchDesc {{}} Bench {{ {} 42.0; }}", "a".repeat(100_000));
    group.throughput(Throughput::Bytes(long_id.len() as u64));
    group.bench_function("scan_100k_ident", |b| {
        b.iter(|| {
            let _ = black_box(parse_description(black_box(&long_id)));
        })
    });

    // Many small tokens: minimal identifiers, lots of punctuation
    let many_tokens: String = {
        let mut s = String::from("BenchDesc {} Bench {\n");
        for i in 0..10_000 {
            s.push_str(&format!("a{i};\n"));
        }
        s.push('}');
        s
    };
    group.throughput(Throughput::Bytes(many_tokens.len() as u64));
    group.bench_function("many_small_tokens", |b| {
        b.iter(|| {
            let _ = black_box(parse_description(black_box(&many_tokens)));
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_lexer_tiers,
    bench_input_patterns,
    bench_scaling,
    bench_micro,
);
criterion_main!(benches);
