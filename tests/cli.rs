//! The binary end to end, against real files in a scratch directory per case: what it
//! prints to stdout and to stderr, and the exit code an agent's `&&` chain turns on. The
//! pure rules are tested in the library; these cases cover the wiring: the walk, the
//! reads, the bounds, and which stream each message lands on.

use std::fmt::Write as _;
use std::fs;

use support::{Scratch, run};

#[cfg(test)]
mod support {
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    /// A directory of its own for one case, removed when the case ends.
    pub struct Scratch {
        pub root: PathBuf,
    }

    impl Scratch {
        pub fn new() -> Self {
            let index = NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst);
            let name = format!("excerpt-cli-{}-{index}", std::process::id());
            let root = std::env::temp_dir().join(name);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        pub fn write(&self, name: &str, text: &str) {
            let path = self.root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    /// Runs the binary from the scratch directory: its exit code, stdout, and stderr.
    pub fn run(scratch: &Scratch, arguments: &[&str]) -> (i32, String, String) {
        let output: Output = Command::new(env!("CARGO_BIN_EXE_excerpt"))
            .args(arguments)
            .current_dir(&scratch.root)
            .output()
            .unwrap();
        (
            output.status.code().unwrap(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }
}

const PLAN: &str = "\
/// A plan.
pub struct Plan;

impl Plan {
    pub fn new() -> Self {
        Self
    }
}

pub fn apply(plan: &Plan) {}
";

mod given_outline {
    use super::*;

    #[test]
    fn when_run_on_a_file_then_its_items_are_printed_and_it_exits_0() {
        let scratch = Scratch::new();
        scratch.write("src/plan.rs", PLAN);

        let (code, stdout, stderr) = run(&scratch, &["outline", "src/plan.rs"]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            stdout,
            "src/plan.rs\n  1-2 struct Plan\n  4-8 impl Plan\n    5-7 fn Plan::new\n  10-10 fn apply\n"
        );
    }

    #[test]
    fn when_run_on_a_directory_then_source_files_are_outlined_in_name_order() {
        let scratch = Scratch::new();
        scratch.write("src/b.rs", "fn b() {}\n");
        scratch.write("src/a.ts", "export function a() {}\n");
        scratch.write("src/notes.md", "# Notes\n");

        let (code, stdout, _) = run(&scratch, &["outline", "src"]);

        assert_eq!(code, 0);
        assert_eq!(stdout, "src/a.ts\n  1-1 function a\nsrc/b.rs\n  1-1 fn b\n");
    }

    #[test]
    fn when_run_on_the_working_directory_then_ignored_and_hidden_files_are_skipped() {
        let scratch = Scratch::new();
        scratch.write(".gitignore", "target/\n");
        scratch.write("target/debug/build.rs", "fn generated() {}\n");
        scratch.write(".hidden/secret.rs", "fn hidden() {}\n");
        scratch.write("src/lib.rs", "fn kept() {}\n");

        let (code, stdout, _) = run(&scratch, &["outline"]);

        assert_eq!(code, 0);
        assert_eq!(stdout, "src/lib.rs\n  1-1 fn kept\n");
    }

    #[test]
    fn when_run_on_a_file_that_is_not_utf8_then_it_is_named_as_skipped() {
        let scratch = Scratch::new();
        fs::write(scratch.root.join("binary.rs"), [0xff_u8, 0xfe, 0x00]).unwrap();

        let (code, stdout, _) = run(&scratch, &["outline", "binary.rs"]);

        assert_eq!(code, 0);
        assert_eq!(stdout, "binary.rs\n  (skipped: not UTF-8 text)\n");
    }
}

mod given_show {
    use super::*;

    #[test]
    fn when_one_item_answers_then_it_is_printed_whole_with_numbered_lines() {
        let scratch = Scratch::new();
        scratch.write("src/plan.rs", PLAN);

        let (code, stdout, stderr) = run(&scratch, &["show", "Plan::new", "src"]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            stdout,
            "src/plan.rs:5-7 fn Plan::new\n     5\t    pub fn new() -> Self {\n     6\t        Self\n     7\t    }\n"
        );
    }

    #[test]
    fn when_several_items_answer_then_each_is_listed_on_stderr_and_it_exits_1() {
        let scratch = Scratch::new();
        scratch.write("src/plan.rs", PLAN);

        let (code, stdout, stderr) = run(&scratch, &["show", "Plan", "src"]);

        assert_eq!(code, 1);
        assert_eq!(stdout, "");
        assert!(
            stderr.contains("  src/plan.rs:1-2 struct Plan\n"),
            "{stderr}"
        );
        assert!(stderr.contains("  src/plan.rs:4-8 impl Plan\n"), "{stderr}");
    }

    #[test]
    fn when_several_items_answer_under_all_then_each_is_printed() {
        let scratch = Scratch::new();
        scratch.write("src/plan.rs", PLAN);

        let (code, stdout, _) = run(&scratch, &["show", "--all", "Plan", "src"]);

        assert_eq!(code, 0);
        assert!(
            stdout.starts_with("src/plan.rs:1-2 struct Plan\n"),
            "{stdout}"
        );
        assert!(
            stdout.contains("\n\nsrc/plan.rs:4-8 impl Plan\n"),
            "{stdout}"
        );
    }

    #[test]
    fn when_no_item_answers_then_it_exits_1_naming_the_paths() {
        let scratch = Scratch::new();
        scratch.write("src/plan.rs", PLAN);

        let (code, stdout, stderr) = run(&scratch, &["show", "absent", "src"]);

        assert_eq!(code, 1);
        assert_eq!(stdout, "");
        assert!(
            stderr.contains("no item answers `absent` under src"),
            "{stderr}"
        );
    }

    #[test]
    fn when_the_item_is_too_long_to_print_then_its_members_are_offered_and_it_exits_1() {
        let scratch = Scratch::new();
        let mut text = String::from("impl Huge {\n    fn first() {}\n");
        for line in 0..1_000 {
            writeln!(text, "    // line {line}").unwrap();
        }
        text.push_str("    fn last() {}\n}\n");
        scratch.write("huge.rs", &text);

        let (code, stdout, stderr) = run(&scratch, &["show", "impl Huge", "huge.rs"]);

        assert_eq!(code, 1);
        assert_eq!(stdout, "");
        assert!(stderr.contains("spans 1004 lines"), "{stderr}");
        assert!(
            stderr.contains("  huge.rs:2-2 fn Huge::first\n"),
            "{stderr}"
        );
    }
}

mod given_paths_it_cannot_read {
    use super::*;

    #[test]
    fn when_a_named_path_does_not_exist_then_it_exits_3() {
        let scratch = Scratch::new();

        let (code, stdout, stderr) = run(&scratch, &["outline", "missing.rs"]);

        assert_eq!(code, 3);
        assert_eq!(stdout, "");
        assert!(stderr.contains("could not read missing.rs"), "{stderr}");
    }

    #[test]
    fn when_a_named_file_is_in_another_language_then_it_exits_1() {
        let scratch = Scratch::new();
        scratch.write("tool.py", "def run(): pass\n");

        let (code, _, stderr) = run(&scratch, &["show", "run", "tool.py"]);

        assert_eq!(code, 1);
        assert!(
            stderr.contains("tool.py is not a file excerpt reads"),
            "{stderr}"
        );
    }

    #[test]
    fn when_a_directory_holds_no_source_file_then_it_exits_1() {
        let scratch = Scratch::new();
        scratch.write("docs/guide.md", "# Guide\n");

        let (code, _, stderr) = run(&scratch, &["outline", "docs"]);

        assert_eq!(code, 1);
        assert!(
            stderr.contains("no .rs, .ts, .mts, .cts, or .tsx file under docs"),
            "{stderr}"
        );
    }
}

mod given_the_command_line {
    use super::*;

    #[test]
    fn when_the_arguments_are_malformed_then_it_exits_2_pointing_at_the_help() {
        let scratch = Scratch::new();

        let (code, _, stderr) = run(&scratch, &["find", "plan"]);

        assert_eq!(code, 2);
        assert!(stderr.contains("`find` is not a command"), "{stderr}");
    }

    #[test]
    fn when_asked_for_its_version_then_it_prints_the_crate_version() {
        let scratch = Scratch::new();

        let (code, stdout, _) = run(&scratch, &["--version"]);

        assert_eq!(code, 0);
        assert_eq!(stdout, format!("excerpt {}\n", env!("CARGO_PKG_VERSION")));
    }
}
