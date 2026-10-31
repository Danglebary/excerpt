//! The argument grammar, as rows of a command line against what it parses to: a request
//! for each measure, the help text, and each refusal with the argument it names.

use super::{Command, Measure, Request, UsageFault, parse_arguments};
use crate::window::Window;

const SINCE: &str = "2026-10-18T01:16:32Z";
const UNTIL: &str = "2026-10-30T00:00:00Z";

fn parse(arguments: &[&str]) -> Result<Command, UsageFault> {
    let owned: Vec<String> = arguments
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect();
    parse_arguments(&owned)
}

fn request(measure: Measure, until: Option<&str>) -> Command {
    Command::Measure(Request {
        measure,
        window: Window {
            since: SINCE.parse().unwrap(),
            until: until.map(|until| until.parse().unwrap()),
        },
    })
}

mod given_a_well_formed_command_line {
    use super::*;

    #[test]
    fn when_parsed_then_it_names_the_measure_and_its_window() {
        let rows = [
            (
                vec!["refusals", "--since", SINCE],
                request(Measure::Refusals, None),
            ),
            (
                vec!["subagent-reads", "--until", UNTIL, "--since", SINCE],
                request(Measure::SubagentReads, Some(UNTIL)),
            ),
            (
                vec!["replay", "--since", SINCE, "--until", UNTIL],
                request(Measure::Replay, Some(UNTIL)),
            ),
            (vec!["--help"], Command::Help),
            (vec!["-h"], Command::Help),
            (vec!["replay", "--since", SINCE, "--help"], Command::Help),
        ];
        for (arguments, parsed) in rows {
            assert_eq!(parse(&arguments), Ok(parsed), "{arguments:?}");
        }
    }
}

mod given_a_malformed_command_line {
    use super::*;

    #[test]
    fn when_parsed_then_the_fault_names_where_it_leaves_the_grammar() {
        let rows = [
            (vec![], UsageFault::NoMeasure),
            (
                vec!["guards"],
                UsageFault::UnknownMeasure("guards".to_owned()),
            ),
            (
                vec!["replay", "--from", SINCE],
                UsageFault::UnknownArgument("--from".to_owned()),
            ),
            (vec!["replay", "--since"], UsageFault::NoValue("--since")),
            (
                vec!["replay", "--since", SINCE, "--since", SINCE],
                UsageFault::Repeated("--since"),
            ),
            (vec!["replay", "--until", UNTIL], UsageFault::NoSince),
            (
                vec!["replay", "--since", "2026-10-18"],
                UsageFault::UnreadableInstant {
                    flag: "--since",
                    value: "2026-10-18".to_owned(),
                },
            ),
            (
                vec!["replay", "--since", UNTIL, "--until", SINCE],
                UsageFault::EmptyWindow,
            ),
            (
                vec!["replay", "--since", SINCE, "--until", SINCE],
                UsageFault::EmptyWindow,
            ),
        ];
        for (arguments, fault) in rows {
            assert_eq!(parse(&arguments), Err(fault), "{arguments:?}");
        }
    }
}
