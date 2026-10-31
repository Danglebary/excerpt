//! The replay tables. Each case hands the tally the facts of a few transcripts, written as
//! values whose commands the guard built from this tree judges, and asserts on the whole
//! of both tables it renders.

use excerpt::guard::ReadSpan;

use super::{Replay, SESSIONS_MAX};
use crate::Full;
use crate::facts::{CodeCommand, CodeRead, Facts, ReadResult};

fn commands(commands: &[(&str, bool)]) -> Facts {
    Facts {
        commands: commands
            .iter()
            .map(|(command, refused)| CodeCommand {
                command: (*command).to_owned(),
                refused: *refused,
            })
            .collect(),
        reads: Vec::new(),
        unheld: 0,
    }
}

fn refused_read() -> CodeRead {
    CodeRead {
        span: ReadSpan::Whole,
        refused: true,
        result: ReadResult::Unnumbered,
        calls_after: 0,
    }
}

mod given_transcripts {
    use super::*;

    #[test]
    fn when_tabled_then_each_call_is_counted_by_its_verdict_then_and_now() {
        let mut replay = Replay::new();
        let first = commands(&[
            ("cat src/plan.rs", true),
            ("awk 'NR>=10 && NR<=40' src/plan.rs", false),
            ("rg -n plan src/plan.rs", false),
        ]);
        let second = commands(&[
            ("sed -n 1,5p src/plan.rs", false),
            ("cat src/plan.rs", true),
        ]);
        replay.take("s1", &first).unwrap();
        replay.take("s2", &second).unwrap();

        let tables = replay.tables();

        assert_eq!(
            tables.split("\n\n").next().unwrap(),
            "then\tnow\tcalls\n\
             passed\tallowed\t1\n\
             passed\tdenied: awk range\t1\n\
             passed\tdenied: sed range\t1\n\
             refused\tdenied: cat\t2"
        );
    }

    #[test]
    fn when_tabled_then_sessions_holding_a_call_newly_denied_are_counted_by_their_refusals() {
        let mut replay = Replay::new();
        let quiet = commands(&[
            ("head -5 src/plan.rs", false),
            ("tail -5 src/plan.rs", false),
        ]);
        let main_thread = commands(&[("cat src/plan.rs", false)]);
        let subagent = Facts {
            reads: vec![refused_read()],
            ..Facts::default()
        };
        let allowed = commands(&[("rg -n plan src/plan.rs", false)]);
        replay.take("quiet", &quiet).unwrap();
        replay.take("refused", &main_thread).unwrap();
        replay.take("refused", &subagent).unwrap();
        replay.take("allowed", &allowed).unwrap();

        let tables = replay.tables();

        assert_eq!(
            tables.split("\n\n").nth(1).unwrap(),
            "session_refusals\tsessions\tcalls\n\
             none\t1\t2\n\
             some\t1\t1\n"
        );
    }
}

mod given_more_sessions_than_the_bound {
    use super::*;

    #[test]
    fn when_the_session_past_it_draws_a_refusal_then_the_tally_is_full() {
        let mut replay = Replay::new();
        let facts = Facts {
            reads: vec![refused_read()],
            ..Facts::default()
        };
        for index in 0..SESSIONS_MAX {
            replay.take(&format!("s{index}"), &facts).unwrap();
        }

        let past = replay.take("one-more", &facts);

        assert_eq!(past, Err(Full::Sessions));
    }

    #[test]
    fn when_the_session_past_it_draws_nothing_then_it_is_not_counted() {
        let mut replay = Replay::new();
        let facts = Facts {
            reads: vec![refused_read()],
            ..Facts::default()
        };
        for index in 0..SESSIONS_MAX {
            replay.take(&format!("s{index}"), &facts).unwrap();
        }

        let past = replay.take("one-more", &Facts::default());

        assert_eq!(past, Ok(()));
    }
}
