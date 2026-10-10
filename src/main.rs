//! The imperative shell: walks the paths, reads the files, prints what the library
//! rendered, and turns every outcome into an exit code. Every decision it acts on is made
//! by the library.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use excerpt::cli::{self, Command};
use excerpt::exit_code;
use excerpt::item::{self, Outline};
use excerpt::language::Language;
use excerpt::query;
use excerpt::report::{self, Entry, File, OUTLINE_ITEMS_MAX};
use excerpt::select::{self, Match, Selection};

/// The largest file read; a larger one is named as skipped.
const FILE_BYTES_MAX: u64 = 16_777_216;
/// The most source files one call reads.
const FILES_MAX: usize = 20_000;
/// The most directory entries one call walks, source files or not.
const ENTRIES_MAX: usize = 1_000_000;
/// The prefix a walk from the working directory puts on every path, which says nothing.
const WORKING_DIRECTORY_PREFIX: &str = "./";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let code = match cli::parse_arguments(&arguments) {
        Ok(Command::Help) => emit(report::HELP, "", exit_code::SUCCESS),
        Ok(Command::Version) => {
            let version = format!("excerpt {}\n", env!("CARGO_PKG_VERSION"));
            emit(&version, "", exit_code::SUCCESS)
        }
        Ok(Command::Outline { paths }) => run_outline(&paths),
        Ok(Command::Show { name, all, paths }) => run_show(&name, all, &paths),
        Err(fault) => emit("", &report::usage_fault(&fault), exit_code::USAGE),
    };
    ExitCode::from(code)
}

/// A source file to read: the path it is shown by, and its language.
struct Source {
    path: String,
    language: Language,
}

/// What reading one file gave.
enum Contents {
    Text(String),
    TooLarge(u64),
    NotText,
}

/// The source files under the paths, in walk order, or the code and message that end the
/// call first: a named path that cannot be read or is not a source file, or more files
/// than a call reads. A failure inside a directory is a message and the walk goes on.
struct Collected {
    sources: Vec<Source>,
    failures: String,
}

fn collect_sources(paths: &[String]) -> Result<Collected, (u8, String)> {
    for path in paths {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err((
                    exit_code::IO,
                    report::read_failure(path, &error.to_string()),
                ));
            }
        };
        if metadata.is_file() && Language::of_path(Path::new(path)).is_none() {
            return Err((exit_code::REFUSED, report::unsupported(path)));
        }
    }
    let mut sources = Vec::new();
    let mut failures = String::new();
    let mut entries: usize = 0;
    for path in paths {
        for walked in walker(path) {
            entries = add(entries, 1);
            if entries > ENTRIES_MAX {
                return Err((exit_code::REFUSED, report::too_many_files(FILES_MAX)));
            }
            let entry = match walked {
                Ok(entry) => entry,
                Err(error) => {
                    failures.push_str(&report::read_failure(path, &error.to_string()));
                    continue;
                }
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(language) = Language::of_path(entry.path()) else {
                continue;
            };
            sources.push(Source {
                path: shown_path(entry.path()),
                language,
            });
            if sources.len() > FILES_MAX {
                return Err((exit_code::REFUSED, report::too_many_files(FILES_MAX)));
            }
        }
    }
    if sources.is_empty() {
        return Err((exit_code::REFUSED, report::no_sources(paths)));
    }
    Ok(Collected { sources, failures })
}

/// A walk of one path as git sees it, every option stated: hidden files and whatever
/// `.gitignore`, `.ignore`, and git's exclude files name are skipped, in a repository or
/// not, and symbolic links are not followed. Entries come sorted by name, so the output
/// is the same on every run.
fn walker(path: &str) -> ignore::Walk {
    ignore::WalkBuilder::new(path)
        .hidden(true)
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .require_git(false)
        .follow_links(false)
        .sort_by_file_name(std::ffi::OsStr::cmp)
        .build()
}

fn shown_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(WORKING_DIRECTORY_PREFIX) {
        Some(rest) => rest.to_owned(),
        None => text.into_owned(),
    }
}

fn read_source(path: &str) -> io::Result<Contents> {
    let file = fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    if bytes > FILE_BYTES_MAX {
        return Ok(Contents::TooLarge(bytes));
    }
    let mut buffer = Vec::new();
    file.take(FILE_BYTES_MAX).read_to_end(&mut buffer)?;
    match String::from_utf8(buffer) {
        Ok(text) => Ok(Contents::Text(text)),
        Err(_) => Ok(Contents::NotText),
    }
}

fn run_outline(paths: &[String]) -> u8 {
    let collected = match collect_sources(paths) {
        Ok(collected) => collected,
        Err((code, message)) => return emit("", &message, code),
    };
    let mut failures = collected.failures;
    let mut read: Vec<(String, ReadOutline)> = Vec::new();
    for source in collected.sources {
        let state = match read_source(&source.path) {
            Ok(Contents::Text(text)) => {
                ReadOutline::Outlined(source.language, item::outline(source.language, &text))
            }
            Ok(Contents::TooLarge(bytes)) => ReadOutline::TooLarge(bytes),
            Ok(Contents::NotText) => ReadOutline::NotText,
            Err(error) => {
                failures.push_str(&report::read_failure(&source.path, &error.to_string()));
                continue;
            }
        };
        read.push((source.path, state));
    }
    let files: Vec<File<'_>> = read
        .iter()
        .map(|(path, state)| File {
            path,
            entry: state.entry(),
        })
        .collect();
    let text = report::outline_text(&files, OUTLINE_ITEMS_MAX);
    let code = if failures.is_empty() {
        exit_code::SUCCESS
    } else {
        exit_code::IO
    };
    emit(&text, &failures, code)
}

/// One file as an outline read it, owning the outline the report borrows.
enum ReadOutline {
    Outlined(Language, Outline),
    TooLarge(u64),
    NotText,
}

impl ReadOutline {
    const fn entry(&self) -> Entry<'_> {
        match self {
            Self::Outlined(language, outline) => Entry::Outlined {
                language: *language,
                outline,
            },
            Self::TooLarge(bytes) => Entry::TooLarge {
                bytes: *bytes,
                bytes_max: FILE_BYTES_MAX,
            },
            Self::NotText => Entry::NotText,
        }
    }
}

/// A file `show` found a match in, kept with its text so the match can be printed.
struct Loaded {
    path: String,
    language: Language,
    outline: Outline,
    text: String,
}

fn run_show(name: &str, all: bool, paths: &[String]) -> u8 {
    let query = match query::parse(name) {
        Ok(query) => query,
        Err(fault) => return emit("", &report::query_fault(&fault), exit_code::USAGE),
    };
    let collected = match collect_sources(paths) {
        Ok(collected) => collected,
        Err((code, message)) => return emit("", &message, code),
    };
    let mut notes = collected.failures;
    let mut failed = !notes.is_empty();
    let mut loaded: Vec<Loaded> = Vec::new();
    let mut matches: Vec<Match> = Vec::new();
    for source in collected.sources {
        let text = match read_source(&source.path) {
            Ok(Contents::Text(text)) => text,
            Ok(Contents::TooLarge(bytes)) => {
                let entry = ReadOutline::TooLarge(bytes);
                notes.push_str(&report::skipped(&source.path, &entry.entry()));
                continue;
            }
            Ok(Contents::NotText) => {
                notes.push_str(&report::skipped(&source.path, &Entry::NotText));
                continue;
            }
            Err(error) => {
                notes.push_str(&report::read_failure(&source.path, &error.to_string()));
                failed = true;
                continue;
            }
        };
        if !query.may_answer_in(&text) {
            continue;
        }
        let outline = item::outline(source.language, &text);
        let answering: Vec<usize> = (0..outline.items.len())
            .filter(|index| {
                outline
                    .items
                    .get(*index)
                    .is_some_and(|item| query.answers(item))
            })
            .collect();
        if answering.is_empty() {
            continue;
        }
        let file = loaded.len();
        for item in answering {
            matches.push(Match { file, item });
        }
        loaded.push(Loaded {
            path: source.path,
            language: source.language,
            outline,
            text,
        });
    }
    let (printed, refusal) = render_selection(name, paths, &loaded, select::select(matches, all));
    notes.push_str(&refusal);
    let code = if failed {
        exit_code::IO
    } else if refusal.is_empty() {
        exit_code::SUCCESS
    } else {
        exit_code::REFUSED
    };
    emit(&printed, &notes, code)
}

/// What `show` prints for a selection, and the refusal it reports, empty when there is
/// none.
fn render_selection(
    name: &str,
    paths: &[String],
    loaded: &[Loaded],
    selection: Selection,
) -> (String, String) {
    match selection {
        Selection::Print(chosen) => {
            let mut printed = Vec::new();
            let mut refusal = String::new();
            for found in chosen {
                let (file, item) = resolve(loaded, found);
                if select::printable(item) {
                    printed.push(report::item_text(
                        &file.path,
                        file.language,
                        item,
                        &file.text,
                    ));
                } else {
                    refusal.push_str(&too_long(file, found.item));
                }
            }
            (printed.join("\n"), refusal)
        }
        Selection::Missing => (String::new(), report::missing(name, paths)),
        Selection::Several(chosen) => {
            let headers: Vec<String> = chosen
                .into_iter()
                .map(|found| {
                    let (file, item) = resolve(loaded, found);
                    report::header(&file.path, file.language, item)
                })
                .collect();
            (String::new(), report::several(name, &headers))
        }
        Selection::TooMany(count) => (String::new(), report::too_many(name, count)),
    }
}

fn too_long(file: &Loaded, index: usize) -> String {
    let Some(item) = file.outline.items.get(index) else {
        unreachable!("a match names an item of its file's outline");
    };
    let members: Vec<String> = item::members(&file.outline.items, index)
        .iter()
        .map(|member| report::header(&file.path, file.language, member))
        .collect();
    let header = report::header(&file.path, file.language, item);
    report::too_long(&header, item.lines().count(), &members)
}

fn resolve(loaded: &[Loaded], found: Match) -> (&Loaded, &item::Item) {
    let Some(file) = loaded.get(found.file) else {
        unreachable!("a match names a loaded file");
    };
    let Some(item) = file.outline.items.get(found.item) else {
        unreachable!("a match names an item of its file's outline");
    };
    (file, item)
}

/// Writes the output and the messages and returns `code`, or the IO code when either
/// cannot be written for any reason but a reader that stopped reading.
fn emit(output: &str, messages: &str, code: u8) -> u8 {
    let mut code = code;
    if !output.is_empty() {
        let written = io::stdout().lock().write_all(output.as_bytes());
        if failed_for_a_reader(written) {
            code = exit_code::IO;
        }
    }
    if !messages.is_empty() {
        let written = io::stderr().lock().write_all(messages.as_bytes());
        if failed_for_a_reader(written) {
            code = exit_code::IO;
        }
    }
    code
}

/// Whether a write failed while its reader was still reading; a closed pipe means the
/// reader took what it wanted, as `| head` does.
fn failed_for_a_reader(written: io::Result<()>) -> bool {
    match written {
        Ok(()) => false,
        Err(error) => error.kind() != io::ErrorKind::BrokenPipe,
    }
}

fn add(left: usize, right: usize) -> usize {
    let Some(sum) = left.checked_add(right) else {
        unreachable!("a count of entries fits in a usize");
    };
    sum
}
