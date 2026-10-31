//! Every message the binary prints but a measure's tables, rendered from the library's
//! values. Pure, so each message is built from values a test constructs.

use std::path::Path;

use claude_code_transcript::store::StoreFault;

use crate::cli::{Measure, SINCE_FLAG, UNTIL_FLAG, UsageFault};
use crate::facts::FACTS_PER_TRANSCRIPT_MAX;
use crate::replay::SESSIONS_MAX;
use crate::subagent_reads::READS_MAX;
use crate::tally::GROUPS_MAX;
use crate::{Full, sum};

/// What every message opens with.
const OPENING: &str = "excerpt-measure: ";

/// The help text, `--help`'s output.
pub const HELP: &str = "\
excerpt-measure: figures about excerpt's guard, from Claude Code's transcripts

Usage: excerpt-measure MEASURE --since INSTANT [--until INSTANT]

Reads every transcript under ~/.claude/projects written since --since and counts the
tool calls made from --since, and before --until where it is named. An instant is
RFC 3339 with an offset, such as 2026-10-18T01:16:32Z. Each measure prints
tab-separated tables, each under a header row and the next after a blank line. A
thread is the main thread or a subagent's type.

Measures:
  refusals        Bash calls naming a code file and Reads of one, by thread and
                  tool, beside the guard's refusals among them.
  subagent-reads  Subagents' Reads of a whole code file that returned its lines: by
                  type, how many ran past the guard's bound, their lines'
                  percentiles, and the later calls carrying each; and by line bound,
                  the share of Reads past it and of the characters they carried.
  replay          Bash calls naming a code file, by whether the guard refused them
                  then and what this build's guard says of them now; and the
                  sessions holding a call that passed then and is denied now, by
                  whether they drew a refusal.

Exit codes: 0 printed, 1 the store could not be read or a bound was reached,
2 the arguments are malformed.
";

/// What a measure passed over while reading the store, each counted where it was met.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassedOver {
    pub transcripts_unread: usize,
    pub transcripts_cut: usize,
    pub lines_too_long: usize,
    pub lines_not_text: usize,
    pub lines_malformed: usize,
    pub calls_unheld: usize,
}

impl PassedOver {
    /// These counts and `other`'s, added.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        Self {
            transcripts_unread: sum(self.transcripts_unread, other.transcripts_unread),
            transcripts_cut: sum(self.transcripts_cut, other.transcripts_cut),
            lines_too_long: sum(self.lines_too_long, other.lines_too_long),
            lines_not_text: sum(self.lines_not_text, other.lines_not_text),
            lines_malformed: sum(self.lines_malformed, other.lines_malformed),
            calls_unheld: sum(self.calls_unheld, other.calls_unheld),
        }
    }
}

/// Why the arguments were refused, and where the grammar is.
#[must_use]
pub fn usage_fault(fault: &UsageFault) -> String {
    let reason = match fault {
        UsageFault::NoMeasure => format!("name a measure: {}", measure_names()),
        UsageFault::UnknownMeasure(name) => {
            format!("{name} is no measure; the measures are {}", measure_names())
        }
        UsageFault::UnknownArgument(argument) => {
            format!("{argument} is no flag; the flags are {SINCE_FLAG} and {UNTIL_FLAG}")
        }
        UsageFault::NoValue(flag) => format!("{flag} takes an instant"),
        UsageFault::Repeated(flag) => format!("{flag} is named twice"),
        UsageFault::NoSince => format!("name the instant the window opens at with {SINCE_FLAG}"),
        UsageFault::UnreadableInstant { flag, value } => {
            format!(
                "{flag} takes an RFC 3339 instant with an offset, such as 2026-10-18T01:16:32Z, not {value}"
            )
        }
        UsageFault::EmptyWindow => format!("{UNTIL_FLAG} falls at or before {SINCE_FLAG}"),
    };
    format!("{OPENING}{reason}\nRun `excerpt-measure --help` for the grammar.\n")
}

/// Why the home folder is unknown.
#[must_use]
pub fn no_home() -> String {
    format!("{OPENING}HOME names no folder, so the transcript store cannot be found\n")
}

/// Why the store could not be listed.
#[must_use]
pub fn store_fault(fault: &StoreFault) -> String {
    let reason = match fault {
        StoreFault::Unlisted { path, kind } => format!("could not list {}: {kind}", shown(path)),
        StoreFault::TooManyEntries { path } => {
            format!("{} holds too many entries to list", shown(path))
        }
        StoreFault::TooManyTranscripts { path } => {
            format!("{} holds too many transcripts to read", shown(path))
        }
    };
    format!("{OPENING}{reason}\n")
}

/// Which bound the measure reached.
#[must_use]
pub fn full(full: Full) -> String {
    let reason = match full {
        Full::Facts => {
            format!("a transcript holds more than {FACTS_PER_TRANSCRIPT_MAX} counted calls")
        }
        Full::Groups => format!("more than {GROUPS_MAX} threads and tools would be counted apart"),
        Full::Reads => format!("more than {READS_MAX} subagent Reads would be kept"),
        Full::Sessions => format!("more than {SESSIONS_MAX} sessions would be counted apart"),
    };
    format!("{OPENING}stopped: {reason}; narrow the window\n")
}

/// What the measure passed over, or nothing where it passed over nothing.
#[must_use]
pub fn passed_over(passed_over: &PassedOver) -> String {
    let counts = [
        (
            passed_over.transcripts_unread,
            "transcript",
            "transcripts",
            " it could not read",
        ),
        (
            passed_over.transcripts_cut,
            "transcript",
            "transcripts",
            " past the line bound",
        ),
        (
            passed_over.lines_too_long,
            "line",
            "lines",
            " past the length bound",
        ),
        (
            passed_over.lines_not_text,
            "line",
            "lines",
            " of no UTF-8 text",
        ),
        (
            passed_over.lines_malformed,
            "malformed line",
            "malformed lines",
            "",
        ),
        (
            passed_over.calls_unheld,
            "call",
            "calls",
            " the pairing could not hold",
        ),
    ];
    let mut named = Vec::new();
    for (count, singular, plural, qualifier) in counts {
        if count > 0 {
            let noun = if count == 1 { singular } else { plural };
            named.push(format!("{count} {noun}{qualifier}"));
        }
    }
    if named.is_empty() {
        String::new()
    } else {
        format!("{OPENING}passed over {}\n", named.join(", "))
    }
}

/// Every measure's name, as a message lists them.
fn measure_names() -> String {
    let names: Vec<&str> = Measure::ALL.iter().map(|measure| measure.name()).collect();
    names.join(", ")
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests;
