//! Subagents' Reads of a whole code file that returned its lines.
//!
//! The first table counts them by the subagent's type: how many ran past the guard's
//! bound, how long their files ran, and how many later calls carried each result. The
//! second counts them by line bound: the share of the Reads past it and of the characters
//! they carried into later calls.

use std::collections::BTreeMap;

use claude_code_transcript::store::Thread;
use excerpt::guard::{ReadSpan, SUBAGENT_READ_LINES_MAX};

use crate::facts::{Facts, ReadResult};
use crate::tally::Group;
use crate::{Full, one_more};

/// The most Reads the tally keeps.
pub const READS_MAX: usize = 1_000_000;

/// The line counts the second table sets apart: the guard's bound, and one either side of
/// it.
const LINE_BOUNDS: [usize; 3] = [200, SUBAGENT_READ_LINES_MAX, 500];
const _: () = assert!(
    LINE_BOUNDS[0] < LINE_BOUNDS[1],
    "the bounds rise to the guard's"
);
const _: () = assert!(
    LINE_BOUNDS[1] < LINE_BOUNDS[2],
    "the bounds rise past the guard's"
);

/// The percentiles of a file's lines the first table gives.
const LINE_PERCENTILES: [usize; 3] = [50, 75, 90];

/// The percentile of later calls the first table gives, their median.
const MEDIAN: usize = 50;

/// The highest percentile, at or under which every value stands.
const PERCENTILE_MAX: usize = 100;

/// What a percentage is of.
const PERCENT_WHOLE: u128 = 100;

/// What the first table names the row counting every Read.
const ALL_NAME: &str = "all";

/// What a table writes where a figure has no values to be taken from.
const NO_VALUE: &str = "-";

/// One Read kept.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SubagentRead {
    agent_type: String,
    lines: usize,
    characters: usize,
    calls_after: usize,
}

/// The Reads kept so far.
#[derive(Debug, Default)]
pub struct SubagentReads {
    reads: Vec<SubagentRead>,
}

impl SubagentRead {
    /// The characters the Read put in the context of its own call and of each call after.
    fn carried(&self) -> u128 {
        let calls = u128::from(one_more_u64(self.calls_after));
        let Some(carried) = u128::from(u64_of(self.characters)).checked_mul(calls) else {
            unreachable!("two counts each within a u64 multiply within a u128")
        };
        carried
    }
}

impl SubagentReads {
    /// A tally having kept nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Keeps each Read a transcript of `thread` made of a whole code file that returned its
    /// numbered lines; a main thread's Reads are not kept.
    ///
    /// # Errors
    ///
    /// [`Full::Reads`] where a Read would pass [`READS_MAX`].
    pub fn take(&mut self, thread: &Thread, facts: &Facts) -> Result<(), Full> {
        let Group::Subagent(agent_type) = Group::of(thread) else {
            return Ok(());
        };
        for read in &facts.reads {
            if read.span == ReadSpan::Whole {
                if let ReadResult::Numbered { lines, characters } = read.result {
                    if self.reads.len() < READS_MAX {
                        self.reads.push(SubagentRead {
                            agent_type: agent_type.clone(),
                            lines,
                            characters,
                            calls_after: read.calls_after,
                        });
                    } else {
                        return Err(Full::Reads);
                    }
                }
            }
        }
        Ok(())
    }

    /// The two tables: by subagent type, the types with the most Reads first and every
    /// Read last; then by line bound, rising.
    #[must_use]
    pub fn tables(&self) -> String {
        let mut by_type: BTreeMap<&str, Vec<&SubagentRead>> = BTreeMap::new();
        for read in &self.reads {
            by_type
                .entry(read.agent_type.as_str())
                .or_default()
                .push(read);
        }
        let mut types: Vec<(&str, Vec<&SubagentRead>)> = by_type.into_iter().collect();
        types.sort_by(|left, right| {
            let by_reads = right.1.len().cmp(&left.1.len());
            by_reads.then(left.0.cmp(right.0))
        });
        let all: Vec<&SubagentRead> = self.reads.iter().collect();
        let mut tables = type_header();
        for (agent_type, reads) in &types {
            tables.push_str(&type_row(agent_type, reads));
        }
        tables.push_str(&type_row(ALL_NAME, &all));
        tables.push('\n');
        tables.push_str(&bound_table(&all));
        tables
    }
}

/// The first table's header row.
fn type_header() -> String {
    let percentiles: Vec<String> = LINE_PERCENTILES
        .iter()
        .map(|percentile| format!("lines_p{percentile}"))
        .collect();
    format!(
        "agent_type\treads\treads_past_{SUBAGENT_READ_LINES_MAX}_lines\t{}\tcharacters_per_line\tcalls_after_p{MEDIAN}\n",
        percentiles.join("\t")
    )
}

/// The first table's row for `reads`, named `name`.
fn type_row(name: &str, reads: &[&SubagentRead]) -> String {
    let past = reads
        .iter()
        .filter(|read| read.lines > SUBAGENT_READ_LINES_MAX)
        .count();
    let mut lines: Vec<usize> = reads.iter().map(|read| read.lines).collect();
    lines.sort_unstable();
    let mut calls_after: Vec<usize> = reads.iter().map(|read| read.calls_after).collect();
    calls_after.sort_unstable();
    let mut fields = vec![name.to_owned(), reads.len().to_string(), past.to_string()];
    for percentile in LINE_PERCENTILES {
        fields.push(figure(nearest_rank(&lines, percentile)));
    }
    let characters: u128 = reads
        .iter()
        .map(|read| u128::from(u64_of(read.characters)))
        .sum();
    let line_total: u128 = lines.iter().map(|count| u128::from(u64_of(*count))).sum();
    fields.push(figure(rounded_ratio(characters, line_total)));
    fields.push(figure(nearest_rank(&calls_after, MEDIAN)));
    format!("{}\n", fields.join("\t"))
}

/// The second table: for each line bound, the Reads past it, and their share of the Reads
/// and of the characters carried into later calls.
fn bound_table(reads: &[&SubagentRead]) -> String {
    let mut table = String::from("past_lines\treads\treads_percent\tcarried_characters_percent\n");
    let read_total = u128::from(u64_of(reads.len()));
    let carried_total: u128 = reads.iter().map(|read| read.carried()).sum();
    for bound in LINE_BOUNDS {
        let past: Vec<&&SubagentRead> = reads.iter().filter(|read| read.lines > bound).collect();
        let carried_past: u128 = past.iter().map(|read| read.carried()).sum();
        let reads_percent = percent(u128::from(u64_of(past.len())), read_total);
        let carried_percent = percent(carried_past, carried_total);
        let row = format!(
            "{bound}\t{}\t{}\t{}\n",
            past.len(),
            figure(reads_percent),
            figure(carried_percent)
        );
        table.push_str(&row);
    }
    table
}

/// The value at `percentile` of `sorted` by nearest rank: the least value that at least
/// `percentile` percent of the values are at or under. None where there are no values.
///
/// # Panics
///
/// When `sorted` is out of order or `percentile` outside 1 to 100, which no caller here
/// passes.
fn nearest_rank(sorted: &[usize], percentile: usize) -> Option<usize> {
    assert!(sorted.is_sorted(), "the values are sorted");
    assert!(percentile >= 1, "a percentile is at least 1");
    assert!(
        percentile <= PERCENTILE_MAX,
        "a percentile is at most the highest"
    );
    if sorted.is_empty() {
        None
    } else {
        let Some(scaled) = percentile.checked_mul(sorted.len()) else {
            unreachable!("a percentile of a bounded count fits a usize")
        };
        let rank = scaled.div_ceil(PERCENTILE_MAX);
        assert!(rank >= 1, "a rank of a value present is at least 1");
        let Some(index) = rank.checked_sub(1) else {
            unreachable!("the rank is at least 1")
        };
        sorted.get(index).copied()
    }
}

/// `part` as a percentage of `whole`, rounded half up; none where `whole` is zero.
fn percent(part: u128, whole: u128) -> Option<u128> {
    assert!(part <= whole, "a part is within its whole");
    let Some(scaled) = part.checked_mul(PERCENT_WHOLE) else {
        unreachable!("a count of characters times 100 fits a u128")
    };
    rounded_ratio(scaled, whole)
}

/// `numerator` over `denominator`, rounded half up; none where `denominator` is zero.
fn rounded_ratio(numerator: u128, denominator: u128) -> Option<u128> {
    if denominator > 0 {
        // The floor of (2n + d) / 2d is n / d rounded half up.
        let doubled = numerator
            .checked_mul(2)
            .and_then(|twice| twice.checked_add(denominator));
        let Some(dividend) = doubled else {
            unreachable!("twice a count of characters times 100 fits a u128")
        };
        let Some(divisor) = denominator.checked_mul(2) else {
            unreachable!("twice a count of characters fits a u128")
        };
        dividend.checked_div(divisor)
    } else {
        None
    }
}

/// A figure as a table writes it.
fn figure<Value: ToString>(value: Option<Value>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => NO_VALUE.to_owned(),
    }
}

/// `count` widened to a u64.
fn u64_of(count: usize) -> u64 {
    let Ok(wide) = u64::try_from(count) else {
        unreachable!("a usize fits a u64 on every target the crate builds for")
    };
    wide
}

/// `count` and one more, widened to a u64.
fn one_more_u64(count: usize) -> u64 {
    u64_of(one_more(count))
}

#[cfg(test)]
mod tests;
