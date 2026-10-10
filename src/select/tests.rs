//! Which of the items answering a name `show` prints, each case a number of matches and
//! whether `--all` was passed.

use super::*;

fn matches(count: usize) -> Vec<Match> {
    (0..count).map(|item| Match { file: 0, item }).collect()
}

mod given_matches {
    use super::*;

    #[test]
    fn when_none_answer_then_the_name_is_missing() {
        assert_eq!(select(Vec::new(), false), Selection::Missing);
    }

    #[test]
    fn when_one_answers_then_it_is_printed() {
        assert_eq!(select(matches(1), false), Selection::Print(matches(1)));
    }

    #[test]
    fn when_several_answer_without_all_then_they_are_listed_instead() {
        assert_eq!(select(matches(2), false), Selection::Several(matches(2)));
    }

    #[test]
    fn when_several_answer_with_all_then_each_is_printed() {
        assert_eq!(select(matches(2), true), Selection::Print(matches(2)));
    }

    #[test]
    fn when_more_answer_than_all_prints_then_they_are_counted() {
        let count = PRINTED_MATCHES_MAX + 1;

        assert_eq!(select(matches(count), true), Selection::TooMany(count));
    }

    #[test]
    fn when_exactly_as_many_answer_as_all_prints_then_each_is_printed() {
        assert_eq!(
            select(matches(PRINTED_MATCHES_MAX), true),
            Selection::Print(matches(PRINTED_MATCHES_MAX))
        );
    }
}

mod given_an_item {
    use super::*;
    use crate::item::{Item, Kind, Lines};

    fn spanning(count: usize) -> Item {
        Item::new(Kind::Fn, vec!["long".to_owned()], Lines::new(1, count))
    }

    #[test]
    fn when_it_spans_the_most_lines_a_show_prints_then_it_is_printable() {
        assert!(printable(&spanning(ITEM_LINES_MAX)));
    }

    #[test]
    fn when_it_spans_more_lines_than_a_show_prints_then_it_is_not() {
        assert!(!printable(&spanning(ITEM_LINES_MAX + 1)));
    }
}
