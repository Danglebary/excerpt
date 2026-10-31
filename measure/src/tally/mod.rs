//! The measure a request names, folding each transcript's facts as it is read, and the
//! groups the measures count threads by.

use claude_code_transcript::store::{AgentType, Found, Thread};

use crate::Full;
use crate::cli::Measure;
use crate::facts::Facts;
use crate::refusals::Refusals;
use crate::replay::Replay;
use crate::subagent_reads::SubagentReads;

/// The most threads and tools a measure counts apart.
pub const GROUPS_MAX: usize = 100;

/// What a table names the main thread.
const MAIN_THREAD_NAME: &str = "main";

/// The thread a call was made in, as a measure counts it: the main thread, or a subagent
/// by its type. The main thread sorts first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Main,
    Subagent(String),
}

/// The measure a request names, holding what it has counted.
#[derive(Debug)]
pub enum Tally {
    Refusals(Refusals),
    SubagentReads(SubagentReads),
    Replay(Replay),
}

impl Group {
    /// The group `thread` is counted in. A subagent whose meta file names no type is
    /// counted under why it names none, in parentheses.
    #[must_use]
    pub fn of(thread: &Thread) -> Self {
        match thread {
            Thread::Main => Self::Main,
            Thread::Subagent { agent_type, .. } => {
                let name = match agent_type {
                    AgentType::Named(name) => name.clone(),
                    AgentType::Unnamed => "(unnamed)".to_owned(),
                    AgentType::NoMeta => "(no meta file)".to_owned(),
                    AgentType::Malformed => "(malformed meta file)".to_owned(),
                    AgentType::Unread(_) => "(unread meta file)".to_owned(),
                };
                Self::Subagent(name)
            }
        }
    }

    /// What a table names the group.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Main => MAIN_THREAD_NAME,
            Self::Subagent(name) => name,
        }
    }
}

impl Tally {
    /// The measure `measure` names, having counted nothing.
    #[must_use]
    pub fn new(measure: Measure) -> Self {
        match measure {
            Measure::Refusals => Self::Refusals(Refusals::new()),
            Measure::SubagentReads => Self::SubagentReads(SubagentReads::new()),
            Measure::Replay => Self::Replay(Replay::new()),
        }
    }

    /// Counts one transcript's facts.
    ///
    /// # Errors
    ///
    /// The bound the measure reached, past which it counts nothing more.
    pub fn take(&mut self, found: &Found, facts: &Facts) -> Result<(), Full> {
        match self {
            Self::Refusals(refusals) => refusals.take(&found.thread, facts),
            Self::SubagentReads(reads) => reads.take(&found.thread, facts),
            Self::Replay(replay) => replay.take(&found.session, facts),
        }
    }

    /// The measure's tables, tab-separated, each under a header row and the next after a
    /// blank line.
    #[must_use]
    pub fn tables(&self) -> String {
        match self {
            Self::Refusals(refusals) => refusals.table(),
            Self::SubagentReads(reads) => reads.tables(),
            Self::Replay(replay) => replay.tables(),
        }
    }
}

#[cfg(test)]
mod tests;
