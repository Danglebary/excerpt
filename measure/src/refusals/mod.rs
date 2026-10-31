//! The guard's refusals, by the thread that drew them and the tool it called.
//!
//! Every call to that tool the guard judges is counted beside them, so two windows
//! compare by share as well as by count.

use std::collections::BTreeMap;

use claude_code_transcript::store::Thread;

use crate::facts::Facts;
use crate::tally::{GROUPS_MAX, Group};
use crate::{Full, one_more};

/// The table's header row.
const HEADER: &str = "thread\ttool\tcalls\trefusals\n";

/// The tools the guard judges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tool {
    Bash,
    Read,
}

/// The calls of one thread and tool, and the refusals among them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Counts {
    calls: usize,
    refusals: usize,
}

/// The calls and refusals counted so far, by thread and tool.
#[derive(Debug, Default)]
pub struct Refusals {
    counts: BTreeMap<(Group, Tool), Counts>,
}

impl Tool {
    const fn name(self) -> &'static str {
        match self {
            Self::Bash => "Bash",
            Self::Read => "Read",
        }
    }
}

impl Refusals {
    /// A tally having counted nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts the Bash calls naming a code file and the Reads of one that a transcript of
    /// `thread` made.
    ///
    /// # Errors
    ///
    /// [`Full::Groups`] where a new thread or tool would pass [`GROUPS_MAX`].
    pub fn take(&mut self, thread: &Thread, facts: &Facts) -> Result<(), Full> {
        let group = Group::of(thread);
        for command in &facts.commands {
            self.count(&group, Tool::Bash, command.refused)?;
        }
        for read in &facts.reads {
            self.count(&group, Tool::Read, read.refused)?;
        }
        Ok(())
    }

    /// The table: a row per thread and tool, the main thread first and each subagent type
    /// in name order.
    ///
    /// # Panics
    ///
    /// When a row counts more refusals than calls, which no facts can cause.
    #[must_use]
    pub fn table(&self) -> String {
        let mut table = String::from(HEADER);
        for ((group, tool), counts) in &self.counts {
            assert!(counts.refusals <= counts.calls, "each refusal is of a call");
            let row = format!(
                "{}\t{}\t{}\t{}\n",
                group.name(),
                tool.name(),
                counts.calls,
                counts.refusals
            );
            table.push_str(&row);
        }
        table
    }

    fn count(&mut self, group: &Group, tool: Tool, refused: bool) -> Result<(), Full> {
        let key = (group.clone(), tool);
        let known = self.counts.contains_key(&key);
        if known || self.counts.len() < GROUPS_MAX {
            let counts = self.counts.entry(key).or_default();
            counts.calls = one_more(counts.calls);
            if refused {
                counts.refusals = one_more(counts.refusals);
            }
            assert!(
                self.counts.len() <= GROUPS_MAX,
                "the groups stop at their bound"
            );
            Ok(())
        } else {
            Err(Full::Groups)
        }
    }
}

#[cfg(test)]
mod tests;
