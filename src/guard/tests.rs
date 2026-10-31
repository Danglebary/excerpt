//! The hook's verdict over commands shaped like the ones agents ran: each row is one Bash
//! command and the verdict it must get. The denied rows print lines of a source file the
//! way the transcripts show agents reading code; the allowed rows read standard input or
//! another kind of file, print by pattern, write rather than print, or only mention a
//! reader in text, and the verdict must never block them.

use super::*;

/// A denial of `reader` naming `path` as the command wrote it.
fn denied(reader: Reader, path: &str) -> Verdict {
    Verdict::Deny(Denial {
        reader,
        path: path.to_owned(),
    })
}

/// A Read call naming `path`, over `span`, made by `caller`.
fn read_call(path: &str, span: ReadSpan, caller: Caller) -> ReadCall {
    ReadCall {
        path: path.to_owned(),
        span,
        caller,
    }
}

/// Holds each command to its verdict, naming the command that broke.
fn assert_rows(rows: &[(&str, Verdict)]) {
    for (command, expected) in rows {
        assert_eq!(&verdict(command), expected, "{command}");
    }
}

/// Holds each command to being let through.
fn assert_allowed(commands: &[&str]) {
    for command in commands {
        assert_eq!(verdict(command), Verdict::Allow, "{command}");
    }
}

mod given_a_sed_line_range_read {
    use super::*;

    #[test]
    fn when_judged_naming_a_source_file_then_it_is_denied_naming_the_file() {
        assert_rows(&[
            (
                "sed -n '10,40p' src/plan.rs",
                denied(Reader::SedRange, "src/plan.rs"),
            ),
            (
                "sed -n 10,40p src/plan.rs",
                denied(Reader::SedRange, "src/plan.rs"),
            ),
            ("sed -n \"1,5p\" a.ts", denied(Reader::SedRange, "a.ts")),
            ("sed -n 12p a.tsx", denied(Reader::SedRange, "a.tsx")),
            ("sed -n '100,$p' a.mts", denied(Reader::SedRange, "a.mts")),
            (
                "sed -n '1,40p;40q' a.cts",
                denied(Reader::SedRange, "a.cts"),
            ),
            ("sed -n '5,+10p' a.rs", denied(Reader::SedRange, "a.rs")),
            ("sed -n 1,5p a.txt b.rs", denied(Reader::SedRange, "b.rs")),
            (
                "sed -n '1,5p' 'my file.rs'",
                denied(Reader::SedRange, "'my file.rs'"),
            ),
            (
                "/usr/bin/sed -n 1,5p a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            ("gsed -n 1,5p a.rs", denied(Reader::SedRange, "a.rs")),
            ("sed -n 1,5p src/*.rs", denied(Reader::SedRange, "src/*.rs")),
        ]);
    }

    #[test]
    fn when_judged_spelling_its_flags_another_way_then_it_is_denied() {
        assert_rows(&[
            (
                "sed -n -e 1,5p -e 9p a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            ("sed -ne 1,5p a.rs", denied(Reader::SedRange, "a.rs")),
            ("sed -En 1,5p a.rs", denied(Reader::SedRange, "a.rs")),
            ("sed --quiet 1,5p a.rs", denied(Reader::SedRange, "a.rs")),
            ("sed --silent 1,5p a.rs", denied(Reader::SedRange, "a.rs")),
            (
                "sed -n --expression=1,5p a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            ("sed -n 1,5p -- a.rs", denied(Reader::SedRange, "a.rs")),
        ]);
    }

    #[test]
    fn when_judged_printing_a_range_a_pattern_bounds_then_it_is_denied() {
        assert_rows(&[
            (
                "sed -n '/fn plan/,/^}/p' a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            (
                "sed -n '/^struct Escalated/,/^}/p' src/main.rs",
                denied(Reader::SedRange, "src/main.rs"),
            ),
            (
                "sed -n '/match envelope.stream_kind {/,/^        }$/p' a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            (
                "sed -n '/let celled/,+2p' a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            (
                "sed -n '/fn plan/,$p' a.ts",
                denied(Reader::SedRange, "a.ts"),
            ),
            ("sed -n '10,/^}/p' a.rs", denied(Reader::SedRange, "a.rs")),
            (
                "sed -n '/a,b/,/c\\/d/p' a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
        ]);
    }

    #[test]
    fn when_judged_reading_a_source_file_through_a_redirect_then_it_is_denied() {
        assert_rows(&[("sed -n 1,5p < a.rs", denied(Reader::SedRange, "a.rs"))]);
    }

    #[test]
    fn when_judged_reading_no_source_file_then_it_is_allowed() {
        assert_allowed(&[
            "rg -n plan src | sed -n 1,40p",
            "git show HEAD:src/plan.rs | sed -n 1,40p",
            "sed -n 1,5p Cargo.toml",
            "sed -n 1,5p \"$F\"",
            "sed -n 1,5p $file",
            "sed -n 1,5p",
        ]);
    }

    #[test]
    fn when_judged_printing_other_than_a_line_range_then_it_is_allowed() {
        assert_allowed(&[
            "sed -n '/^use /p' a.rs",
            "sed -n '/fn plan/p' a.rs",
            "sed -n '/unterminated,5p' a.rs",
            "sed -n '/fn plan/,p' a.rs",
            "sed -n p a.rs",
            "sed 's/a/b/' a.rs",
            "sed '1,5p' a.rs",
            "sed -i 's/a/b/' a.rs",
            "sed -n -i 1,5p a.rs",
            "sed -n -f lines.sed a.rs",
            "sed -n \"$range\" a.rs",
        ]);
    }

    #[test]
    fn when_judged_writing_its_output_to_a_file_then_it_is_allowed() {
        assert_allowed(&[
            "sed -n 1,40p a.rs > slice.rs",
            "sed -n 1,40p a.rs >> slice.rs",
            "sed -n 1,40p a.rs 1> slice.rs",
            "sed -n 1,40p a.rs &> slice.rs",
        ]);
    }
}

mod given_an_awk_line_number_read {
    use super::*;

    #[test]
    fn when_judged_naming_a_source_file_then_it_is_denied_naming_the_file() {
        assert_rows(&[
            (
                "awk 'NR>=1730 && NR<=1742' src/sqlite/tests.rs",
                denied(Reader::AwkRange, "src/sqlite/tests.rs"),
            ),
            ("awk 'NR==323' a.rs", denied(Reader::AwkRange, "a.rs")),
            ("awk 'FNR<=40' a.ts", denied(Reader::AwkRange, "a.ts")),
            ("awk 'NR>1' a.rs", denied(Reader::AwkRange, "a.rs")),
            (
                "awk 'NR < 5 || NR > 90' a.rs",
                denied(Reader::AwkRange, "a.rs"),
            ),
            ("awk '40>=NR' a.rs", denied(Reader::AwkRange, "a.rs")),
            (
                "awk 'NR>=10 && NR<=16 {print NR\": \"$0}' a.rs",
                denied(Reader::AwkRange, "a.rs"),
            ),
            (
                "awk 'NR==5 { print; exit }' a.rs",
                denied(Reader::AwkRange, "a.rs"),
            ),
            (
                "awk 'NR<=9 {print $0}' a.rs",
                denied(Reader::AwkRange, "a.rs"),
            ),
            ("gawk 'NR<=20' a.rs", denied(Reader::AwkRange, "a.rs")),
            ("awk -F: 'NR<=20' a.rs", denied(Reader::AwkRange, "a.rs")),
            (
                "awk -v total=0 'NR<=20' n=1 a.rs",
                denied(Reader::AwkRange, "a.rs"),
            ),
            ("awk -- 'NR<=20' a.rs", denied(Reader::AwkRange, "a.rs")),
        ]);
    }

    #[test]
    fn when_judged_reading_a_source_file_through_a_redirect_then_it_is_denied() {
        assert_rows(&[("awk 'NR<=20' < a.rs", denied(Reader::AwkRange, "a.rs"))]);
    }

    #[test]
    fn when_judged_reading_no_source_file_then_it_is_allowed() {
        assert_allowed(&[
            "cargo test 2>&1 | awk 'NR<=20'",
            "awk 'NR<=20' Cargo.toml",
            "awk 'NR<=20' \"$F\"",
            "awk 'NR<=20'",
        ]);
    }

    #[test]
    fn when_judged_selecting_other_than_by_line_number_then_it_is_allowed() {
        assert_allowed(&[
            "awk '/fn plan/' a.rs",
            "awk '{print $1}' a.rs",
            "awk 'NR<=n' a.rs",
            "awk 'NR>=10 {sum += $1} END {print sum}' a.rs",
            "awk 'NR<=20 {print $2}' a.rs",
            "awk 'NR<=20 {print NR}' a.rs",
            "awk 'NR<=20 {}' a.rs",
            "awk '{sub(/pub /, \"pub(crate) \"); print}' a.rs",
            "awk -f lines.awk a.rs",
            "awk \"$program\" a.rs",
        ]);
    }

    #[test]
    fn when_judged_writing_its_output_to_a_file_then_it_is_allowed() {
        assert_allowed(&["awk 'NR<63' a.rs > head.rs", "awk 'NR<=20' a.rs >> part.rs"]);
    }
}

mod given_head_tail_or_cat {
    use super::*;

    #[test]
    fn when_judged_naming_a_source_file_then_it_is_denied_as_that_reader() {
        assert_rows(&[
            ("head -40 src/main.rs", denied(Reader::Head, "src/main.rs")),
            ("head -n 5 a.rs", denied(Reader::Head, "a.rs")),
            ("head -c 100 a.ts", denied(Reader::Head, "a.ts")),
            ("tail -n 20 src/lib.rs", denied(Reader::Tail, "src/lib.rs")),
            ("tail -n +20 a.rs", denied(Reader::Tail, "a.rs")),
            ("cat src/plan.rs", denied(Reader::Cat, "src/plan.rs")),
            ("cat -n src/plan.rs", denied(Reader::Cat, "src/plan.rs")),
            ("cat a.rs b.rs", denied(Reader::Cat, "a.rs")),
            ("cat README.md src/*.rs", denied(Reader::Cat, "src/*.rs")),
            ("cat < a.rs", denied(Reader::Cat, "a.rs")),
            ("cat a.rs 2> errors.txt", denied(Reader::Cat, "a.rs")),
            ("cat src/'plan'.rs", denied(Reader::Cat, "src/'plan'.rs")),
        ]);
    }

    #[test]
    fn when_judged_following_a_file_then_it_is_allowed() {
        assert_allowed(&[
            "tail -f a.rs",
            "tail -F a.rs",
            "tail -fn 20 a.rs",
            "tail --follow=name a.rs",
        ]);
    }

    #[test]
    fn when_judged_reading_no_source_file_then_it_is_allowed() {
        assert_allowed(&[
            "head -5 README.md",
            "cat flake.nix Cargo.toml",
            "cargo test 2>&1 | tail -20",
            "cat",
            "cat -",
            "cat \"$S/plan.rs\"",
            "cat $k/mod.rs",
            "cat ${k}/mod.rs",
            "tail -5 $b/$k/events/mod.rs",
        ]);
    }

    #[test]
    fn when_judged_writing_its_output_to_a_file_then_it_is_allowed() {
        assert_allowed(&[
            "cat a.rs > b.rs",
            "cat a.rs >> b.rs",
            "cat a.rs > /dev/null",
            "{ head -n 3 $f; cat parts.rs; tail -n +9 $f; } > $f.new",
            "for f in a; do cat a.rs; done > out",
            "cat a.rs | sort > out",
            "{ a && cat b.rs; } > out",
            "b && cat a.rs > out",
            "cat > a.rs <<'EOF'\nfn main() {}\nEOF",
            "cat >> a.rs <<'EOF'\nfn main() {}\nEOF",
        ]);
    }
}

mod given_a_reader_inside_a_larger_command {
    use super::*;

    #[test]
    fn when_judged_then_it_is_denied_wherever_it_stands() {
        assert_rows(&[
            (
                "cd ~/projects/excerpt && sed -n 1,5p src/main.rs | head -3",
                denied(Reader::SedRange, "src/main.rs"),
            ),
            (
                "rg -n plan src; sed -n 1,5p a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            ("echo ---; head -30 a.ts", denied(Reader::Head, "a.ts")),
            (
                "for f in x; do head -3 a.rs; done",
                denied(Reader::Head, "a.rs"),
            ),
            (
                "if true; then cat a.ts 2>/dev/null; fi",
                denied(Reader::Cat, "a.ts"),
            ),
            ("echo $(cat a.rs)", denied(Reader::Cat, "a.rs")),
            ("cat <<EOF\n$(head a.rs)\nEOF", denied(Reader::Head, "a.rs")),
            ("(cd src && cat plan.rs)", denied(Reader::Cat, "plan.rs")),
            ("{ cat a.rs; echo; }", denied(Reader::Cat, "a.rs")),
            ("(cat a.rs) 2> errors.txt", denied(Reader::Cat, "a.rs")),
            (
                "a && sed -n 1,10p x.rs && just check > log 2>&1",
                denied(Reader::SedRange, "x.rs"),
            ),
            ("cat a.rs && b > out", denied(Reader::Cat, "a.rs")),
            ("cat a.rs | sort && b > out", denied(Reader::Cat, "a.rs")),
        ]);
    }

    #[test]
    fn when_judged_behind_a_prefix_then_the_program_after_it_is_judged() {
        assert_rows(&[
            (
                "LC_ALL=C sed -n 1,5p a.rs",
                denied(Reader::SedRange, "a.rs"),
            ),
            ("sudo head -n 5 a.rs", denied(Reader::Head, "a.rs")),
            ("env LC_ALL=C cat a.rs", denied(Reader::Cat, "a.rs")),
            ("timeout 5 cat a.rs", denied(Reader::Cat, "a.rs")),
            ("command cat a.rs", denied(Reader::Cat, "a.rs")),
        ]);
    }
}

mod given_text_that_only_mentions_a_reader {
    use super::*;

    #[test]
    fn when_judged_then_it_is_allowed() {
        assert_allowed(&[
            "echo 'sed -n 1,5p a.rs'",
            "echo \"cat a.rs\"",
            "# head a.rs\nls",
            "git commit -m \"Stop running head a.rs\"",
            "cat > notes.md <<'EOF'\nsed -n 1,5p a.rs\nEOF",
            "git commit -F - <<'EOF'\nRead cat a.rs no more\nEOF",
            "rg 'sed -n 1,5p a.rs' tests",
        ]);
    }
}

mod given_other_programs {
    use super::*;

    #[test]
    fn when_judged_naming_a_source_file_then_it_is_allowed() {
        assert_allowed(&[
            "excerpt show plan src/plan.rs",
            "excerpt outline src/plan.rs",
            "wc -l src/plan.rs",
            "rg -n plan src/plan.rs",
            "git diff src/plan.rs",
            "rustfmt src/plan.rs",
            "ls src/*.rs | xargs head -5",
        ]);
    }

    #[test]
    fn when_judged_unparseable_then_it_is_allowed() {
        assert_allowed(&["", "   ", "((", "echo 'unterminated"]);
    }
}

mod given_a_read_call {
    use super::*;

    #[test]
    fn when_screened_then_only_a_subagents_whole_read_of_a_code_file_waits_on_its_line_count() {
        let rows = [
            (
                read_call("/p/a.rs", ReadSpan::Whole, Caller::Subagent),
                ReadScreen::LineCountDecides,
            ),
            (
                read_call("/p/a.tsx", ReadSpan::Whole, Caller::Subagent),
                ReadScreen::LineCountDecides,
            ),
            (
                read_call("/p/a.rs", ReadSpan::Whole, Caller::MainThread),
                ReadScreen::Allowed,
            ),
            (
                read_call("/p/a.rs", ReadSpan::Ranged, Caller::Subagent),
                ReadScreen::Allowed,
            ),
            (
                read_call("/p/README.md", ReadSpan::Whole, Caller::Subagent),
                ReadScreen::Allowed,
            ),
        ];
        for (call, screen) in rows {
            assert_eq!(read_screen(&call), screen, "{call:?}");
        }
    }

    #[test]
    fn when_judged_past_the_bound_then_it_is_denied_naming_the_file_as_one_shell_word() {
        let past_bound = SUBAGENT_READ_LINES_MAX + 1;
        let rows = [
            ("/p/a.rs", "/p/a.rs"),
            ("/p/my plan.rs", "'/p/my plan.rs'"),
            ("/p/it's.rs", "'/p/it'\\''s.rs'"),
            ("/p/$HOME.rs", "'/p/$HOME.rs'"),
        ];
        for (path, word) in rows {
            let call = read_call(path, ReadSpan::Whole, Caller::Subagent);

            let judged = read_verdict(&call, past_bound);

            assert_eq!(judged, denied(Reader::WholeFileRead, word), "{path}");
        }
    }

    #[test]
    fn when_judged_at_or_under_the_bound_then_it_is_allowed() {
        let call = read_call("/p/a.rs", ReadSpan::Whole, Caller::Subagent);
        for line_count in [0, 1, SUBAGENT_READ_LINES_MAX] {
            assert_eq!(
                read_verdict(&call, line_count),
                Verdict::Allow,
                "{line_count}"
            );
        }
    }

    #[test]
    fn when_its_span_is_read_then_a_named_offset_or_limit_ranges_it_and_a_null_names_none() {
        let rows = [
            (
                serde_json::json!({ "file_path": "/p/a.rs" }),
                ReadSpan::Whole,
            ),
            (
                serde_json::json!({ "file_path": "/p/a.rs", "offset": 10 }),
                ReadSpan::Ranged,
            ),
            (
                serde_json::json!({ "file_path": "/p/a.rs", "limit": 5 }),
                ReadSpan::Ranged,
            ),
            (
                serde_json::json!({ "file_path": "/p/a.rs", "offset": null, "limit": null }),
                ReadSpan::Whole,
            ),
        ];
        for (tool_input, span) in rows {
            assert_eq!(read_span(&tool_input), span, "{tool_input}");
        }
    }
}

mod given_file_bytes {
    use super::*;

    #[test]
    fn when_counted_then_a_last_line_without_a_newline_counts() {
        let rows: [(&[u8], usize); 6] = [
            (b"", 0),
            (b"a", 1),
            (b"a\n", 1),
            (b"a\nb", 2),
            (b"a\nb\n", 2),
            (b"\n\n", 2),
        ];
        for (bytes, count) in rows {
            assert_eq!(
                line_count(bytes),
                count,
                "{}",
                String::from_utf8_lossy(bytes)
            );
        }
    }
}

mod given_a_hook_input {
    use super::*;

    #[test]
    fn when_read_from_a_bash_call_then_its_command_is_returned() {
        let input = br#"{"session_id":"s","tool_name":"Bash","tool_input":{"command":"sed -n 1,5p a.rs","description":"d"}}"#;

        assert_eq!(
            hook_input(input),
            Ok(HookInput::Bash("sed -n 1,5p a.rs".to_owned()))
        );
    }

    #[test]
    fn when_read_from_another_tool_then_it_is_passed_over() {
        let input = br#"{"tool_name":"Grep","tool_input":{"pattern":"plan","path":"a.rs"}}"#;

        assert_eq!(hook_input(input), Ok(HookInput::OtherTool));
    }

    #[test]
    fn when_read_from_a_read_call_then_its_file_span_and_caller_are_returned() {
        let rows: [(&[u8], ReadCall); 5] = [
            (
                br#"{"tool_name":"Read","tool_input":{"file_path":"/p/a.rs"},"agent_id":"a1","agent_type":"Explore"}"#,
                read_call("/p/a.rs", ReadSpan::Whole, Caller::Subagent),
            ),
            (
                br#"{"tool_name":"Read","tool_input":{"file_path":"/p/a.rs","offset":10}}"#,
                read_call("/p/a.rs", ReadSpan::Ranged, Caller::MainThread),
            ),
            (
                br#"{"tool_name":"Read","tool_input":{"file_path":"/p/a.rs","limit":40},"agent_id":"a1"}"#,
                read_call("/p/a.rs", ReadSpan::Ranged, Caller::Subagent),
            ),
            (
                br#"{"tool_name":"Read","tool_input":{"file_path":"/p/a.rs","offset":null,"limit":null},"agent_id":"a1"}"#,
                read_call("/p/a.rs", ReadSpan::Whole, Caller::Subagent),
            ),
            (
                br#"{"tool_name":"Read","tool_input":{"file_path":"/p/a.rs"},"agent_type":"reviewer"}"#,
                read_call("/p/a.rs", ReadSpan::Whole, Caller::MainThread),
            ),
        ];
        for (input, call) in rows {
            assert_eq!(
                hook_input(input),
                Ok(HookInput::Read(call)),
                "{}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn when_read_from_a_read_call_naming_no_file_then_the_missing_field_is_named() {
        let inputs: [&[u8]; 3] = [
            br#"{"tool_name":"Read"}"#,
            br#"{"tool_name":"Read","tool_input":{}}"#,
            br#"{"tool_name":"Read","tool_input":{"file_path":7}}"#,
        ];
        for input in inputs {
            assert_eq!(
                hook_input(input),
                Err(HookInputFault::NoFilePath),
                "{}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn when_read_lacking_a_field_then_the_missing_field_is_named() {
        let rows: [(&[u8], HookInputFault); 4] = [
            (
                br#"{"tool_input":{"command":"ls"}}"#,
                HookInputFault::NoToolName,
            ),
            (br#"{"tool_name":7}"#, HookInputFault::NoToolName),
            (br#"{"tool_name":"Bash"}"#, HookInputFault::NoCommand),
            (
                br#"{"tool_name":"Bash","tool_input":{"command":["ls"]}}"#,
                HookInputFault::NoCommand,
            ),
        ];
        for (input, fault) in rows {
            assert_eq!(
                hook_input(input),
                Err(fault),
                "{}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn when_read_holding_no_json_then_it_is_refused_as_not_json() {
        assert!(matches!(
            hook_input(b"sed -n 1,5p a.rs"),
            Err(HookInputFault::NotJson(_))
        ));
    }
}
