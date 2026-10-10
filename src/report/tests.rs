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
        let rust = outlined(
            vec![
                item(Kind::Impl, &["Cursor"], 3, 9),
                item(Kind::Fn, &["Cursor", "new"], 4, 6),
                item(Kind::Fn, &["plan"], 11, 20),
            ],
            None,
        );
        let typescript = outlined(vec![item(Kind::Method, &["Service", "run"], 5, 7)], None);
        let files = [
            File {
                path: "src/plan.rs",
                entry: Entry::Outlined {
                    language: Language::Rust,
                    outline: &rust,
                },
            },
            File {
                path: "web/service.ts",
                entry: Entry::Outlined {
                    language: Language::TypeScript,
                    outline: &typescript,
                },
            },
        ];

        assert_eq!(
            outline_text(&files, OUTLINE_ITEMS_MAX),
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
        let broken = outlined(vec![item(Kind::Fn, &["ok"], 1, 1)], Some(3));
        let files = [File {
            path: "src/lib.rs",
            entry: Entry::Outlined {
                language: Language::Rust,
                outline: &broken,
            },
        }];

        assert_eq!(
            outline_text(&files, OUTLINE_ITEMS_MAX),
            "src/lib.rs\n  1-1 fn ok\n  (a syntax error at line 3; items past it may be missing)\n"
        );
    }

    #[test]
    fn when_rendered_holding_no_items_then_the_file_says_so() {
        let empty = outlined(Vec::new(), None);
        let files = [File {
            path: "src/lib.rs",
            entry: Entry::Outlined {
                language: Language::Rust,
                outline: &empty,
            },
        }];

        assert_eq!(
            outline_text(&files, OUTLINE_ITEMS_MAX),
            "src/lib.rs\n  (no items)\n"
        );
    }

    #[test]
    fn when_rendered_past_the_bound_then_the_rest_are_counted_and_no_later_file_is_named() {
        let first = outlined(
            vec![item(Kind::Fn, &["a"], 1, 1), item(Kind::Fn, &["b"], 2, 2)],
            None,
        );
        let second = outlined(vec![item(Kind::Fn, &["c"], 1, 1)], None);
        let files = [
            File {
                path: "a.rs",
                entry: Entry::Outlined {
                    language: Language::Rust,
                    outline: &first,
                },
            },
            File {
                path: "b.rs",
                entry: Entry::Outlined {
                    language: Language::Rust,
                    outline: &second,
                },
            },
        ];

        assert_eq!(
            outline_text(&files, 1),
            "a.rs\n  1-1 fn a\n2 more items left out; outline a narrower path\n"
        );
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
            outline_text(&files, OUTLINE_ITEMS_MAX),
            "src/generated.rs\n  (skipped: 20000000 bytes, more than the 16777216 excerpt reads)\n"
        );
    }
}

mod given_refusals {
    use super::*;

    #[test]
    fn when_several_items_answer_then_each_is_listed_up_to_the_bound() {
        let headers: Vec<String> = (1..=LISTED_MATCHES_MAX + 2)
            .map(|line| format!("src/lib.rs:{line}-{line} fn new"))
            .collect();

        let message = several("new", &headers);

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
