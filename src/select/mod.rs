//! Which of the items answering a name `show` prints: exactly one, or every one under
//! `--all` up to a bound, and never an item longer than a reader can take in at once.

use crate::item::Item;

/// The most items `--all` prints.
pub const PRINTED_MATCHES_MAX: usize = 20;
/// The most lines one item may span and still be printed whole.
pub const ITEM_LINES_MAX: usize = 1_000;

/// An item answering a name: the index of its file among those read, and its index in
/// that file's outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub file: usize,
    pub item: usize,
}

/// What `show` does with the matches it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Print(Vec<Match>),
    Missing,
    /// More than one item answers and the reader asked for one.
    Several(Vec<Match>),
    /// More items answer than `--all` prints; the count of them.
    TooMany(usize),
}

#[must_use]
pub fn select(matches: Vec<Match>, all: bool) -> Selection {
    match matches.len() {
        0 => Selection::Missing,
        1 => Selection::Print(matches),
        count => {
            if !all {
                return Selection::Several(matches);
            }
            if count > PRINTED_MATCHES_MAX {
                return Selection::TooMany(count);
            }
            Selection::Print(matches)
        }
    }
}

/// Whether an item is short enough to print whole.
#[must_use]
pub fn printable(item: &Item) -> bool {
    item.lines().count() <= ITEM_LINES_MAX
}

#[cfg(test)]
mod tests;
