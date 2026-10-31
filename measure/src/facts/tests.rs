//! What a fold makes of a transcript's lines. Each case writes the lines a transcript holds
//! as Claude Code writes them, reads each through the parser, and asserts on the whole of
//! the facts the fold finishes with, so a call counted or lost by mistake fails the case.

use claude_code_transcript::record::{Line, read_line};
use excerpt::guard::ReadSpan;
use serde_json::{Value, json};

use super::{CodeCommand, CodeRead, FACTS_PER_TRANSCRIPT_MAX, Facts, Fold, ReadResult};
use crate::Full;
use crate::window::Window;

const SESSION: &str = "9d5fe9d0-d389-44a2-a3ba-a29db8bbb544";
const INSIDE: &str = "2026-10-20T12:00:00Z";
const BEFORE: &str = "2026-10-17T12:00:00Z";
const REFUSAL: &str = "PreToolUse:Bash hook error: [excerpt guard]: excerpt guard: blocked a `cat` read of src/plan.rs.\nRead code by item instead:\n";

fn window() -> Window {
    Window {
        since: "2026-10-18T00:00:00Z".parse().unwrap(),
        until: None,
    }
}

fn line(record: &Value) -> Line {
    read_line(&record.to_string())
}

fn call(message_id: &str, stamp: &str, id: &str, tool: &str, input: &Value) -> Line {
    line(&json!({
        "type": "assistant",
        "sessionId": SESSION,
        "timestamp": stamp,
        "message": {
            "id": message_id,
            "model": "claude-opus-5-5",
            "content": [{ "type": "tool_use", "id": id, "name": tool, "input": input }],
            "usage": { "input_tokens": 1, "output_tokens": 1 }
        }
    }))
}

fn text(message_id: &str) -> Line {
    line(&json!({
        "type": "assistant",
        "sessionId": SESSION,
        "timestamp": INSIDE,
        "message": {
            "id": message_id,
            "model": "claude-opus-5-5",
            "content": [{ "type": "text", "text": "Done." }],
            "usage": { "input_tokens": 1, "output_tokens": 1 }
        }
    }))
}

fn result(id: &str, output: &str, is_error: bool) -> Line {
    line(&json!({
        "type": "user",
        "sessionId": SESSION,
        "timestamp": INSIDE,
        "message": {
            "role": "user",
            "content": [{ "type": "tool_result", "tool_use_id": id, "content": output, "is_error": is_error }]
        }
    }))
}

fn bash(command: &str) -> Value {
    json!({ "command": command, "description": "a call" })
}

fn fold(lines: &[Line]) -> Facts {
    let mut fold = Fold::new(window());
    for line in lines {
        fold.take(line).unwrap();
    }
    fold.finish()
}

fn command(command: &str, refused: bool) -> CodeCommand {
    CodeCommand {
        command: command.to_owned(),
        refused,
    }
}

mod given_bash_calls {
    use super::*;

    #[test]
    fn when_one_names_a_code_file_then_it_is_kept_with_whether_the_guard_refused_it() {
        let facts = fold(&[
            call("m1", INSIDE, "t1", "Bash", &bash("cat src/plan.rs")),
            result("t1", REFUSAL, true),
            call("m2", INSIDE, "t2", "Bash", &bash("rg -n plan src/plan.rs")),
            result("t2", "12:fn plan() {}", false),
        ]);

        assert_eq!(
            facts,
            Facts {
                commands: vec![
                    command("cat src/plan.rs", true),
                    command("rg -n plan src/plan.rs", false),
                ],
                reads: Vec::new(),
                unheld: 0,
            }
        );
    }

    #[test]
    fn when_a_result_quotes_a_refusal_it_is_not_then_the_call_is_not_refused() {
        let quoted = format!("test output:\n{REFUSAL}");
        let facts = fold(&[
            call("m1", INSIDE, "t1", "Bash", &bash("cat src/plan.rs")),
            result("t1", REFUSAL, false),
            call("m2", INSIDE, "t2", "Bash", &bash("cargo test src/plan.rs")),
            result("t2", &quoted, true),
        ]);

        assert_eq!(
            facts.commands,
            vec![
                command("cat src/plan.rs", false),
                command("cargo test src/plan.rs", false),
            ]
        );
    }

    #[test]
    fn when_each_is_judged_then_only_those_naming_a_code_file_extension_are_kept() {
        let rows = [
            ("cat src/plan.rs", true),
            ("sed -n 1,5p web/app.ts | less", true),
            ("ls web/view.tsx:", true),
            ("cat web/module.mts", true),
            ("cat 'src/plan.rs'", true),
            ("cat Cargo.toml", false),
            ("cat notes.rsx", false),
            ("cat web/app.tsv", false),
            ("echo .rs_files", false),
        ];
        for (text, kept) in rows {
            let facts = fold(&[
                call("m1", INSIDE, "t1", "Bash", &bash(text)),
                result("t1", "", false),
            ]);
            assert_eq!(facts.commands.len(), usize::from(kept), "{text}");
        }
    }

    #[test]
    fn when_one_was_made_before_the_window_then_it_is_not_kept() {
        let facts = fold(&[
            call("m1", BEFORE, "t1", "Bash", &bash("cat src/plan.rs")),
            result("t1", REFUSAL, true),
        ]);

        assert_eq!(facts, Facts::default());
    }

    #[test]
    fn when_one_awaits_no_result_then_it_is_not_kept() {
        let facts = fold(&[call("m1", INSIDE, "t1", "Bash", &bash("cat src/plan.rs"))]);

        assert_eq!(facts, Facts::default());
    }

    #[test]
    fn when_a_result_names_no_call_awaiting_then_it_is_passed_over() {
        let facts = fold(&[result("t9", REFUSAL, true)]);

        assert_eq!(facts, Facts::default());
    }

    #[test]
    fn when_two_share_an_id_while_awaiting_then_the_second_is_unheld() {
        let facts = fold(&[
            call("m1", INSIDE, "t1", "Bash", &bash("cat src/plan.rs")),
            call("m2", INSIDE, "t1", "Bash", &bash("cat src/plan.rs")),
            result("t1", "", false),
        ]);

        assert_eq!(
            facts,
            Facts {
                commands: vec![command("cat src/plan.rs", false)],
                reads: Vec::new(),
                unheld: 1,
            }
        );
    }
}

mod given_reads {
    use super::*;

    #[test]
    fn when_one_takes_a_code_file_whole_then_its_lines_characters_and_later_calls_are_kept() {
        let numbered = "     1\tfn plan() {\n     2\t}\n";
        let facts = fold(&[
            call(
                "m1",
                INSIDE,
                "t1",
                "Read",
                &json!({ "file_path": "/w/src/plan.rs" }),
            ),
            result("t1", numbered, false),
            text("m2"),
            text("m3"),
        ]);

        assert_eq!(
            facts,
            Facts {
                commands: Vec::new(),
                reads: vec![CodeRead {
                    span: ReadSpan::Whole,
                    refused: false,
                    result: ReadResult::Numbered {
                        lines: 2,
                        characters: numbered.chars().count(),
                    },
                    calls_after: 2,
                }],
                unheld: 0,
            }
        );
    }

    #[test]
    fn when_one_names_an_offset_or_a_limit_then_its_span_is_ranged() {
        let rows = [
            (
                json!({ "file_path": "/w/src/plan.rs", "offset": 10 }),
                ReadSpan::Ranged,
            ),
            (
                json!({ "file_path": "/w/src/plan.rs", "limit": 5 }),
                ReadSpan::Ranged,
            ),
            (
                json!({ "file_path": "/w/src/plan.rs", "offset": null }),
                ReadSpan::Whole,
            ),
        ];
        for (input, span) in rows {
            let facts = fold(&[
                call("m1", INSIDE, "t1", "Read", &input),
                result("t1", "    10\tfn plan() {", false),
            ]);
            assert_eq!(facts.reads.len(), 1, "{input}");
            assert_eq!(facts.reads[0].span, span, "{input}");
        }
    }

    #[test]
    fn when_the_guard_refused_one_then_it_is_refused_with_no_numbered_lines() {
        let refusal = "PreToolUse:Read hook error: [excerpt guard]: excerpt guard: blocked a subagent's whole-file Read of /w/src/plan.rs.\n";
        let facts = fold(&[
            call(
                "m1",
                INSIDE,
                "t1",
                "Read",
                &json!({ "file_path": "/w/src/plan.rs" }),
            ),
            result("t1", refusal, true),
        ]);

        assert_eq!(
            facts.reads,
            vec![CodeRead {
                span: ReadSpan::Whole,
                refused: true,
                result: ReadResult::Unnumbered,
                calls_after: 0,
            }]
        );
    }

    #[test]
    fn when_a_result_numbers_no_first_line_then_it_is_unnumbered() {
        let rows = [
            "<system-reminder>Warning: the file exists but the contents are empty.</system-reminder>",
            "File does not exist.",
            "     2\tfn plan() {",
        ];
        for output in rows {
            let facts = fold(&[
                call(
                    "m1",
                    INSIDE,
                    "t1",
                    "Read",
                    &json!({ "file_path": "/w/src/plan.rs" }),
                ),
                result("t1", output, false),
            ]);
            assert_eq!(facts.reads[0].result, ReadResult::Unnumbered, "{output}");
        }
    }

    #[test]
    fn when_one_takes_a_file_of_another_language_then_it_is_not_kept() {
        let facts = fold(&[
            call(
                "m1",
                INSIDE,
                "t1",
                "Read",
                &json!({ "file_path": "/w/README.md" }),
            ),
            result("t1", "     1\t# excerpt", false),
        ]);

        assert_eq!(facts, Facts::default());
    }
}

mod given_more_calls_than_the_bound {
    use super::*;
    use claude_code_transcript::record::{AssistantBlock, UserBlock};

    /// `template` with the id its one call or result names set to `id`.
    fn renamed(template: &Line, id: &str) -> Line {
        let mut line = template.clone();
        match &mut line {
            Line::Assistant(assistant) => match assistant.blocks.as_mut_slice() {
                [AssistantBlock::ToolUse(tool_use)] => tool_use.id = id.to_owned(),
                blocks => panic!("the template makes one call: {blocks:?}"),
            },
            Line::User(user) => match user.blocks.as_mut_slice() {
                [UserBlock::ToolResult(result)] => result.tool_use_id = id.to_owned(),
                blocks => panic!("the template holds one result: {blocks:?}"),
            },
            other => panic!("the template is a call or a result: {other:?}"),
        }
        line
    }

    #[test]
    fn when_the_result_past_it_arrives_then_the_fold_is_full() {
        let call_template = call("m1", INSIDE, "t", "Bash", &bash("cat src/plan.rs"));
        let result_template = result("t", "", false);
        let mut fold = Fold::new(window());
        for index in 0..FACTS_PER_TRANSCRIPT_MAX {
            let id = format!("t{index}");
            fold.take(&renamed(&call_template, &id)).unwrap();
            fold.take(&renamed(&result_template, &id)).unwrap();
        }
        fold.take(&renamed(&call_template, "past")).unwrap();

        let past = fold.take(&renamed(&result_template, "past"));

        assert_eq!(past, Err(Full::Facts));
    }
}
