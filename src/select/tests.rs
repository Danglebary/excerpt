//! Which of the items answering a name `show` keeps and prints, each case the answering
//! items of the files read and whether `--all` was passed.

use super::*;

fn matches(count: usize) -> Vec<Match> {
    (0..count).map(|item| Match { file: 0, item }).collect()
}

/// `count` items of one file found.
fn found(count: usize) -> Found {
    let items: Vec<usize> = (0..count).collect();
    let mut found = Found::default();
    let _ = found.add(0, &items);
    found
}

mod given_matches {
    use super::*;

    #[test]
    fn when_none_answer_then_the_name_is_missing() {
        assert_eq!(select(Found::default(), false), Selection::Missing);
    }

    #[test]
    fn when_one_answers_then_it_is_printed() {
        assert_eq!(select(found(1), false), Selection::Print(matches(1)));
    }

    #[test]
    fn when_several_answer_without_all_then_they_are_listed_instead() {
        assert_eq!(
            select(found(2), false),
            Selection::Several {
                matches: matches(2),
                count: 2
            }
        );
    }

    #[test]
    fn when_several_answer_with_all_then_each_is_printed() {
        assert_eq!(select(found(2), true), Selection::Print(matches(2)));
    }

    #[test]
    fn when_more_answer_than_all_prints_then_they_are_counted() {
        let count = PRINTED_MATCHES_MAX + 1;

        assert_eq!(select(found(count), true), Selection::TooMany(count));
    }

    #[test]
    fn when_exactly_as_many_answer_as_all_prints_then_each_is_printed() {
        assert_eq!(
            select(found(PRINTED_MATCHES_MAX), true),
            Selection::Print(matches(PRINTED_MATCHES_MAX))
        );
    }

    #[test]
    fn when_more_answer_than_are_kept_without_all_then_the_kept_are_listed_and_all_counted() {
        let count = KEPT_MATCHES_MAX + 5;

        assert_eq!(
            select(found(count), false),
            Selection::Several {
                matches: matches(KEPT_MATCHES_MAX),
                count
            }
        );
    }
}

mod given_files_with_answering_items {
    use super::*;

    #[test]
    fn when_added_then_the_first_items_are_kept_across_files_and_every_one_is_counted() {
        let first: Vec<usize> = (0..KEPT_MATCHES_MAX - 5).collect();
        let second: Vec<usize> = (0..10).collect();
        let mut found = Found::default();

        let _ = found.add(0, &first);
        let _ = found.add(1, &second);

        let mut expected = matches(KEPT_MATCHES_MAX - 5);
        for item in 0..5 {
            expected.push(Match { file: 1, item });
        }
        assert_eq!(
            select(found, false),
            Selection::Several {
                matches: expected,
                count: KEPT_MATCHES_MAX + 5
            }
        );
    }

    #[test]
    fn when_added_then_a_file_is_kept_only_where_one_of_its_items_is() {
        let filling: Vec<usize> = (0..KEPT_MATCHES_MAX - 1).collect();
        let mut found = Found::default();
        let _ = found.add(0, &filling);

        let kept = [found.add(1, &[7, 8]), found.add(2, &[3])];

        assert_eq!(kept, [true, false]);
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
