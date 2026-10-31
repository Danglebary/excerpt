//! The messages, read whole: each case renders one from values it constructs and asserts
//! on its text, since a message is what this module produces.

use super::{PassedOver, full, passed_over, usage_fault};
use crate::Full;
use crate::cli::UsageFault;

mod given_a_usage_fault {
    use super::*;

    #[test]
    fn when_rendered_then_it_names_the_fault_and_points_at_the_help() {
        let message = usage_fault(&UsageFault::UnknownMeasure("guards".to_owned()));

        assert_eq!(
            message,
            "excerpt-measure: guards is no measure; the measures are refusals, subagent-reads, replay\n\
             Run `excerpt-measure --help` for the grammar.\n"
        );
    }
}

mod given_a_bound_reached {
    use super::*;

    #[test]
    fn when_rendered_then_it_names_the_bound_and_how_to_stay_under_it() {
        assert_eq!(
            full(Full::Groups),
            "excerpt-measure: stopped: more than 100 threads and tools would be counted apart; narrow the window\n"
        );
    }
}

mod given_what_a_measure_passed_over {
    use super::*;

    #[test]
    fn when_it_passed_over_nothing_then_the_message_is_empty() {
        assert_eq!(passed_over(&PassedOver::default()), "");
    }

    #[test]
    fn when_it_passed_over_some_then_each_count_is_named_in_its_number() {
        let counts = PassedOver {
            transcripts_unread: 1,
            lines_malformed: 2,
            calls_unheld: 3,
            ..PassedOver::default()
        };

        assert_eq!(
            passed_over(&counts),
            "excerpt-measure: passed over 1 transcript it could not read, 2 malformed lines, 3 calls the pairing could not hold\n"
        );
    }

    #[test]
    fn when_two_counts_are_added_then_each_field_is_summed() {
        let left = PassedOver {
            lines_too_long: 1,
            transcripts_cut: 2,
            ..PassedOver::default()
        };
        let right = PassedOver {
            lines_too_long: 4,
            lines_not_text: 5,
            ..PassedOver::default()
        };

        assert_eq!(
            left.plus(&right),
            PassedOver {
                lines_too_long: 5,
                transcripts_cut: 2,
                lines_not_text: 5,
                ..PassedOver::default()
            }
        );
    }
}
