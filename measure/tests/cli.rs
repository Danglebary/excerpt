//! The binary end to end, against a transcript store written in a scratch home per case:
//! what it prints to stdout and to stderr, and its exit code. The figures are tested in
//! the library; these cases cover the wiring: finding the store beneath `HOME`, reading
//! each transcript and its subagents', passing over a transcript last written before the
//! window, and which stream each message lands on.

use std::fs::{self, File};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use support::{Scratch, run};

#[cfg(test)]
mod support {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    /// A home of its own for one case, removed when the case ends.
    pub struct Scratch {
        pub root: PathBuf,
    }

    impl Scratch {
        pub fn new() -> Self {
            let index = NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst);
            let name = format!("excerpt-measure-cli-{}-{index}", std::process::id());
            let root = std::env::temp_dir().join(name);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        /// Writes `lines` as the transcript at `path`, beneath the home's store.
        pub fn transcript(&self, path: &str, lines: &[serde_json::Value]) -> PathBuf {
            let mut text = String::new();
            for line in lines {
                text.push_str(&line.to_string());
                text.push('\n');
            }
            self.write(path, &text)
        }

        pub fn write(&self, path: &str, text: &str) -> PathBuf {
            let path = self.root.join(".claude/projects").join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    /// Runs the binary with the scratch directory as its home: its exit code, stdout, and
    /// stderr.
    pub fn run(scratch: &Scratch, arguments: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_excerpt-measure"))
            .args(arguments)
            .env("HOME", &scratch.root)
            .output()
            .unwrap();
        (
            output.status.code().unwrap(),
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap(),
        )
    }
}

const SESSION: &str = "9d5fe9d0-d389-44a2-a3ba-a29db8bbb544";
const STAMP: &str = "2026-10-20T12:00:00Z";
const SINCE: &str = "2026-10-18T00:00:00Z";
const FOLDER: &str = "-home-admin-projects-plan";

fn call(message_id: &str, id: &str, tool: &str, input: &Value) -> Value {
    json!({
        "type": "assistant",
        "sessionId": SESSION,
        "timestamp": STAMP,
        "message": {
            "id": message_id,
            "model": "claude-opus-5-5",
            "content": [{ "type": "tool_use", "id": id, "name": tool, "input": input }],
            "usage": { "input_tokens": 1, "output_tokens": 1 }
        }
    })
}

fn result(id: &str, output: &str, is_error: bool) -> Value {
    json!({
        "type": "user",
        "sessionId": SESSION,
        "timestamp": STAMP,
        "message": {
            "role": "user",
            "content": [{ "type": "tool_result", "tool_use_id": id, "content": output, "is_error": is_error }]
        }
    })
}

fn refusal(tool: &str) -> String {
    format!(
        "PreToolUse:{tool} hook error: [excerpt guard]: excerpt guard: blocked a read of src/plan.rs.\n"
    )
}

/// A store holding a main thread that drew one refusal of two Bash calls, and an Explore
/// subagent that read a code file whole once and drew one Read refusal.
fn store(scratch: &Scratch) {
    scratch.transcript(
        &format!("{FOLDER}/{SESSION}.jsonl"),
        &[
            call("m1", "t1", "Bash", &json!({ "command": "cat src/plan.rs" })),
            result("t1", &refusal("Bash"), true),
            call(
                "m2",
                "t2",
                "Bash",
                &json!({ "command": "head -3 src/plan.rs" }),
            ),
            result("t2", "fn plan() {}", false),
        ],
    );
    let subagent = format!("{FOLDER}/{SESSION}/subagents/agent-a1");
    scratch.write(
        &format!("{subagent}.meta.json"),
        r#"{"agentType":"Explore"}"#,
    );
    scratch.transcript(
        &format!("{subagent}.jsonl"),
        &[
            call(
                "m3",
                "t3",
                "Read",
                &json!({ "file_path": "/w/src/plan.rs" }),
            ),
            result("t3", "     1\tfn plan() {\n     2\t}\n", false),
            call(
                "m4",
                "t4",
                "Read",
                &json!({ "file_path": "/w/src/long.rs" }),
            ),
            result("t4", &refusal("Read"), true),
        ],
    );
}

mod given_a_store {
    use super::*;

    #[test]
    fn when_refusals_are_measured_then_each_thread_and_tool_is_counted_and_it_exits_0() {
        let scratch = Scratch::new();
        store(&scratch);

        let (code, stdout, stderr) = run(&scratch, &["refusals", "--since", SINCE]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(stderr, "");
        assert_eq!(
            stdout,
            "thread\ttool\tcalls\trefusals\n\
             main\tBash\t2\t1\n\
             Explore\tRead\t2\t1\n"
        );
    }

    #[test]
    fn when_subagent_reads_are_measured_then_the_subagents_whole_reads_are_tabled() {
        let scratch = Scratch::new();
        store(&scratch);

        let (code, stdout, stderr) = run(&scratch, &["subagent-reads", "--since", SINCE]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            stdout.lines().nth(1).unwrap(),
            "Explore\t1\t0\t2\t2\t2\t14\t1"
        );
    }

    #[test]
    fn when_replayed_then_each_call_is_counted_by_its_verdict_then_and_now() {
        let scratch = Scratch::new();
        store(&scratch);

        let (code, stdout, stderr) = run(&scratch, &["replay", "--since", SINCE]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            stdout,
            "then\tnow\tcalls\n\
             passed\tdenied: head\t1\n\
             refused\tdenied: cat\t1\n\
             \n\
             session_refusals\tsessions\tcalls\n\
             none\t0\t0\n\
             some\t1\t1\n"
        );
    }

    #[test]
    fn when_a_transcript_was_last_written_before_the_window_then_it_is_not_read() {
        let scratch = Scratch::new();
        store(&scratch);
        let main_thread = scratch
            .root
            .join(".claude/projects")
            .join(format!("{FOLDER}/{SESSION}.jsonl"));
        let before = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        File::options()
            .write(true)
            .open(&main_thread)
            .unwrap()
            .set_modified(before)
            .unwrap();

        let (code, stdout, stderr) = run(&scratch, &["refusals", "--since", SINCE]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            stdout,
            "thread\ttool\tcalls\trefusals\nExplore\tRead\t2\t1\n"
        );
    }

    #[test]
    fn when_a_line_is_malformed_then_it_is_counted_on_stderr() {
        let scratch = Scratch::new();
        store(&scratch);
        let folder = scratch.root.join(".claude/projects").join(FOLDER);
        fs::write(
            folder.join("broken.jsonl"),
            "{\"type\":\"user\", not json\n",
        )
        .unwrap();

        let (code, _, stderr) = run(&scratch, &["refusals", "--since", SINCE]);

        assert_eq!(code, 0, "{stderr}");
        assert_eq!(stderr, "excerpt-measure: passed over 1 malformed line\n");
    }
}

mod given_no_store {
    use super::*;

    #[test]
    fn when_measured_then_it_exits_1_naming_the_folder_it_could_not_list() {
        let scratch = Scratch::new();

        let (code, stdout, stderr) = run(&scratch, &["refusals", "--since", SINCE]);

        assert_eq!(code, 1);
        assert_eq!(stdout, "");
        assert!(
            stderr.starts_with("excerpt-measure: could not list "),
            "{stderr}"
        );
        assert!(stderr.contains(".claude/projects"), "{stderr}");
    }
}

mod given_the_command_line {
    use super::*;

    #[test]
    fn when_the_arguments_are_malformed_then_it_exits_2_pointing_at_the_help() {
        let scratch = Scratch::new();

        let (code, stdout, stderr) = run(&scratch, &["refusals"]);

        assert_eq!(code, 2);
        assert_eq!(stdout, "");
        assert!(
            stderr.ends_with("Run `excerpt-measure --help` for the grammar.\n"),
            "{stderr}"
        );
    }

    #[test]
    fn when_asked_for_help_then_it_prints_the_grammar_and_exits_0() {
        let scratch = Scratch::new();

        let (code, stdout, _) = run(&scratch, &["--help"]);

        assert_eq!(code, 0);
        assert!(
            stdout.starts_with("excerpt-measure: figures about excerpt's guard"),
            "{stdout}"
        );
    }
}
