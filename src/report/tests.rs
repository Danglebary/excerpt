//! Every message excerpt prints, built from values a test constructs. These cases read
//! the rendered text, since a message is what this module produces.

use super::*;
use crate::item::{Item, Kind, Lines};

fn item(kind: Kind, path: &[&str], first: usize, last: usize) -> Item {
    let path = path.iter().map(|segment| (*segment).to_owned()).collect();
    Item::new(kind, path, Lines::new(first, last))
}

fn outlined(items: Vec<Item>, first_error_line: Option<usize>) -> Outline {
    Outline {
        items,
        first_error_line,
    }
}

/// `outline` whole, with no item left out.
fn whole(outline: Outline) -> Abridgment {
    Abridgment {
        outline,
        left_out: 0,
    }
}

/// A Rust file of the outline listing reads.
fn rust_file<'file>(path: &'file str, abridgment: &'file Abridgment) -> File<'file> {
    File {
        path,
        entry: Entry::Outlined {
            language: Language::Rust,
            abridgment,
        },
    }
}

/// The outline of `files` as the shell lists it: each file in order until the listing
/// breaks, then the count of the files left.
fn listed(files: &[File<'_>], items_max: usize) -> String {
    let mut listing = Listing::new(items_max);
    let mut taken: usize = 0;
    for file in files {
        taken = taken.checked_add(1).unwrap();
        if listing.push(file).is_break() {
            break;
        }
    }
    listing.text(files.len().checked_sub(taken).unwrap())
}

mod given_an_item {
    use super::*;

    #[test]
    fn when_headed_then_its_path_lines_kind_and_qualified_name_are_one_line() {
        let method = item(Kind::Fn, &["Cursor", "new"], 251, 260);

        assert_eq!(
            header("src/plan/mod.rs", Language::Rust, &method),
            "src/plan/mod.rs:251-260 fn Cursor::new"
        );
    }

    #[test]
    fn when_printed_then_its_lines_follow_its_header_numbered_as_the_read_tool_numbers_them() {
        let text = "fn a() {}\nfn b() {\n    1\n}\nfn c() {}\n";
        let middle = item(Kind::Fn, &["b"], 2, 4);

        assert_eq!(
            item_text("src/lib.rs", Language::Rust, &middle, text),
            "src/lib.rs:2-4 fn b\n     2\tfn b() {\n     3\t    1\n     4\t}\n"
        );
    }
}

mod given_outlines {
    use super::*;

    #[test]
    fn when_rendered_then_each_file_lists_its_items_indented_by_nesting() {
        let rust = whole(outlined(
            vec![
                item(Kind::Impl, &["Cursor"], 3, 9),
                item(Kind::Fn, &["Cursor", "new"], 4, 6),
                item(Kind::Fn, &["plan"], 11, 20),
            ],
            None,
        ));
        let typescript = whole(outlined(
            vec![item(Kind::Method, &["Service", "run"], 5, 7)],
            None,
        ));
        let files = [
            rust_file("src/plan.rs", &rust),
            File {
                path: "web/service.ts",
                entry: Entry::Outlined {
                    language: Language::TypeScript,
                    abridgment: &typescript,
                },
            },
        ];

        assert_eq!(
            listed(&files, OUTLINE_ITEMS_MAX),
            "\
src/plan.rs
  3-9 impl Cursor
    4-6 fn Cursor::new
  11-20 fn plan
web/service.ts
    5-7 method Service.run
"
        );
    }

    #[test]
    fn when_rendered_holding_a_syntax_error_then_the_file_says_where() {
        let broken = whole(outlined(vec![item(Kind::Fn, &["ok"], 1, 1)], Some(3)));
        let files = [rust_file("src/lib.rs", &broken)];

        assert_eq!(
            listed(&files, OUTLINE_ITEMS_MAX),
            "src/lib.rs\n  1-1 fn ok\n  (a syntax error at line 3; items past it may be missing)\n"
        );
    }

    #[test]
    fn when_rendered_holding_no_items_then_the_file_says_so() {
        let empty = whole(outlined(Vec::new(), None));
        let files = [rust_file("src/lib.rs", &empty)];

        assert_eq!(
            listed(&files, OUTLINE_ITEMS_MAX),
            "src/lib.rs\n  (no items)\n"
        );
    }

    #[test]
    fn when_the_bound_falls_inside_a_file_then_its_items_past_it_and_the_later_files_are_counted() {
        let first = whole(outlined(
            vec![item(Kind::Fn, &["a"], 1, 1), item(Kind::Fn, &["b"], 2, 2)],
            None,
        ));
        let second = whole(outlined(vec![item(Kind::Fn, &["c"], 1, 1)], None));
        let files = [
            rust_file("a.rs", &first),
            rust_file("b.rs", &second),
            File {
                path: "c.rs",
                entry: Entry::NotText,
            },
        ];

        assert_eq!(
            listed(&files, 1),
            "a.rs\n  1-1 fn a\n  (1 more item left out)\n2 more files left out; outline a narrower path\n"
        );
    }

    #[test]
    fn when_the_bound_falls_on_a_file_s_last_item_then_only_the_later_files_are_counted() {
        let first = whole(outlined(vec![item(Kind::Fn, &["a"], 1, 1)], None));
        let second = whole(outlined(vec![item(Kind::Fn, &["c"], 1, 1)], None));
        let files = [rust_file("a.rs", &first), rust_file("b.rs", &second)];

        assert_eq!(
            listed(&files, 1),
            "a.rs\n  1-1 fn a\n1 more file left out; outline a narrower path\n"
        );
    }

    #[test]
    fn when_the_last_file_reaches_the_bound_then_nothing_is_counted() {
        let only = whole(outlined(vec![item(Kind::Fn, &["a"], 1, 1)], None));
        let files = [rust_file("a.rs", &only)];

        assert_eq!(listed(&files, 1), "a.rs\n  1-1 fn a\n");
    }

    #[test]
    fn when_a_file_s_abridgment_left_items_out_then_its_note_counts_them() {
        let abridged = Abridgment {
            outline: outlined(vec![item(Kind::Fn, &["a"], 1, 1)], Some(9)),
            left_out: 5,
        };
        let files = [rust_file("a.rs", &abridged)];

        assert_eq!(
            listed(&files, OUTLINE_ITEMS_MAX),
            "a.rs\n  1-1 fn a\n  (5 more items left out)\n  (a syntax error at line 9; items past it may be missing)\n"
        );
    }

    #[test]
    fn when_files_are_pushed_then_the_listing_breaks_on_the_one_reaching_the_bound() {
        let pair = whole(outlined(
            vec![item(Kind::Fn, &["a"], 1, 1), item(Kind::Fn, &["b"], 2, 2)],
            None,
        ));
        let file = rust_file("a.rs", &pair);
        let mut listing = Listing::new(3);

        let flows = [listing.push(&file), listing.push(&file)];

        assert_eq!(flows, [ControlFlow::Continue(()), ControlFlow::Break(())]);
    }

    #[test]
    fn when_rendered_holding_a_file_too_large_to_read_then_it_is_named_as_skipped() {
        let files = [File {
            path: "src/generated.rs",
            entry: Entry::TooLarge {
                bytes: 20_000_000,
                bytes_max: 16_777_216,
            },
        }];

        assert_eq!(
            listed(&files, OUTLINE_ITEMS_MAX),
            "src/generated.rs\n  (skipped: 20000000 bytes, more than the 16777216 excerpt reads)\n"
        );
    }
}

mod given_an_outline {
    use super::*;

    #[test]
    fn when_abridged_past_the_bound_then_its_first_items_are_kept_and_the_rest_counted() {
        let items = vec![
            item(Kind::Fn, &["a"], 1, 1),
            item(Kind::Fn, &["b"], 2, 2),
            item(Kind::Fn, &["c"], 3, 3),
        ];

        let abridgment = abridge(outlined(items, Some(7)), 2);

        assert_eq!(
            abridgment,
            Abridgment {
                outline: outlined(
                    vec![item(Kind::Fn, &["a"], 1, 1), item(Kind::Fn, &["b"], 2, 2)],
                    Some(7)
                ),
                left_out: 1,
            }
        );
    }

    #[test]
    fn when_abridged_within_the_bound_then_it_is_kept_whole() {
        let items = vec![item(Kind::Fn, &["a"], 1, 1), item(Kind::Fn, &["b"], 2, 2)];

        let abridgment = abridge(outlined(items.clone(), None), 2);

        assert_eq!(abridgment, whole(outlined(items, None)));
    }
}

mod given_refusals {
    use super::*;

    #[test]
    fn when_several_items_answer_then_each_is_listed_up_to_the_bound() {
        let headers: Vec<String> = (1..=LISTED_MATCHES_MAX + 2)
            .map(|line| format!("src/lib.rs:{line}-{line} fn new"))
            .collect();

        let message = several("new", &headers, headers.len());

        assert!(
            message.starts_with("excerpt: 22 items answer `new`;"),
            "{message}"
        );
        assert!(
            message.contains("\n  src/lib.rs:20-20 fn new\n"),
            "{message}"
        );
        assert!(!message.contains("src/lib.rs:21-21"), "{message}");
        assert!(message.ends_with("  and 2 more\n"), "{message}");
    }

    #[test]
    fn when_more_items_answer_than_were_kept_then_the_unkept_are_counted_among_the_rest() {
        let headers: Vec<String> = (1..=LISTED_MATCHES_MAX)
            .map(|line| format!("src/lib.rs:{line}-{line} fn new"))
            .collect();
        let count = LISTED_MATCHES_MAX + 10;

        let message = several("new", &headers, count);

        assert!(
            message.starts_with("excerpt: 30 items answer `new`;"),
            "{message}"
        );
        assert!(message.ends_with("  and 10 more\n"), "{message}");
    }

    #[test]
    fn when_an_item_is_too_long_then_its_members_are_offered_instead() {
        let members = ["src/lib.rs:2-400 fn tests::a".to_owned()];

        let message = too_long("src/lib.rs:1-1200 mod tests", 1_200, &members);

        assert_eq!(
            message,
            "excerpt: src/lib.rs:1-1200 mod tests spans 1200 lines, more than the 1000 a show prints; show one of its members:\n  src/lib.rs:2-400 fn tests::a\n"
        );
    }

    #[test]
    fn when_an_item_without_members_is_too_long_then_a_range_read_is_offered() {
        let message = too_long("src/lib.rs:1-1200 fn huge", 1_200, &[]);

        assert_eq!(
            message,
            "excerpt: src/lib.rs:1-1200 fn huge spans 1200 lines, more than the 1000 a show prints; read a range of it with the Read tool\n"
        );
    }
}

mod given_a_guard_denial {
    use super::*;
    use crate::cli::{Command, parse_arguments};
    use crate::guard::{Denial, Reader};

    fn denial(reader: Reader, path: &str) -> Denial {
        Denial {
            reader,
            path: path.to_owned(),
        }
    }

    #[test]
    fn when_rendered_then_it_names_the_read_and_offers_outline_and_show_on_its_path() {
        let message = guard_denial(&denial(Reader::SedRange, "'my plan.rs'"));

        assert_eq!(
            message,
            "excerpt guard: blocked a `sed -n` line-range read of 'my plan.rs'.\n\
             Read code by item instead:\n\
             \x20 excerpt outline 'my plan.rs'\n\
             \x20 excerpt show NAME 'my plan.rs'\n\
             `outline` lists every item in the file with the lines it spans; `show` prints \
             the one NAME names, whole, NAME written as the outline prints it. A line no item \
             holds, such as an import or the line a compiler error names, is read with the \
             Read tool's offset and limit.\n"
        );
    }

    #[test]
    fn when_rendered_for_each_reader_then_it_names_that_reader() {
        let rows = [
            (Reader::SedRange, "a `sed -n` line-range read"),
            (Reader::AwkRange, "an `awk` line-number read"),
            (Reader::WholeFileRead, "a subagent's whole-file Read"),
            (Reader::Head, "a `head` read"),
            (Reader::Tail, "a `tail` read"),
            (Reader::Cat, "a `cat` read"),
        ];
        for (reader, read) in rows {
            let message = guard_denial(&denial(reader, "src/plan.rs"));

            let headline = format!("excerpt guard: blocked {read} of src/plan.rs.\n");
            assert!(message.starts_with(&headline), "{message}");
        }
    }

    #[test]
    fn when_rendered_then_every_command_it_offers_parses_against_the_grammar() {
        let message = guard_denial(&denial(Reader::Cat, "src/plan.rs"));

        let offered: Vec<Vec<String>> = message
            .lines()
            .filter_map(|line| line.strip_prefix("  excerpt "))
            .map(|arguments| arguments.split_whitespace().map(str::to_owned).collect())
            .collect();
        assert_eq!(offered.len(), 2, "{message}");
        for arguments in &offered {
            let parsed = parse_arguments(arguments);
            assert!(parsed.is_ok(), "{arguments:?}");
            if let Ok(Command::Show { name, .. }) = parsed {
                assert!(crate::query::parse(&name).is_ok(), "{name}");
            }
        }
    }
}

mod given_a_hook_input_it_cannot_use {
    use super::*;
    use crate::guard::HookInputFault;

    #[test]
    fn when_unreadable_then_the_message_says_why() {
        assert_eq!(
            hook_input_unreadable("broken pipe"),
            "excerpt guard: could not read the hook input: broken pipe\n"
        );
    }

    #[test]
    fn when_malformed_then_the_message_names_what_it_lacks() {
        let rows = [
            (
                HookInputFault::NotJson("expected value at line 1 column 1".to_owned()),
                "it is not JSON: expected value at line 1 column 1",
            ),
            (HookInputFault::NoToolName, "it holds no `tool_name`"),
            (
                HookInputFault::NoCommand,
                "a Bash call holds no `tool_input.command`",
            ),
            (
                HookInputFault::NoFilePath,
                "a Read call holds no `tool_input.file_path`",
            ),
        ];
        for (fault, detail) in rows {
            assert_eq!(
                hook_input_fault(&fault),
                format!("excerpt guard: the hook input is not what Claude Code sends: {detail}\n")
            );
        }
    }
}

mod given_a_usage_fault {
    use super::*;

    #[test]
    fn when_guard_is_given_an_argument_then_the_message_says_it_takes_none() {
        assert_eq!(
            usage_fault(&UsageFault::GuardArgument("--strict".to_owned())),
            "excerpt: guard takes no arguments, and `--strict` is one\nRun `excerpt --help` for the grammar.\n"
        );
    }

    #[test]
    fn when_the_command_is_unknown_then_every_command_is_named() {
        assert_eq!(
            usage_fault(&UsageFault::UnknownCommand("find".to_owned())),
            "excerpt: `find` is not a command; the commands are outline, show, and guard\nRun `excerpt --help` for the grammar.\n"
        );
    }
}
