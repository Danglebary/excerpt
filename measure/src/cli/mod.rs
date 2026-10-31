//! The argument grammar: a measure's name, then the window it counts. Pure, so every
//! refusal is a value a test reads.

use jiff::Timestamp;

use crate::window::Window;

/// The flag naming the instant a window opens at.
pub const SINCE_FLAG: &str = "--since";
/// The flag naming the instant a window closes before.
pub const UNTIL_FLAG: &str = "--until";
/// The flags asking for the help text.
const HELP_FLAGS: [&str; 2] = ["--help", "-h"];

/// The figures a request can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    Refusals,
    SubagentReads,
    Replay,
}

/// What the arguments ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Measure(Request),
}

/// A measure over a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub measure: Measure,
    pub window: Window,
}

/// Every way the arguments fail the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageFault {
    NoMeasure,
    UnknownMeasure(String),
    UnknownArgument(String),
    NoValue(&'static str),
    Repeated(&'static str),
    NoSince,
    UnreadableInstant { flag: &'static str, value: String },
    EmptyWindow,
}

impl Measure {
    /// Every measure, in the order the help text lists them.
    pub const ALL: [Self; 3] = [Self::Refusals, Self::SubagentReads, Self::Replay];

    /// The name the command line gives the measure.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Refusals => "refusals",
            Self::SubagentReads => "subagent-reads",
            Self::Replay => "replay",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|measure| measure.name() == name)
    }
}

/// Reads the command line, the program's name left off.
///
/// # Errors
///
/// The [`UsageFault`] naming where the arguments leave the grammar.
pub fn parse_arguments(arguments: &[String]) -> Result<Command, UsageFault> {
    let Some((first, rest)) = arguments.split_first() else {
        return Err(UsageFault::NoMeasure);
    };
    if HELP_FLAGS.contains(&first.as_str()) {
        return Ok(Command::Help);
    }
    let Some(measure) = Measure::named(first) else {
        return Err(UsageFault::UnknownMeasure(first.clone()));
    };
    let mut since = None;
    let mut until = None;
    let mut pending = rest.iter();
    while let Some(argument) = pending.next() {
        let flag = match argument.as_str() {
            SINCE_FLAG => SINCE_FLAG,
            UNTIL_FLAG => UNTIL_FLAG,
            asked if HELP_FLAGS.contains(&asked) => return Ok(Command::Help),
            _ => return Err(UsageFault::UnknownArgument(argument.clone())),
        };
        let Some(value) = pending.next() else {
            return Err(UsageFault::NoValue(flag));
        };
        let Ok(instant) = value.parse::<Timestamp>() else {
            let value = value.clone();
            return Err(UsageFault::UnreadableInstant { flag, value });
        };
        let slot = if flag == SINCE_FLAG {
            &mut since
        } else {
            &mut until
        };
        if slot.is_some() {
            return Err(UsageFault::Repeated(flag));
        }
        *slot = Some(instant);
    }
    let Some(since) = since else {
        return Err(UsageFault::NoSince);
    };
    if let Some(until) = until {
        if until <= since {
            return Err(UsageFault::EmptyWindow);
        }
    }
    let window = Window { since, until };
    Ok(Command::Measure(Request { measure, window }))
}

#[cfg(test)]
mod tests;
