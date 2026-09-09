mod formats;

use criterion::{BatchSize, Criterion, SamplingMode, Throughput};
use formats::Fixture;
use formats::adapters::Format;
use formats::model::{Dataset, Pattern};
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let directory =
        PathBuf::from(std::env::var("FORMAT_FIXTURES").expect("FORMAT_FIXTURES required"));
    let pattern =
        Pattern::parse(&std::env::var("FORMAT_PATTERN").expect("FORMAT_PATTERN required")).unwrap();
    let nodes: usize = std::env::var("FORMAT_NODES").unwrap().parse().unwrap();
    assert!(nodes <= 50_000);
    if std::env::var("FORMAT_MODE").as_deref() == Ok("prepare") {
        let fixtures = Fixture::prepare(&directory, pattern, nodes).unwrap();
        let path = directory.join(format!("{}-{nodes}.manifest.json", pattern.name()));
        std::fs::write(path, serde_json::to_vec_pretty(&fixtures).unwrap()).unwrap();
        println!("PREPARED {} {nodes}: four verified formats", pattern.name());
        return;
    }
    let format = Format::parse_name(&std::env::var("FORMAT_FORMAT").unwrap()).unwrap();
    let phase = std::env::var("FORMAT_PHASE").unwrap();
    let manifest: Vec<Fixture> = serde_json::from_slice(
        &std::fs::read(directory.join(format!("{}-{nodes}.manifest.json", pattern.name())))
            .unwrap(),
    )
    .unwrap();
    let fixture = manifest.iter().find(|f| f.format == format.name()).unwrap();
    let source = std::fs::read_to_string(directory.join(&fixture.file)).unwrap();
    assert_eq!(
        blake3::hash(source.as_bytes()).to_hex().as_str(),
        fixture.blake3
    );
    eprintln!("FIXTURE {}", serde_json::to_string(fixture).unwrap());
    let data: Option<Dataset> = if phase == "serialize" {
        let json =
            std::fs::read(directory.join(format!("{}-{nodes}.json", pattern.name()))).unwrap();
        assert_eq!(
            blake3::hash(&json).to_hex().as_str(),
            fixture.semantic_blake3
        );
        Some(serde_json::from_slice(&json).unwrap())
    } else {
        None
    };
    let mut criterion = Criterion::default().configure_from_args();
    let mut group = criterion.benchmark_group(format!(
        "formats/{}/{nodes}/{}",
        pattern.name(),
        format.name()
    ));
    group
        .sample_size(10)
        .warm_up_time(Duration::from_millis(200))
        .measurement_time(Duration::from_millis(500))
        .sampling_mode(SamplingMode::Flat);
    group.throughput(Throughput::Bytes(fixture.bytes as u64));
    match phase.as_str() {
        "serialize" => group.bench_function(&phase, |b| {
            b.iter_batched(
                || (),
                |()| format.encode(black_box(data.as_ref().unwrap())).unwrap(),
                BatchSize::PerIteration,
            )
        }),
        "parse_document" => group.bench_function(&phase, |b| {
            b.iter_batched(
                || (),
                |()| format.document(black_box(&source)).unwrap(),
                BatchSize::PerIteration,
            )
        }),
        "decode_records" => group.bench_function(&phase, |b| {
            b.iter_batched(
                || (),
                |()| format.decode(black_box(&source)).unwrap(),
                BatchSize::PerIteration,
            )
        }),
        "consume_ast" => {
            assert!(matches!(format, Format::Hymeko));
            // Parsing is setup; conversion includes destruction of consumed AST nodes.
            group.bench_function(&phase, |b| {
                b.iter_batched(
                    || parser::parse_description(&source).unwrap(),
                    |ast| Dataset::from_hymeko(black_box(ast)).unwrap(),
                    BatchSize::PerIteration,
                )
            })
        }
        "drop_ast" => {
            assert!(matches!(format, Format::Hymeko));
            group.bench_function(&phase, |b| {
                b.iter_batched(
                    || parser::parse_description(&source).unwrap(),
                    |ast| drop(black_box(ast)),
                    BatchSize::PerIteration,
                )
            })
        }
        _ => panic!("unknown phase {phase}"),
    };
    group.finish();
    criterion.final_summary();
}
