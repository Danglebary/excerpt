//! Every message excerpt prints, rendered from the library's values. Pure, so each message
//! is built from values a test constructs.

use crate::cli::UsageFault;
use crate::item::{Item, Outline};
use crate::language::{EXTENSIONS, Language};
use crate::query::QueryFault;
use crate::select::{ITEM_LINES_MAX, PRINTED_MATCHES_MAX};

/// The most items one outline lists before counting the rest.
pub const OUTLINE_ITEMS_MAX: usize = 500;
/// The most matches a refusal lists before counting the rest.
pub const LISTED_MATCHES_MAX: usize = 20;
/// The indent of an outline's item at the top level, under its file's path.
const OUTLINE_INDENT: &str = "  ";
/// The width a printed line's number is right-aligned to, the Read tool's.
const LINE_NUMBER_WIDTH: usize = 6;

/// The whole grammar, which is what an agent reads before its first call.
pub const HELP: &str = "\
excerpt: print a source file's outline, or one of its items whole, by name.

Usage:
  excerpt outline [PATH...]
      List every item in each file, or in each source file under each directory, with
      the lines it spans: `src/plan.rs` then `  104-180 fn plan`. Nested items are
      indented under their container. PATH defaults to the working directory.
  excerpt show [--all] NAME [PATH...]
      Print the item NAME names, whole, its doc comments and attributes included, each
      line numbered. Refused when no item answers NAME, or when several do and --all is
      absent; the refusal lists them. --all prints every match, up to 20.

NAME is an item's name, optionally qualified by the items it sits in and led by its
kind, exactly as an outline prints it after the line numbers:
  plan                       any item named plan
  Cursor::new  Service.run   the method new of Cursor, the method run of Service
  impl Cursor                the impl block, not the struct of the same name
  impl Display for Cursor    a trait impl

Reads .rs, .ts, .mts, .cts, and .tsx. A directory is walked as git sees it: ignored
and hidden files are skipped.

Exit status:
  0  printed
  1  refused: no item answers, several do, one is too long to print, or a named file
     is in a language excerpt does not read
  2  the arguments are malformed
  3  reading a path failed; what was printed may be incomplete
";

/// One item's line in an outline or a refusal: `src/plan.rs:104-180 fn plan`.
#[must_use]
pub fn header(path: &str, language: Language, item: &Item) -> String {
    let lines = item.lines();
    format!(
        "{path}:{}-{} {} {}",
        lines.first(),
        lines.last(),
        item.kind().keyword(),
        item.qualified_name(language)
    )
}

/// An item's header and then each of its lines, numbered.
#[must_use]
pub fn item_text(path: &str, language: Language, item: &Item, text: &str) -> String {
    let lines = item.lines();
    let mut rendered = vec![header(path, language, item)];
    let Some(skipped) = lines.first().checked_sub(1) else {
        unreachable!("lines are numbered from 1");
    };
    let mut number = lines.first();
    for line in text.lines().skip(skipped).take(lines.count()) {
        rendered.push(format!("{number:>LINE_NUMBER_WIDTH$}\t{line}"));
        let Some(next) = number.checked_add(1) else {
            unreachable!("a line number is below usize::MAX");
        };
        number = next;
    }
    joined(&rendered)
}

/// Every file's items, each under its path, up to `items_max` items; past the bound the
/// remaining items are counted and no later file is named.
#[must_use]
pub fn outline_text(files: &[File<'_>], items_max: usize) -> String {
    let mut rendered = Vec::new();
    let mut listed: usize = 0;
    let mut left_out: usize = 0;
    for file in files {
        let Entry::Outlined { language, outline } = &file.entry else {
            if listed < items_max {
                rendered.push(file.path.to_owned());
                rendered.push(format!("{OUTLINE_INDENT}({})", skip_reason(&file.entry)));
            }
            continue;
        };
        if listed >= items_max {
            left_out = add(left_out, outline.items.len());
            continue;
        }
        rendered.push(file.path.to_owned());
        if outline.items.is_empty() {
            rendered.push(format!("{OUTLINE_INDENT}(no items)"));
        }
        for item in &outline.items {
            if listed >= items_max {
                left_out = add(left_out, 1);
                continue;
            }
            let lines = item.lines();
            let indent = OUTLINE_INDENT.repeat(add(item.depth(), 1));
            rendered.push(format!(
                "{indent}{}-{} {} {}",
                lines.first(),
                lines.last(),
                item.kind().keyword(),
                item.qualified_name(*language)
            ));
            listed = add(listed, 1);
        }
        if let Some(line) = outline.first_error_line {
            rendered.push(format!(
                "{OUTLINE_INDENT}(a syntax error at line {line}; items past it may be missing)"
            ));
        }
    }
    if left_out > 0 {
        rendered.push(format!(
            "{left_out} more items left out; outline a narrower path"
        ));
    }
    joined(&rendered)
}

fn skip_reason(entry: &Entry<'_>) -> String {
    match entry {
        Entry::TooLarge { bytes, bytes_max } => {
            format!("skipped: {bytes} bytes, more than the {bytes_max} excerpt reads")
        }
        Entry::NotText => "skipped: not UTF-8 text".to_owned(),
        Entry::Outlined { .. } => unreachable!("an outlined file is listed, not skipped"),
    }
}

fn add(left: usize, right: usize) -> usize {
    let Some(sum) = left.checked_add(right) else {
        unreachable!("a count of items fits in a usize");
    };
    sum
}

/// Lines ended by a newline each, or nothing for no lines.
fn joined(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Up to `LISTED_MATCHES_MAX` headers, indented under a refusal, and a count of the rest.
fn listed(headers: &[String]) -> Vec<String> {
    let mut lines: Vec<String> = headers
        .iter()
        .take(LISTED_MATCHES_MAX)
        .map(|header| format!("  {header}"))
        .collect();
    if let Some(unlisted) = headers.len().checked_sub(LISTED_MATCHES_MAX) {
        if unlisted > 0 {
            lines.push(format!("  and {unlisted} more"));
        }
    }
    lines
}

#[must_use]
pub fn missing(name: &str, paths: &[String]) -> String {
    format!(
        "excerpt: no item answers `{name}` under {}; `excerpt outline PATH` lists the items there\n",
        paths.join(" ")
    )
}

/// The refusal for a name several items answer, listing each up to the bound.
#[must_use]
pub fn several(name: &str, headers: &[String]) -> String {
    let mut rendered = vec![format!(
        "excerpt: {} items answer `{name}`; name one by its kind or a qualified name, or pass --all to print each:",
        headers.len()
    )];
    rendered.extend(listed(headers));
    joined(&rendered)
}

#[must_use]
pub fn too_many(name: &str, count: usize) -> String {
    format!(
        "excerpt: {count} items answer `{name}`, more than the {PRINTED_MATCHES_MAX} --all prints; name them by kind or a qualified name\n"
    )
}

/// The refusal for an item too long to print, offering what to read instead.
#[must_use]
pub fn too_long(header: &str, count: usize, member_headers: &[String]) -> String {
    let opening = format!(
        "excerpt: {header} spans {count} lines, more than the {ITEM_LINES_MAX} a show prints"
    );
    if member_headers.is_empty() {
        return format!("{opening}; read a range of it with the Read tool\n");
    }
    let mut rendered = vec![format!("{opening}; show one of its members:")];
    rendered.extend(listed(member_headers));
    joined(&rendered)
}

#[must_use]
pub fn unsupported(path: &str) -> String {
    format!(
        "excerpt: {path} is not a file excerpt reads; it reads {}\n",
        extension_list()
    )
}

#[must_use]
pub fn no_sources(paths: &[String]) -> String {
    format!(
        "excerpt: no {} file under {}\n",
        extension_list(),
        paths.join(" ")
    )
}

#[must_use]
pub fn too_many_files(files_max: usize) -> String {
    format!("excerpt: the paths hold more than {files_max} source files; name a narrower path\n")
}

#[must_use]
pub fn read_failure(path: &str, error: &str) -> String {
    format!("excerpt: could not read {path}: {error}\n")
}

/// The note `show` prints for a file it skipped, which may have held a match.
#[must_use]
pub fn skipped(path: &str, entry: &Entry<'_>) -> String {
    format!("excerpt: {path}: {}\n", skip_reason(entry))
}

#[must_use]
pub fn usage_fault(fault: &UsageFault) -> String {
    let problem = match fault {
        UsageFault::MissingCommand => "name a command, outline or show".to_owned(),
        UsageFault::UnknownCommand(word) => {
            format!("`{word}` is not a command; the commands are outline and show")
        }
        UsageFault::UnknownFlag(flag) => format!("`{flag}` is not a flag excerpt takes"),
        UsageFault::MissingName => "show needs the name of the item to print".to_owned(),
    };
    format!("excerpt: {problem}\nRun `excerpt --help` for the grammar.\n")
}

#[must_use]
pub fn query_fault(fault: &QueryFault) -> String {
    match fault {
        QueryFault::Empty => "excerpt: the name is blank\n".to_owned(),
        QueryFault::EmptySegment(name) => {
            format!("excerpt: `{name}` has a separator with no name on one side of it\n")
        }
    }
}

/// `.rs, .ts, .mts, .cts, or .tsx`.
fn extension_list() -> String {
    let dotted: Vec<String> = EXTENSIONS
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect();
    let Some((last, rest)) = dotted.split_last() else {
        unreachable!("excerpt reads at least one extension");
    };
    format!("{}, or {last}", rest.join(", "))
}

/// What became of one file an outline read.
pub enum Entry<'outline> {
    Outlined {
        language: Language,
        outline: &'outline Outline,
    },
    TooLarge {
        bytes: u64,
        bytes_max: u64,
    },
    NotText,
}

/// One file of an outline, by the path it was reached at.
pub struct File<'file> {
    pub path: &'file str,
    pub entry: Entry<'file>,
}

#[cfg(test)]
mod tests;
