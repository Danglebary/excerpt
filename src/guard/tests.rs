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
    fn when_judged_printing_other_than_by_line_number_then_it_is_allowed() {
        assert_allowed(&[
            "sed -n '/fn plan/,/^}/p' a.rs",
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
        let input = br#"{"tool_name":"Read","tool_input":{"file_path":"a.rs"}}"#;

        assert_eq!(hook_input(input), Ok(HookInput::OtherTool));
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
