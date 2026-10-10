//! The name a reader asks `show` for: an item's name, optionally qualified by the names it
//! sits in and led by the keyword of its kind, exactly as an outline prints it.

use crate::item::{Item, Kind};

/// The separators a name's segments are split at: Rust's path separator and the dot of
/// TypeScript's, since no name in either language holds the other's.
const SEPARATORS: [&str; 2] = ["::", "."];

/// A parsed name: the kind it asks for, if any, and its segments, outermost first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    kind: Option<Kind>,
    segments: Vec<String>,
}

/// Every way a name's text fails to name anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryFault {
    Empty,
    EmptySegment(String),
}

/// Parses the text a reader passed as a name.
///
/// # Errors
///
/// Returns the fault when the text holds no name or a separator with nothing on one side.
pub fn parse(text: &str) -> Result<Query, QueryFault> {
    let trimmed = text.trim();
    let Some((first_word, rest)) = trimmed.split_once(char::is_whitespace) else {
        return segments_of(trimmed).map(|segments| Query {
            kind: None,
            segments,
        });
    };
    let kind = Kind::of_keyword(first_word);
    let name = match kind {
        Some(_) => rest,
        None => trimmed,
    };
    let segments = segments_of(name)?;
    Ok(Query { kind, segments })
}

/// A name split at its separators, the spacing inside each segment collapsed so `Display
/// for Cursor` reads the same however it was spaced.
fn segments_of(name: &str) -> Result<Vec<String>, QueryFault> {
    if name.trim().is_empty() {
        return Err(QueryFault::Empty);
    }
    let Some(first_separator) = SEPARATORS.first() else {
        unreachable!("the separator list is not empty");
    };
    let mut unified = name.to_owned();
    for separator in SEPARATORS {
        unified = unified.replace(separator, first_separator);
    }
    let mut segments = Vec::new();
    for segment in unified.split(first_separator) {
        let words: Vec<&str> = segment.split_whitespace().collect();
        if words.is_empty() {
            return Err(QueryFault::EmptySegment(name.trim().to_owned()));
        }
        segments.push(words.join(" "));
    }
    assert!(!segments.is_empty(), "a non-blank name holds a segment");
    Ok(segments)
}

impl Query {
    #[must_use]
    pub const fn kind(&self) -> Option<Kind> {
        self.kind
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Whether a file's text could hold an item this name answers, decided without
    /// parsing it. Every word of an item's qualified name is text of its file, a bare
    /// type name or a declared name or `default` from `export default`, so a file lacking
    /// one of the name's words holds no item it answers.
    #[must_use]
    pub fn may_answer_in(&self, text: &str) -> bool {
        self.segments
            .iter()
            .flat_map(|segment| segment.split_whitespace())
            .all(|word| text.contains(word))
    }

    /// Whether `item` is one this name asks for.
    #[must_use]
    pub fn answers(&self, item: &Item) -> bool {
        if let Some(kind) = self.kind {
            if kind != item.kind() {
                return false;
            }
        }
        item.path().ends_with(&self.segments)
    }
}

#[cfg(test)]
mod tests;
