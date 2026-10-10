//! The name a reader asks for: how its text parses, and which items it answers.

use super::*;
use crate::item::{Kind, Lines};

fn item(kind: Kind, path: &[&str]) -> Item {
    let path = path.iter().map(|segment| (*segment).to_owned()).collect();
    Item::new(kind, path, Lines::new(1, 1))
}

fn query(text: &str) -> Query {
    let Ok(parsed) = parse(text) else {
        panic!("{text} parses");
    };
    parsed
}

mod given_text {
    use super::*;

    #[test]
    fn when_parsed_holding_a_bare_name_then_it_is_one_segment_of_any_kind() {
        let parsed = query("plan");

        assert_eq!(parsed.kind(), None);
        assert_eq!(parsed.segments(), ["plan"]);
    }

    #[test]
    fn when_parsed_leading_with_a_kind_keyword_then_the_kind_is_split_off() {
        let parsed = query("fn Cursor::new");

        assert_eq!(parsed.kind(), Some(Kind::Fn));
        assert_eq!(parsed.segments(), ["Cursor", "new"]);
    }

    #[test]
    fn when_parsed_holding_a_lone_keyword_then_it_is_a_name() {
        let parsed = query("type");

        assert_eq!(parsed.kind(), None);
        assert_eq!(parsed.segments(), ["type"]);
    }

    #[test]
    fn when_parsed_holding_either_separator_then_both_split_segments() {
        assert_eq!(query("Service.run").segments(), ["Service", "run"]);
        assert_eq!(
            query("given_input::when_run").segments(),
            ["given_input", "when_run"]
        );
    }

    #[test]
    fn when_parsed_holding_a_trait_impl_name_then_its_spacing_is_normalized() {
        let parsed = query("impl  Display   for Cursor");

        assert_eq!(parsed.kind(), Some(Kind::Impl));
        assert_eq!(parsed.segments(), ["Display for Cursor"]);
    }

    #[test]
    fn when_parsed_holding_only_blanks_then_it_is_refused() {
        assert_eq!(parse("  "), Err(QueryFault::Empty));
    }

    #[test]
    fn when_parsed_holding_an_empty_segment_then_it_is_refused() {
        assert_eq!(
            parse("Cursor::"),
            Err(QueryFault::EmptySegment("Cursor::".to_owned()))
        );
    }
}

mod given_an_item {
    use super::*;

    #[test]
    fn when_asked_for_by_its_own_name_then_it_answers() {
        assert!(query("new").answers(&item(Kind::Fn, &["Cursor", "new"])));
    }

    #[test]
    fn when_asked_for_by_its_qualified_name_then_it_answers() {
        assert!(query("Cursor::new").answers(&item(Kind::Fn, &["Cursor", "new"])));
    }

    #[test]
    fn when_asked_for_by_a_name_its_own_only_contains_then_it_does_not_answer() {
        assert!(!query("new").answers(&item(Kind::Fn, &["Cursor", "renew"])));
    }

    #[test]
    fn when_asked_for_under_another_qualifier_then_it_does_not_answer() {
        assert!(!query("Plan::new").answers(&item(Kind::Fn, &["Cursor", "new"])));
    }

    #[test]
    fn when_asked_for_by_a_path_longer_than_its_own_then_it_does_not_answer() {
        assert!(!query("text::Cursor::new").answers(&item(Kind::Fn, &["Cursor", "new"])));
    }

    #[test]
    fn when_asked_for_as_another_kind_then_it_does_not_answer() {
        assert!(!query("struct Cursor").answers(&item(Kind::Impl, &["Cursor"])));
    }

    #[test]
    fn when_asked_for_as_its_own_kind_then_it_answers() {
        assert!(
            query("impl Display for Cursor").answers(&item(Kind::Impl, &["Display for Cursor"]))
        );
    }
}

mod given_a_file_text {
    use super::*;
    use crate::item::outline;
    use crate::language::Language;

    #[test]
    fn when_screened_lacking_a_word_of_the_name_then_it_cannot_answer() {
        assert!(!query("Cursor::new").may_answer_in("impl Plan { fn new() {} }"));
    }

    #[test]
    fn when_screened_holding_every_word_of_the_name_then_it_may_answer() {
        assert!(query("Cursor::new").may_answer_in("impl Cursor { fn new() {} }"));
    }

    #[test]
    fn when_screened_then_every_item_a_name_answers_passes() {
        let rows = [
            (
                Language::Rust,
                "impl fmt::Display for Cursor<'_> { fn fmt(&self) {} }\n",
                "impl Display for Cursor",
            ),
            (
                Language::Rust,
                "impl fmt::Display for Cursor<'_> { fn fmt(&self) {} }\n",
                "Cursor::fmt",
            ),
            (
                Language::Rust,
                "mod given_input {\n    fn when_run() {}\n}\n",
                "given_input::when_run",
            ),
            (
                Language::TypeScript,
                "export default function () {}\n",
                "function default",
            ),
            (
                Language::TypeScript,
                "export class Service { run() {} }\n",
                "Service.run",
            ),
        ];
        for (language, text, name) in rows {
            let parsed = query(name);
            let answered = outline(language, text)
                .items
                .iter()
                .any(|item| parsed.answers(item));
            assert!(answered, "{name} answers an item of {text}");
            assert!(
                parsed.may_answer_in(text),
                "{name} passes the screen of {text}"
            );
        }
    }
}
