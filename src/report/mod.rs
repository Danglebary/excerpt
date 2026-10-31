//! Every message excerpt prints, rendered from the library's values. Pure, so each message
//! is built from values a test constructs.

use std::ops::ControlFlow;

use crate::cli::UsageFault;
use crate::guard::{Denial, HookInputFault, Reader, SUBAGENT_READ_LINES_MAX};
use crate::item::{Item, Outline};
use crate::language::{EXTENSIONS, Language};
use crate::query::QueryFault;
use crate::select::{ITEM_LINES_MAX, KEPT_MATCHES_MAX, PRINTED_MATCHES_MAX};

/// The most items one outline lists before counting the rest.
pub const OUTLINE_ITEMS_MAX: usize = 500;
/// The most matches a refusal lists before counting the rest.
pub const LISTED_MATCHES_MAX: usize = 20;
const _: () = assert!(
    LISTED_MATCHES_MAX <= KEPT_MATCHES_MAX,
    "a refusal lists no more matches than are kept"
);
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
  excerpt guard
      The Claude Code hook the plugin runs before each Bash and Read call. Reads the
      hook's input on stdin and blocks printing lines of a file excerpt reads through a
      `sed -n` line range, `awk` line numbers, `head`, `tail`, or `cat`, or a subagent's
      whole-file Read of a long one, offering outline and show in its place.
  excerpt brief
      The Claude Code hook the plugin runs as each subagent starts. Prints the JSON that
      adds the rule on reading code by item to the subagent's context.

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
guard speaks Claude Code's hook codes instead: 0 lets the command run, 2 blocks it,
and 1 reports a hook input it cannot read without blocking the command. brief exits 0
once its JSON is printed.
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

/// An outline listed one file at a time in walk order, up to `items_max` items.
pub struct Listing {
    lines: Vec<String>,
    listed: usize,
    items_max: usize,
}

impl Listing {
    /// # Panics
    ///
    /// When `items_max` is 0.
    #[must_use]
    pub const fn new(items_max: usize) -> Self {
        assert!(items_max > 0, "an outline lists at least one item");
        Self {
            lines: Vec::new(),
            listed: 0,
            items_max,
        }
    }

    /// Lists `file` under the files before it: its items up to the bound, each indented
    /// by nesting, and a count of the rest. Breaks once the outline holds its bound of
    /// items, after which no file is pushed.
    ///
    /// # Panics
    ///
    /// When a file is pushed after the listing broke.
    pub fn push(&mut self, file: &File<'_>) -> ControlFlow<()> {
        assert!(
            self.listed < self.items_max,
            "no file is pushed once the bound is reached"
        );
        self.lines.push(file.path.to_owned());
        let Entry::Outlined {
            language,
            abridgment,
        } = &file.entry
        else {
            let reason = skip_reason(&file.entry);
            self.lines.push(format!("{OUTLINE_INDENT}({reason})"));
            return ControlFlow::Continue(());
        };
        let items = &abridgment.outline.items;
        if items.is_empty() {
            assert_eq!(
                abridgment.left_out, 0,
                "an abridgment keeps items it can list"
            );
            self.lines.push(format!("{OUTLINE_INDENT}(no items)"));
        }
        let Some(room) = self.items_max.checked_sub(self.listed) else {
            unreachable!("the listed items stay within the bound");
        };
        let shown = items.len().min(room);
        for item in items.iter().take(shown) {
            self.lines.push(outline_line(item, *language));
        }
        self.listed = add(self.listed, shown);
        let Some(unshown) = items.len().checked_sub(shown) else {
            unreachable!("no more items are shown than a file holds");
        };
        let left_out = add(unshown, abridgment.left_out);
        if left_out > 0 {
            let noun = noun_for(left_out, "item", "items");
            self.lines
                .push(format!("{OUTLINE_INDENT}({left_out} more {noun} left out)"));
        }
        if let Some(line) = abridgment.outline.first_error_line {
            self.lines.push(format!(
                "{OUTLINE_INDENT}(a syntax error at line {line}; items past it may be missing)"
            ));
        }
        if self.listed == self.items_max {
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    /// The outline, closed by a count of the files left out after the bound was reached.
    ///
    /// # Panics
    ///
    /// When files are left out of a listing that did not reach its bound.
    #[must_use]
    pub fn text(self, files_left_out: usize) -> String {
        let mut lines = self.lines;
        if files_left_out > 0 {
            assert_eq!(
                self.listed, self.items_max,
                "files are left out only once the bound is reached"
            );
            let noun = noun_for(files_left_out, "file", "files");
            lines.push(format!(
                "{files_left_out} more {noun} left out; outline a narrower path"
            ));
        }
        joined(&lines)
    }
}

/// One file's outline as an outline keeps it: no more than its first `items_max` items,
/// and the count of the items it drops.
#[derive(Debug, PartialEq, Eq)]
pub struct Abridgment {
    pub outline: Outline,
    pub left_out: usize,
}

/// `outline` holding no more than its first `items_max` items, counting the rest.
///
/// # Panics
///
/// When the items kept and the items counted differ from the items `outline` held.
#[must_use]
pub fn abridge(outline: Outline, items_max: usize) -> Abridgment {
    let Outline {
        mut items,
        first_error_line,
    } = outline;
    let item_count = items.len();
    let left_out = item_count.saturating_sub(items_max);
    items.truncate(items_max);
    assert_eq!(
        add(items.len(), left_out),
        item_count,
        "every item is kept or counted"
    );
    Abridgment {
        outline: Outline {
            items,
            first_error_line,
        },
        left_out,
    }
}

/// One item's line in an outline: `  104-180 fn plan`, indented by its nesting.
fn outline_line(item: &Item, language: Language) -> String {
    let lines = item.lines();
    let indent = OUTLINE_INDENT.repeat(add(item.depth(), 1));
    format!(
        "{indent}{}-{} {} {}",
        lines.first(),
        lines.last(),
        item.kind().keyword(),
        item.qualified_name(language)
    )
}

/// `singular` for a count of one, `plural` for any other.
const fn noun_for(count: usize, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 { singular } else { plural }
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

/// Up to `LISTED_MATCHES_MAX` headers, indented under a refusal, and a count of the rest
/// of `count`.
fn listed(headers: &[String], count: usize) -> Vec<String> {
    assert!(headers.len() <= count, "every header is counted");
    let mut lines: Vec<String> = headers
        .iter()
        .take(LISTED_MATCHES_MAX)
        .map(|header| format!("  {header}"))
        .collect();
    let Some(unlisted) = count.checked_sub(lines.len()) else {
        unreachable!("no more headers are listed than are counted");
    };
    if unlisted > 0 {
        lines.push(format!("  and {unlisted} more"));
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

/// The refusal for a name `count` items answer, listing `headers`, the first of them, up
/// to the bound.
#[must_use]
pub fn several(name: &str, headers: &[String], count: usize) -> String {
    let mut rendered = vec![format!(
        "excerpt: {count} items answer `{name}`; name one by its kind or a qualified name, or pass --all to print each:"
    )];
    rendered.extend(listed(headers, count));
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
    rendered.extend(listed(member_headers, member_headers.len()));
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
        UsageFault::MissingCommand => "name a command: outline, show, guard, or brief".to_owned(),
        UsageFault::UnknownCommand(word) => {
            format!("`{word}` is not a command; the commands are outline, show, guard, and brief")
        }
        UsageFault::UnknownFlag(flag) => format!("`{flag}` is not a flag excerpt takes"),
        UsageFault::MissingName => "show needs the name of the item to print".to_owned(),
        UsageFault::GuardArgument(word) => {
            format!("guard takes no arguments, and `{word}` is one")
        }
        UsageFault::BriefArgument(word) => {
            format!("brief takes no arguments, and `{word}` is one")
        }
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
        abridgment: &'outline Abridgment,
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

/// What the agent reads when the hook blocks its command: the read that was blocked, and
/// the item commands that read the same file, spelled with its path as the command wrote
/// it.
#[must_use]
pub fn guard_denial(denial: &Denial) -> String {
    let read = match denial.reader {
        Reader::SedRange => "a `sed -n` line-range read",
        Reader::AwkRange => "an `awk` line-number read",
        Reader::WholeFileRead => "a subagent's whole-file Read",
        Reader::Head => "a `head` read",
        Reader::Tail => "a `tail` read",
        Reader::Cat => "a `cat` read",
    };
    let path = &denial.path;
    format!(
        "excerpt guard: blocked {read} of {path}.\n\
         Read code by item instead:\n\
         \x20 excerpt outline {path}\n\
         \x20 excerpt show NAME {path}\n\
         `outline` lists every item in the file with the lines it spans; `show` prints the \
         one NAME names, whole, NAME written as the outline prints it. A line no item \
         holds, such as an import or the line a compiler error names, is read with the Read \
         tool's offset and limit.\n"
    )
}

/// What a subagent's context begins with, before its first prompt: the item commands, the
/// reads the guard refuses, and the read that serves a line no item holds.
#[must_use]
pub fn subagent_brief() -> String {
    let extensions = extension_list();
    format!(
        "Read Rust and TypeScript code ({extensions}) by item with excerpt:\n\
         \x20 excerpt outline PATH\n\
         \x20 excerpt show NAME [PATH]\n\
         `outline` lists every item in a file, or in each source file under a directory, \
         with the lines it spans; `show` prints the one NAME names, whole, NAME written as \
         the outline prints it. The excerpt plugin's hook refuses printing these files' \
         lines from Bash through a `sed -n` line range, `awk` line numbers, `head`, `tail`, \
         or `cat`, and refuses a Read of one past {SUBAGENT_READ_LINES_MAX} lines that names \
         no offset or limit. A line no item holds, such as an import or the line a compiler \
         error names, is read with the Read tool's offset and limit.\n"
    )
}

#[must_use]
pub fn hook_input_unreadable(detail: &str) -> String {
    format!("excerpt guard: could not read the hook input: {detail}\n")
}

#[must_use]
pub fn hook_input_fault(fault: &HookInputFault) -> String {
    let detail = match fault {
        HookInputFault::NotJson(message) => format!("it is not JSON: {message}"),
        HookInputFault::NoToolName => "it holds no `tool_name`".to_owned(),
        HookInputFault::NoCommand => "a Bash call holds no `tool_input.command`".to_owned(),
        HookInputFault::NoFilePath => "a Read call holds no `tool_input.file_path`".to_owned(),
    };
    format!("excerpt guard: the hook input is not what Claude Code sends: {detail}\n")
}

#[cfg(test)]
mod tests;
