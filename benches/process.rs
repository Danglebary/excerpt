//! What one call to the built binary costs from spawn to exit, which is the cost an agent
//! pays per call. Each case spawns the binary cargo builds for this bench against a
//! generated tree of Rust files and waits for it to exit; `--version` reads nothing and
//! stands as the floor every call pays to start a process. A show of a name one file
//! holds is screened down to that file before any parse, and a show of a name every file
//! holds parses them all, so the two bound what a show costs. Each case checks its exit
//! code before it is timed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use excerpt::exit_code;

const BINARY: &str = env!("CARGO_BIN_EXE_excerpt");
const SCRATCH_DIRECTORY: &str = env!("CARGO_TARGET_TMPDIR");
/// Files in the generated tree, and the items each holds.
const FILE_COUNT: usize = 200;
const ITEMS_PER_FILE: usize = 50;
const SAMPLE_COUNT: usize = 20;
const WARM_UP_SECONDS: u64 = 1;
const MEASUREMENT_SECONDS: u64 = 5;

/// One call: the arguments after the binary and the code it exits with.
struct Call {
    name: &'static str,
    arguments: &'static [&'static str],
    exit_code: u8,
}

const CALLS: [Call; 4] = [
    Call {
        name: "version",
        arguments: &["--version"],
        exit_code: exit_code::SUCCESS,
    },
    Call {
        name: "outline-tree",
        arguments: &["outline", "tree"],
        exit_code: exit_code::SUCCESS,
    },
    Call {
        name: "show-one-file",
        arguments: &["show", "Module7::item_3", "tree"],
        exit_code: exit_code::SUCCESS,
    },
    Call {
        name: "show-every-file",
        arguments: &["show", "--all", "item_3", "tree"],
        exit_code: exit_code::REFUSED,
    },
];

fn call_by_kind(criterion: &mut Criterion) {
    let root = generated_tree();
    let mut group = criterion.benchmark_group("call");
    for call in &CALLS {
        let status = spawn(&root, call.arguments);
        assert_eq!(status, call.exit_code, "{} exits as it declares", call.name);
        group.bench_function(call.name, |bencher| {
            bencher.iter(|| spawn(&root, call.arguments));
        });
    }
    group.finish();
}

/// `FILE_COUNT` files, each an impl of `ITEMS_PER_FILE` methods qualified by a type named
/// for the file, so a qualified name is held by one file and a bare one by all of them.
fn generated_tree() -> PathBuf {
    let root = Path::new(SCRATCH_DIRECTORY).join("process-bench");
    let tree = root.join("tree");
    fs::create_dir_all(&tree)
        .unwrap_or_else(|error| unreachable!("the bench tree is writable: {error}"));
    for file in 0..FILE_COUNT {
        let mut text = format!("impl Module{file} {{\n");
        for item in 0..ITEMS_PER_FILE {
            let method = format!("    fn item_{item}() {{\n        let value = {item};\n    }}\n");
            text.push_str(&method);
        }
        text.push_str("}\n");
        let path = tree.join(format!("module_{file}.rs"));
        fs::write(path, text)
            .unwrap_or_else(|error| unreachable!("the bench tree is writable: {error}"));
    }
    root
}

fn spawn(root: &Path, arguments: &[&str]) -> u8 {
    let status = Command::new(BINARY)
        .args(arguments)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|error| unreachable!("the built binary starts: {error}"));
    let Some(code) = status.code() else {
        unreachable!("the binary exits rather than being signaled");
    };
    let Ok(code) = u8::try_from(code) else {
        unreachable!("an exit code fits a u8");
    };
    code
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
    targets = call_by_kind
}
criterion_main!(benches);
