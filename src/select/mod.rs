//! Which of the items answering a name `show` keeps and prints: exactly one, or every one
//! under `--all` up to a bound, and never an item longer than a reader can take in at
//! once.

use crate::item::Item;

/// The most items `--all` prints.
pub const PRINTED_MATCHES_MAX: usize = 20;
/// The most matches `Found` keeps, which is as many as any selection prints.
pub const KEPT_MATCHES_MAX: usize = PRINTED_MATCHES_MAX;
/// The most lines one item may span and still be printed whole.
pub const ITEM_LINES_MAX: usize = 1_000;

/// An item answering a name: the index of its file among those read, and its index in
/// that file's outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub file: usize,
    pub item: usize,
}

/// The items answering a name in the files read, in walk order: the first
/// `KEPT_MATCHES_MAX` of them, and the count of every one.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Found {
    kept: Vec<Match>,
    count: usize,
}

impl Found {
    /// Counts `items`, the answering items of one more file, and keeps as many of them
    /// as there is room for, each a match in `file`. Returns whether any was kept, which
    /// is whether a selection may print from the file.
    ///
    /// # Panics
    ///
    /// When more matches are kept than are counted.
    pub fn add(&mut self, file: usize, items: &[usize]) -> bool {
        let Some(count) = self.count.checked_add(items.len()) else {
            unreachable!("a count of matches fits in a usize");
        };
        self.count = count;
        let Some(room) = KEPT_MATCHES_MAX.checked_sub(self.kept.len()) else {
            unreachable!("the kept matches stay within their bound");
        };
        let kept_count = items.len().min(room);
        for item in items.iter().take(kept_count) {
            self.kept.push(Match { file, item: *item });
        }
        assert!(self.kept.len() <= self.count, "every kept match is counted");
        kept_count > 0
    }
}

/// What `show` does with the matches it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Print(Vec<Match>),
    Missing,
    /// More than one item answers and the reader asked for one: the matches kept, and the
    /// count of every one.
    Several {
        matches: Vec<Match>,
        count: usize,
    },
    /// More items answer than `--all` prints; the count of them.
    TooMany(usize),
}

/// What `show` does with the matches `found` holds, `--all` given or not.
///
/// # Panics
///
/// When `--all` prints a match `found` did not keep.
#[must_use]
pub fn select(found: Found, all: bool) -> Selection {
    let Found { kept, count } = found;
    match count {
        0 => Selection::Missing,
        1 => Selection::Print(kept),
        count => {
            if !all {
                return Selection::Several {
                    matches: kept,
                    count,
                };
            }
            if count > PRINTED_MATCHES_MAX {
                return Selection::TooMany(count);
            }
            assert_eq!(kept.len(), count, "every match --all prints is kept");
            Selection::Print(kept)
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
