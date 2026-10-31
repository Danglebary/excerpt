//! The binary end to end, against real files in a scratch directory per case: what it
//! prints to stdout and to stderr, and the exit code an agent's `&&` chain turns on. The
//! pure rules are tested in the library; these cases cover the wiring: the walk, the
//! reads, the bounds, and which stream each message lands on.

use std::fmt::Write as _;
use std::fs;

use excerpt::parallel::{ATTEMPT_INPUT_BYTES_MAX, WORKERS_MAX};
use excerpt::report::{LISTED_MATCHES_MAX, OUTLINE_ITEMS_MAX};
use support::{Scratch, bash_call, lines, read_call, run, run_with_input};

#[cfg(test)]
mod support {
    use std::fmt::Write as _;
    use std::fs;
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::process::{Command, Output, Stdio};
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

    /// Runs the binary from the scratch directory with `input` on stdin: its exit code,
    /// stdout, and stderr.
    pub fn run_with_input(
        scratch: &Scratch,
        arguments: &[&str],
        input: &[u8],
    ) -> (i32, String, String) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_excerpt"))
            .args(arguments)
            .current_dir(&scratch.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let output = child.wait_with_output().unwrap();
        (
            output.status.code().unwrap(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }

    /// The input Claude Code hands a `PreToolUse` hook for a Bash call running `command`.
    pub fn bash_call(command: &str) -> Vec<u8> {
        let input = serde_json::json!({
            "session_id": "session",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": command, "description": "a call" },
        });
        serde_json::to_vec(&input).unwrap()
    }

    /// A file of `line_count` lines, each one item's worth of text.
    pub fn lines(line_count: usize) -> String {
        let mut text = String::new();
        for index in 0..line_count {
            writeln!(text, "const LINE_{index}: u8 = 0;").unwrap();
        }
        text
    }

    /// The input Claude Code hands a `PreToolUse` hook for a Read of `path` naming no
    /// `offset` or `limit`, made inside the subagent `agent_id` names or on the main thread.
    pub fn read_call(path: &str, agent_id: Option<&str>) -> Vec<u8> {
        let mut input = serde_json::json!({
            "session_id": "session",
            "hook_event_name": "PreToolUse",
            "tool_name": "Read",
            "tool_input": { "file_path": path },
        });
        if let Some(agent_id) = agent_id {
            input["agent_id"] = serde_json::json!(agent_id);
            input["agent_type"] = serde_json::json!("Explore");
        }
        serde_json::to_vec(&input).unwrap()
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

/// The largest file the binary reads, 2 MiB.
const FILE_BYTES_MAX: usize = 2_097_152;

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
    fn when_run_on_more_files_than_workers_then_they_are_outlined_in_name_order() {
        let scratch = Scratch::new();
        let file_count = WORKERS_MAX.checked_mul(3).unwrap();
        let mut expected = String::new();
        for file in 0..file_count {
            let name = format!("src/file_{file:02}.rs");
            scratch.write(&name, &format!("fn item_{file}() {{}}\n"));
            writeln!(expected, "{name}\n  1-1 fn item_{file}").unwrap();
        }

        let (code, stdout, stderr) = run(&scratch, &["outline", "src"]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(stdout, expected);
    }

    #[test]
    fn when_run_on_a_file_an_attempt_defers_then_it_keeps_its_place_in_name_order() {
        let scratch = Scratch::new();
        scratch.write("src/a.rs", "fn a() {}\n");
        scratch.write("src/c.rs", "fn c() {}\n");
        let filler = "    // filler\n";
        let bytes_max = usize::try_from(ATTEMPT_INPUT_BYTES_MAX).unwrap();
        let filler_count = (bytes_max / filler.len()).checked_add(1).unwrap();
        let mut large = String::from("fn b() {\n");
        for _ in 0..filler_count {
            large.push_str(filler);
        }
        large.push_str("}\n");
        scratch.write("src/b.rs", &large);
        let last_line = filler_count.checked_add(2).unwrap();

        let (code, stdout, stderr) = run(&scratch, &["outline", "src"]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            stdout,
            format!("src/a.rs\n  1-1 fn a\nsrc/b.rs\n  1-{last_line} fn b\nsrc/c.rs\n  1-1 fn c\n")
        );
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
    fn when_run_on_more_items_than_the_bound_then_it_stops_in_the_file_reaching_it() {
        let scratch = Scratch::new();
        let items_per_file: usize = 300;
        assert!(
            OUTLINE_ITEMS_MAX > items_per_file,
            "the first file fits the bound"
        );
        assert!(
            OUTLINE_ITEMS_MAX < items_per_file.checked_mul(2).unwrap(),
            "the second file reaches the bound"
        );
        let second_listed = OUTLINE_ITEMS_MAX.checked_sub(items_per_file).unwrap();
        let second_left_out = items_per_file.checked_sub(second_listed).unwrap();
        let mut expected = String::new();
        for (file, listed) in [("a", items_per_file), ("b", second_listed), ("c", 0)] {
            let mut text = String::new();
            for line in 1..=items_per_file {
                writeln!(text, "fn {file}_{line}() {{}}").unwrap();
            }
            scratch.write(&format!("src/{file}.rs"), &text);
            if listed > 0 {
                writeln!(expected, "src/{file}.rs").unwrap();
            }
            for line in 1..=listed {
                writeln!(expected, "  {line}-{line} fn {file}_{line}").unwrap();
            }
        }
        writeln!(expected, "  ({second_left_out} more items left out)").unwrap();
        writeln!(expected, "1 more file left out; outline a narrower path").unwrap();

        let (code, stdout, stderr) = run(&scratch, &["outline", "src"]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(stdout, expected);
    }

    #[test]
    fn when_run_on_a_file_that_is_not_utf8_then_it_is_named_as_skipped() {
        let scratch = Scratch::new();
        fs::write(scratch.root.join("binary.rs"), [0xff_u8, 0xfe, 0x00]).unwrap();

        let (code, stdout, _) = run(&scratch, &["outline", "binary.rs"]);

        assert_eq!(code, 0);
        assert_eq!(stdout, "binary.rs\n  (skipped: not UTF-8 text)\n");
    }

    #[test]
    fn when_run_on_a_file_larger_than_the_read_bound_then_it_is_named_as_skipped() {
        let scratch = Scratch::new();
        let filler = "// generated\n";
        let filler_count = (FILE_BYTES_MAX / filler.len()).checked_add(1).unwrap();
        let large = filler.repeat(filler_count);
        scratch.write("src/generated.rs", &large);
        let bytes = large.len();

        let (code, stdout, stderr) = run(&scratch, &["outline", "src/generated.rs"]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            stdout,
            format!(
                "src/generated.rs\n  (skipped: {bytes} bytes, more than the {FILE_BYTES_MAX} excerpt reads)\n"
            )
        );
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
    fn when_items_in_more_files_than_workers_answer_then_they_are_listed_in_name_order() {
        let scratch = Scratch::new();
        let file_count = WORKERS_MAX.checked_mul(3).unwrap();
        // Each file holds the item on a line of its own, so a file listed with another
        // file's outcome shows the wrong lines.
        for file in 0..file_count {
            let text = format!("{}fn shared() {{}}\n", "\n".repeat(file));
            scratch.write(&format!("src/file_{file:02}.rs"), &text);
        }

        let (code, _, stderr) = run(&scratch, &["show", "shared", "src"]);

        assert_eq!(code, 1);
        let listed: Vec<&str> = stderr
            .lines()
            .filter(|line| line.starts_with("  src/"))
            .collect();
        let expected: Vec<String> = (0..LISTED_MATCHES_MAX)
            .map(|file| {
                let line = file.checked_add(1).unwrap();
                format!("  src/file_{file:02}.rs:{line}-{line} fn shared")
            })
            .collect();
        assert_eq!(listed, expected);
    }

    #[test]
    fn when_more_items_answer_than_are_listed_then_every_one_is_counted() {
        let scratch = Scratch::new();
        let file_count = LISTED_MATCHES_MAX.checked_add(10).unwrap();
        for file in 0..file_count {
            scratch.write(&format!("src/file_{file:02}.rs"), "fn shared() {}\n");
        }

        let (code, _, stderr) = run(&scratch, &["show", "shared", "src"]);

        assert_eq!(code, 1);
        assert!(
            stderr.starts_with(&format!("excerpt: {file_count} items answer `shared`;")),
            "{stderr}"
        );
        assert!(stderr.ends_with("\n  and 10 more\n"), "{stderr}");
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

#[cfg(target_env = "musl")]
mod given_musl {
    use std::process::Command;

    use super::*;

    /// Items in the generated file: enough that its parse holds most of the process's
    /// memory.
    const ITEM_COUNT: usize = 15_000;
    /// The least share of the process's peak memory mimalloc must have committed: most of
    /// it, which holds only when the parse's allocations are mimalloc's.
    const COMMITTED_SHARE_MIN: f64 = 0.5;

    #[test]
    fn when_a_large_file_is_parsed_then_mimalloc_commits_most_of_the_memory() {
        let scratch = Scratch::new();
        let mut text = String::from("impl Items {\n");
        for index in 0..ITEM_COUNT {
            writeln!(
                text,
                "    /// Item {index}.\n    fn item_{index}() {{\n        let value = {index};\n    }}"
            )
            .unwrap();
        }
        text.push_str("}\n");
        scratch.write("items.rs", &text);

        let output = Command::new(env!("CARGO_BIN_EXE_excerpt"))
            .args(["outline", "items.rs"])
            .env("MIMALLOC_SHOW_STATS", "1")
            .current_dir(&scratch.root)
            .output()
            .unwrap();

        let stderr = String::from_utf8(output.stderr).unwrap();
        let peak_rss = statistic(&stderr, "peak rss: ");
        let peak_commit = statistic(&stderr, "peak commit: ");
        assert!(peak_commit >= peak_rss * COMMITTED_SHARE_MIN, "{stderr}");
    }

    /// The size mimalloc's statistics print after `label`, such as `46.7 MiB`, in bytes.
    #[cfg(test)]
    fn statistic(stderr: &str, label: &str) -> f64 {
        let (_, after) = stderr
            .split_once(label)
            .unwrap_or_else(|| panic!("mimalloc prints `{label}`:\n{stderr}"));
        let mut words = after.split_whitespace();
        let number: f64 = words.next().unwrap().parse().unwrap();
        let unit = words.next().unwrap().trim_end_matches(',');
        let scale = match unit {
            "B" => 1.0,
            "KiB" => 1_024.0,
            "MiB" => 1_048_576.0,
            "GiB" => 1_073_741_824.0,
            other => panic!("`{other}` is a unit mimalloc prints"),
        };
        number * scale
    }
}

#[cfg(not(target_env = "musl"))]
mod given_another_c_library {
    use std::process::Command;

    use super::*;

    #[test]
    fn when_run_asking_mimalloc_for_statistics_then_none_print() {
        let scratch = Scratch::new();
        scratch.write("plan.rs", PLAN);

        let output = Command::new(env!("CARGO_BIN_EXE_excerpt"))
            .args(["outline", "plan.rs"])
            .env("MIMALLOC_SHOW_STATS", "1")
            .current_dir(&scratch.root)
            .output()
            .unwrap();

        assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    }
}

mod given_guard {
    use super::*;
    use excerpt::guard::{HOOK_INPUT_BYTES_MAX, SUBAGENT_READ_LINES_MAX};

    #[test]
    fn when_the_command_prints_source_lines_then_it_exits_2_with_the_refusal_on_stderr() {
        let scratch = Scratch::new();

        let (code, stdout, stderr) =
            run_with_input(&scratch, &["guard"], &bash_call("sed -n 1,40p src/plan.rs"));

        assert_eq!(code, 2);
        assert_eq!(stdout, "");
        assert!(
            stderr
                .starts_with("excerpt guard: blocked a `sed -n` line-range read of src/plan.rs.\n"),
            "{stderr}"
        );
    }

    #[test]
    fn when_the_command_reads_by_item_then_it_exits_0_printing_nothing() {
        let scratch = Scratch::new();

        let (code, stdout, stderr) = run_with_input(
            &scratch,
            &["guard"],
            &bash_call("excerpt show plan src/plan.rs"),
        );

        assert_eq!((code, stdout.as_str(), stderr.as_str()), (0, "", ""));
    }

    #[test]
    fn when_the_call_is_another_tool_s_then_it_exits_0_printing_nothing() {
        let scratch = Scratch::new();
        let input = br#"{"tool_name":"Grep","tool_input":{"pattern":"plan","path":"src/plan.rs"}}"#;

        let (code, stdout, stderr) = run_with_input(&scratch, &["guard"], input);

        assert_eq!((code, stdout.as_str(), stderr.as_str()), (0, "", ""));
    }

    #[test]
    fn when_a_subagent_reads_a_code_file_past_the_bound_whole_then_it_exits_2_with_the_refusal() {
        let scratch = Scratch::new();
        scratch.write("long.rs", &lines(SUBAGENT_READ_LINES_MAX + 1));
        let path = scratch.root.join("long.rs").to_string_lossy().into_owned();

        let (code, stdout, stderr) =
            run_with_input(&scratch, &["guard"], &read_call(&path, Some("agent")));

        assert_eq!(code, 2);
        assert_eq!(stdout, "");
        let headline = format!("excerpt guard: blocked a subagent's whole-file Read of {path}.\n");
        assert!(stderr.starts_with(&headline), "{stderr}");
    }

    #[test]
    fn when_a_whole_read_is_one_the_bound_lets_through_then_it_exits_0_printing_nothing() {
        let scratch = Scratch::new();
        scratch.write("short.rs", &lines(SUBAGENT_READ_LINES_MAX));
        scratch.write("long.rs", &lines(SUBAGENT_READ_LINES_MAX + 1));
        let short = scratch.root.join("short.rs").to_string_lossy().into_owned();
        let long = scratch.root.join("long.rs").to_string_lossy().into_owned();
        let missing = scratch
            .root
            .join("missing.rs")
            .to_string_lossy()
            .into_owned();
        let calls = [
            read_call(&short, Some("agent")),
            read_call(&long, None),
            read_call(&missing, Some("agent")),
        ];

        for call in calls {
            let (code, stdout, stderr) = run_with_input(&scratch, &["guard"], &call);

            assert_eq!(
                (code, stdout.as_str(), stderr.as_str()),
                (0, "", ""),
                "{}",
                String::from_utf8_lossy(&call)
            );
        }
    }

    #[test]
    fn when_the_input_is_malformed_then_it_exits_1_without_blocking() {
        let scratch = Scratch::new();

        let (code, _, stderr) = run_with_input(&scratch, &["guard"], b"sed -n 1,40p a.rs");

        assert_eq!(code, 1);
        assert!(
            stderr.starts_with(
                "excerpt guard: the hook input is not what Claude Code sends: it is not JSON"
            ),
            "{stderr}"
        );
    }

    #[test]
    fn when_the_input_is_past_the_read_bound_then_it_exits_1_without_blocking() {
        let scratch = Scratch::new();
        let input_bytes = usize::try_from(HOOK_INPUT_BYTES_MAX + 1).unwrap();

        let (code, _, stderr) = run_with_input(&scratch, &["guard"], &vec![b' '; input_bytes]);

        assert_eq!(code, 1);
        assert!(
            stderr.starts_with("excerpt guard: could not read the hook input"),
            "{stderr}"
        );
    }
}

mod given_brief {
    use super::*;

    #[test]
    fn when_run_then_it_prints_the_subagent_start_context_and_exits_0() {
        let scratch = Scratch::new();

        let (code, stdout, stderr) = run(&scratch, &["brief"]);

        assert_eq!((code, stderr.as_str()), (0, ""));
        let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(
            output["hookSpecificOutput"]["hookEventName"],
            "SubagentStart"
        );
        assert_eq!(
            output["hookSpecificOutput"]["additionalContext"],
            excerpt::report::subagent_brief()
        );
    }
}
