//! Figures about excerpt's guard, computed from the transcripts Claude Code keeps of its
//! sessions: `facts` takes from one transcript the calls a measure counts, each measure
//! (`refusals`, `subagent_reads`, `replay`) folds those facts across transcripts into its
//! tables, and `tally` picks the measure a request names.
//!
//! Everything here is pure, which is what lets a figure be replayed against fixture lines;
//! the binary walks the store, reads each transcript, and prints what the library rendered.
//! The verdicts a figure counts come from excerpt's library at the commit being built.

pub mod cli;
pub mod facts;
pub mod refusals;
pub mod replay;
pub mod report;
pub mod subagent_reads;
pub mod tally;
pub mod window;

/// A bound a measure reached, past which it counts nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Full {
    /// One transcript holds more calls than [`facts::FACTS_PER_TRANSCRIPT_MAX`].
    Facts,
    /// The threads and tools counted apart pass [`tally::GROUPS_MAX`].
    Groups,
    /// The subagent Reads kept pass [`subagent_reads::READS_MAX`].
    Reads,
    /// The sessions counted apart pass [`replay::SESSIONS_MAX`].
    Sessions,
}

/// `count` and one more.
///
/// # Panics
///
/// When `count` is `usize::MAX`, which no count of a transcript's lines or calls reaches.
pub(crate) fn one_more(count: usize) -> usize {
    let Some(next) = count.checked_add(1) else {
        unreachable!("a count of lines or calls stays below usize::MAX")
    };
    assert!(next > count, "one more is more");
    next
}

/// Two counts added.
///
/// # Panics
///
/// When the sum passes `usize::MAX`, which no two counts of a store's lines or calls reach.
pub(crate) fn sum(left: usize, right: usize) -> usize {
    let Some(total) = left.checked_add(right) else {
        unreachable!("counts of a store's lines or calls add within a usize")
    };
    assert!(total >= left, "a sum is at least either count");
    total
}
