//! The refusals table. Each case hands the tally the facts of a few transcripts, written as
//! values, and asserts on the whole table it renders.

use claude_code_transcript::store::{AgentType, Thread};
use excerpt::guard::ReadSpan;

use super::Refusals;
use crate::Full;
use crate::facts::{CodeCommand, CodeRead, Facts, ReadResult};
use crate::tally::GROUPS_MAX;

fn subagent(agent_type: &str) -> Thread {
    Thread::Subagent {
        agent_id: "a1".to_owned(),
        agent_type: AgentType::Named(agent_type.to_owned()),
    }
}

fn commands(refused: &[bool]) -> Facts {
    Facts {
        commands: refused
            .iter()
            .map(|refused| CodeCommand {
                command: "cat src/plan.rs".to_owned(),
                refused: *refused,
            })
            .collect(),
        reads: Vec::new(),
        unheld: 0,
    }
}

fn read(refused: bool) -> CodeRead {
    CodeRead {
        span: ReadSpan::Whole,
        refused,
        result: ReadResult::Unnumbered,
        calls_after: 0,
    }
}

mod given_transcripts {
    use super::*;

    #[test]
    fn when_tabled_then_each_thread_and_tool_counts_its_calls_and_refusals() {
        let mut refusals = Refusals::new();
        let explore = Facts {
            reads: vec![read(true), read(false)],
            ..commands(&[true])
        };
        refusals
            .take(
                &subagent("general-purpose"),
                &commands(&[true, true, false]),
            )
            .unwrap();
        refusals.take(&subagent("Explore"), &explore).unwrap();
        refusals
            .take(&Thread::Main, &commands(&[false, true]))
            .unwrap();
        refusals
            .take(&subagent("general-purpose"), &commands(&[false]))
            .unwrap();

        assert_eq!(
            refusals.table(),
            "thread\ttool\tcalls\trefusals\n\
             main\tBash\t2\t1\n\
             Explore\tBash\t1\t1\n\
             Explore\tRead\t2\t1\n\
             general-purpose\tBash\t4\t2\n"
        );
    }

    #[test]
    fn when_none_made_a_counted_call_then_the_table_is_its_header() {
        let mut refusals = Refusals::new();
        refusals.take(&Thread::Main, &Facts::default()).unwrap();

        assert_eq!(refusals.table(), "thread\ttool\tcalls\trefusals\n");
    }
}

mod given_more_groups_than_the_bound {
    use super::*;

    #[test]
    fn when_the_group_past_it_is_counted_then_the_tally_is_full() {
        let mut refusals = Refusals::new();
        for index in 0..GROUPS_MAX {
            let thread = subagent(&format!("type-{index}"));
            refusals.take(&thread, &commands(&[false])).unwrap();
        }

        let past = refusals.take(&subagent("one-more"), &commands(&[false]));

        assert_eq!(past, Err(Full::Groups));
    }
}
