//! How the time to outline one file grows with its length, per language. Each case
//! generates a file of small items at three lengths, checks that the outline finds every
//! item before it is timed, and times `outline` alone, the parse included. Throughput is
//! counted in lines, so a case that scales linearly keeps its lines per second as the
//! file grows and a case that scales worse loses them.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use excerpt::item::outline;
use excerpt::language::Language;

const ITEM_COUNTS: [usize; 3] = [100, 1_000, 10_000];
const SAMPLE_COUNT: usize = 10;
const WARM_UP_SECONDS: u64 = 1;
const MEASUREMENT_SECONDS: u64 = 3;

/// mimalloc on musl, the allocator each case's parse is timed under there.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// A language, and one container of `count` small items in it, which outline to
/// `count` + 1 items.
struct Case {
    name: &'static str,
    language: Language,
    generate: fn(usize) -> String,
}

const CASES: [Case; 2] = [
    Case {
        name: "rust",
        language: Language::Rust,
        generate: rust_items,
    },
    Case {
        name: "typescript",
        language: Language::TypeScript,
        generate: typescript_items,
    },
];

fn outline_by_length(criterion: &mut Criterion) {
    for case in &CASES {
        let mut group = criterion.benchmark_group(case.name);
        for count in ITEM_COUNTS {
            let text = (case.generate)(count);
            let found = outline(case.language, &text);
            let Some(expected) = count.checked_add(1) else {
                unreachable!("an item count is below usize::MAX");
            };
            assert_eq!(
                found.items.len(),
                expected,
                "{} outlines every item",
                case.name
            );
            assert_eq!(found.first_error_line, None, "{} parses cleanly", case.name);
            let line_count = text.lines().count();
            let Ok(elements) = u64::try_from(line_count) else {
                unreachable!("a line count fits a u64");
            };
            group.throughput(Throughput::Elements(elements));
            let id = BenchmarkId::from_parameter(line_count);
            group.bench_with_input(id, &text, |bencher, text| {
                bencher.iter(|| outline(black_box(case.language), black_box(text.as_str())));
            });
        }
        group.finish();
    }
}

/// An impl of `count` documented methods of three lines each.
fn rust_items(count: usize) -> String {
    let mut text = String::from("impl Items {\n");
    for index in 0..count {
        let item = format!(
            "    /// Item {index}.\n    fn item_{index}() {{\n        let value = {index};\n    }}\n"
        );
        text.push_str(&item);
    }
    text.push_str("}\n");
    text
}

/// A class of `count` documented methods of three lines each.
fn typescript_items(count: usize) -> String {
    let mut text = String::from("export class Items {\n");
    for index in 0..count {
        let item = format!(
            "  /** Item {index}. */\n  item{index}(): number {{\n    return {index};\n  }}\n"
        );
        text.push_str(&item);
    }
    text.push_str("}\n");
    text
}

fn configured() -> Criterion {
    let warm_up = Duration::from_secs(WARM_UP_SECONDS);
    let measurement = Duration::from_secs(MEASUREMENT_SECONDS);
    Criterion::default()
        .sample_size(SAMPLE_COUNT)
        .warm_up_time(warm_up)
        .measurement_time(measurement)
}

criterion_group! {
    name = benches;
    config = configured();
    targets = outline_by_length
}
criterion_main!(benches);
