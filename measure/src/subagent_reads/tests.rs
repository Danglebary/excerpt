//! The subagent Reads tables. Each case hands the tally the facts of a few transcripts,
//! written as values with figures chosen so every percentile and share is worked by hand,
//! and asserts on the whole of both tables it renders.

use claude_code_transcript::store::{AgentType, Thread};
use excerpt::guard::ReadSpan;

use super::{SubagentReads, nearest_rank, percent};
use crate::facts::{CodeRead, Facts, ReadResult};

fn subagent(agent_type: &str) -> Thread {
    Thread::Subagent {
        agent_id: "a1".to_owned(),
        agent_type: AgentType::Named(agent_type.to_owned()),
    }
}

fn read(lines: usize, characters: usize, calls_after: usize) -> CodeRead {
    CodeRead {
        span: ReadSpan::Whole,
        refused: false,
        result: ReadResult::Numbered { lines, characters },
        calls_after,
    }
}

fn reads(reads: Vec<CodeRead>) -> Facts {
    Facts {
        commands: Vec::new(),
        reads,
        unheld: 0,
    }
}

mod given_subagent_reads {
    use super::*;

    #[test]
    fn when_tabled_then_each_type_and_bound_carries_its_figures() {
        let mut tally = SubagentReads::new();
        let general = reads(vec![
            read(100, 4_000, 9),
            read(250, 10_000, 1),
            read(600, 24_000, 0),
        ]);
        let explore = reads(vec![read(400, 16_000, 4)]);
        tally.take(&subagent("general-purpose"), &general).unwrap();
        tally.take(&subagent("Explore"), &explore).unwrap();

        assert_eq!(
            tally.tables(),
            "agent_type\treads\treads_past_300_lines\tlines_p50\tlines_p75\tlines_p90\tcharacters_per_line\tcalls_after_p50\n\
             general-purpose\t3\t1\t250\t600\t600\t40\t1\n\
             Explore\t1\t1\t400\t400\t400\t40\t4\n\
             all\t4\t2\t250\t400\t600\t40\t1\n\
             \n\
             past_lines\treads\treads_percent\tcarried_characters_percent\n\
             200\t3\t75\t76\n\
             300\t2\t50\t63\n\
             500\t1\t25\t15\n"
        );
    }

    #[test]
    fn when_one_is_ranged_unnumbered_or_a_main_threads_then_it_is_not_kept() {
        let mut tally = SubagentReads::new();
        let ranged = CodeRead {
            span: ReadSpan::Ranged,
            ..read(400, 16_000, 0)
        };
        let unnumbered = CodeRead {
            result: ReadResult::Unnumbered,
            ..read(0, 0, 0)
        };
        tally
            .take(&subagent("Explore"), &reads(vec![ranged, unnumbered]))
            .unwrap();
        tally
            .take(&Thread::Main, &reads(vec![read(400, 16_000, 0)]))
            .unwrap();

        assert_eq!(
            tally.tables(),
            "agent_type\treads\treads_past_300_lines\tlines_p50\tlines_p75\tlines_p90\tcharacters_per_line\tcalls_after_p50\n\
             all\t0\t0\t-\t-\t-\t-\t-\n\
             \n\
             past_lines\treads\treads_percent\tcarried_characters_percent\n\
             200\t0\t-\t-\n\
             300\t0\t-\t-\n\
             500\t0\t-\t-\n"
        );
    }
}

mod given_sorted_values {
    use super::*;

    #[test]
    fn when_a_percentile_is_taken_then_it_is_the_least_value_that_many_stand_at_or_under() {
        let values = [10, 20, 30, 40];
        let rows = [
            (1, Some(10)),
            (25, Some(10)),
            (26, Some(20)),
            (50, Some(20)),
            (75, Some(30)),
            (100, Some(40)),
        ];
        for (percentile, value) in rows {
            assert_eq!(nearest_rank(&values, percentile), value, "p{percentile}");
        }
        assert_eq!(nearest_rank(&[], 50), None);
    }

    #[test]
    #[should_panic]
    fn when_they_are_out_of_order_then_taking_a_percentile_crashes() {
        let _ = nearest_rank(&[20, 10], 50);
    }
}

mod given_a_part_of_a_whole {
    use super::*;

    #[test]
    fn when_its_percentage_is_taken_then_it_is_rounded_half_up() {
        let rows = [
            (1, 8, Some(13)),
            (1, 3, Some(33)),
            (2, 3, Some(67)),
            (1, 200, Some(1)),
            (0, 5, Some(0)),
            (0, 0, None),
        ];
        for (part, whole, rounded) in rows {
            assert_eq!(percent(part, whole), rounded, "{part} of {whole}");
        }
    }
}
