//! The imperative shell: walks the paths, reads the files, prints what the library
//! rendered, and turns every outcome into an exit code. Every decision it acts on is made
//! by the library.

use std::fs;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::path::Path;
use std::process::ExitCode;

use excerpt::cli::{self, Command};
use excerpt::exit_code;
use excerpt::guard::{self, HOOK_INPUT_BYTES_MAX, HookInput, Verdict};
use excerpt::item::{self, Outline};
use excerpt::language::Language;
use excerpt::parallel::{self, ATTEMPT_INPUT_BYTES_MAX, Attempt, Pool, Work};
use excerpt::query::{self, Answer};
use excerpt::report::{self, Entry, File, Listing, OUTLINE_ITEMS_MAX};
use excerpt::select::{self, Found, Match, Selection};

/// The largest file read; a larger one is named as skipped.
const FILE_BYTES_MAX: u64 = 2_097_152;
/// The most source files one call reads.
const FILES_MAX: usize = 20_000;
/// The most directory entries one call walks, source files or not.
const ENTRIES_MAX: usize = 1_000_000;
/// The prefix a walk from the working directory puts on every path, which says nothing.
const WORKING_DIRECTORY_PREFIX: &str = "./";

/// mimalloc serves every allocation the process makes on musl.
#[cfg(target_env = "musl")]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
        Ok(Command::Guard) => run_guard(),
        Err(fault) => emit("", &report::usage_fault(&fault), exit_code::USAGE),
    };
    ExitCode::from(code)
}

/// A source file to read: the path it is shown by, and its language.
struct Source {
    path: String,
    language: Language,
}

/// A source file opened for reading, or the size that keeps it closed.
enum Opened {
    File { file: fs::File, bytes: u64 },
    TooLarge(u64),
}

/// What reading an opened file gave.
enum Contents {
    Text(String),
    NotText,
    /// The file holds more bytes than the read allowed, having grown since its size was
    /// read.
    Longer,
}

/// What a read bounded to some bytes gave: every byte the source held, or that it held
/// more.
enum Bounded {
    Bytes(Vec<u8>),
    Longer,
}

/// What became of one source file: what was made of its text, or why it has none.
enum Sourced<T> {
    Used(T),
    TooLarge(u64),
    NotText,
    Failed(io::Error),
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

fn open_source(path: &str) -> io::Result<Opened> {
    let file = fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    if bytes > FILE_BYTES_MAX {
        return Ok(Opened::TooLarge(bytes));
    }
    Ok(Opened::File { file, bytes })
}

/// Reads up to `bytes_max` bytes of an opened file, telling a file that holds more apart.
fn read_contents(file: &fs::File, bytes_max: u64) -> io::Result<Contents> {
    let Bounded::Bytes(buffer) = read_bounded(file, bytes_max)? else {
        return Ok(Contents::Longer);
    };
    match String::from_utf8(buffer) {
        Ok(text) => Ok(Contents::Text(text)),
        Err(_) => Ok(Contents::NotText),
    }
}

/// Reads up to `bytes_max` bytes, telling a source that holds more apart.
fn read_bounded(source: impl Read, bytes_max: u64) -> io::Result<Bounded> {
    let Some(read_max) = bytes_max.checked_add(1) else {
        unreachable!("a byte bound is below u64::MAX");
    };
    let mut buffer = Vec::new();
    source.take(read_max).read_to_end(&mut buffer)?;
    let Ok(read_bytes) = u64::try_from(buffer.len()) else {
        unreachable!("a buffer's length fits a u64");
    };
    if read_bytes > bytes_max {
        return Ok(Bounded::Longer);
    }
    assert!(
        read_bytes <= bytes_max,
        "a bounded read holds no more than its bound"
    );
    Ok(Bounded::Bytes(buffer))
}

/// The Claude Code hook: judges the Bash call on stdin and exits with the code the hook
/// contract gives the verdict. An input it cannot use is reported without blocking, so a
/// change in what Claude Code sends never stops every Bash call.
fn run_guard() -> u8 {
    let input = match read_bounded(io::stdin().lock(), HOOK_INPUT_BYTES_MAX) {
        Ok(Bounded::Bytes(input)) => input,
        Ok(Bounded::Longer) => {
            let message = report::hook_input_unreadable("it is longer than excerpt guard reads");
            return emit("", &message, exit_code::HOOK_ERROR);
        }
        Err(error) => {
            let message = report::hook_input_unreadable(&error.to_string());
            return emit("", &message, exit_code::HOOK_ERROR);
        }
    };
    let command = match guard::hook_input(&input) {
        Ok(HookInput::Bash(command)) => command,
        Ok(HookInput::OtherTool) => return exit_code::SUCCESS,
        Err(fault) => return emit("", &report::hook_input_fault(&fault), exit_code::HOOK_ERROR),
    };
    match guard::verdict(&command) {
        Verdict::Allow => exit_code::SUCCESS,
        Verdict::Deny(denial) => emit("", &report::guard_denial(&denial), exit_code::HOOK_BLOCK),
    }
}

/// One source file read and its text handed to `use_text`, or why it has no text.
fn use_source<T>(source: &Source, use_text: &impl Fn(&Source, String) -> T) -> Sourced<T> {
    let file = match open_source(&source.path) {
        Ok(Opened::File { file, .. }) => file,
        Ok(Opened::TooLarge(bytes)) => return Sourced::TooLarge(bytes),
        Err(error) => return Sourced::Failed(error),
    };
    match read_contents(&file, FILE_BYTES_MAX) {
        Ok(Contents::Text(text)) => Sourced::Used(use_text(source, text)),
        Ok(Contents::NotText) => Sourced::NotText,
        Ok(Contents::Longer) => grown(&file),
        Err(error) => Sourced::Failed(error),
    }
}

/// A file that grew past `FILE_BYTES_MAX` after its size was read, by the size it has now.
fn grown<T>(file: &fs::File) -> Sourced<T> {
    match file.metadata() {
        Ok(metadata) => Sourced::TooLarge(metadata.len()),
        Err(error) => Sourced::Failed(error),
    }
}

/// As `use_source`, except that a file an attempt defers is left unread.
fn attempt_source<T>(
    source: &Source,
    use_text: &impl Fn(&Source, String) -> T,
) -> Attempt<Sourced<T>> {
    let file = match open_source(&source.path) {
        Ok(Opened::File { file, bytes }) => {
            if parallel::attempt_defers(bytes) {
                return Attempt::Deferred;
            }
            file
        }
        Ok(Opened::TooLarge(bytes)) => return Attempt::Done(Sourced::TooLarge(bytes)),
        Err(error) => return Attempt::Done(Sourced::Failed(error)),
    };
    match read_contents(&file, ATTEMPT_INPUT_BYTES_MAX) {
        Ok(Contents::Text(text)) => Attempt::Done(Sourced::Used(use_text(source, text))),
        Ok(Contents::NotText) => Attempt::Done(Sourced::NotText),
        Ok(Contents::Longer) => Attempt::Deferred,
        Err(error) => Attempt::Done(Sourced::Failed(error)),
    }
}

/// Every source's outcome handed to `consume` in walk order until it breaks, the files an
/// attempt defers read by few threads at once, since a parse's memory grows with its file.
fn consume_sources<'sources, T: Send>(
    sources: &'sources [Source],
    use_text: impl Fn(&Source, String) -> T + Sync,
    consume: impl FnMut(&'sources Source, Sourced<T>) -> ControlFlow<()>,
) {
    let pool = Pool::new(available_threads());
    let work = Work {
        attempt: |source: &Source| attempt_source(source, &use_text),
        deferred: |source: &Source| use_source(source, &use_text),
    };
    pool.consume_in_order(sources, &work, consume);
}

/// The threads the machine offers this process, or one where it cannot say.
fn available_threads() -> NonZeroUsize {
    match std::thread::available_parallelism() {
        Ok(available) => available,
        Err(_) => NonZeroUsize::MIN,
    }
}

fn run_outline(paths: &[String]) -> u8 {
    let collected = match collect_sources(paths) {
        Ok(collected) => collected,
        Err((code, message)) => return emit("", &message, code),
    };
    let mut failures = collected.failures;
    let mut listing = Listing::new(OUTLINE_ITEMS_MAX);
    let mut taken: usize = 0;
    let abridge_source = |source: &Source, text: String| {
        let outline = item::outline(source.language, &text);
        report::abridge(outline, OUTLINE_ITEMS_MAX)
    };
    consume_sources(&collected.sources, abridge_source, |source, outcome| {
        taken = add(taken, 1);
        let entry = match &outcome {
            Sourced::Used(abridgment) => Entry::Outlined {
                language: source.language,
                abridgment,
            },
            Sourced::TooLarge(bytes) => too_large(*bytes),
            Sourced::NotText => Entry::NotText,
            Sourced::Failed(error) => {
                failures.push_str(&report::read_failure(&source.path, &error.to_string()));
                return ControlFlow::Continue(());
            }
        };
        listing.push(&File {
            path: &source.path,
            entry,
        })
    });
    let Some(left_out) = collected.sources.len().checked_sub(taken) else {
        unreachable!("no more sources are taken than were collected");
    };
    let text = listing.text(left_out);
    let code = if failures.is_empty() {
        exit_code::SUCCESS
    } else {
        exit_code::IO
    };
    emit(&text, &failures, code)
}

/// A file of `bytes` the read bound kept closed.
const fn too_large(bytes: u64) -> Entry<'static> {
    Entry::TooLarge {
        bytes,
        bytes_max: FILE_BYTES_MAX,
    }
}

/// A file `show` found a match in, kept with its text so the match can be printed.
struct Loaded {
    path: String,
    language: Language,
    outline: Outline,
    text: String,
}

/// One file's answer to the name `show` asks for, keeping the text only where items answer,
/// since those are printed from it.
enum Kept {
    Answered {
        outline: Outline,
        items: Vec<usize>,
        text: String,
    },
    Unanswered,
}

/// What `show` found across the files it read, in walk order: the files holding the
/// answering items it kept, the matches, and the notes on files skipped or failed.
struct Gathered {
    loaded: Vec<Loaded>,
    found: Found,
    notes: String,
    failed: bool,
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
    let gathered = gather(collected, |source, text| {
        let answer = query.answer_in(source.language, &text);
        kept(answer, text)
    });
    let selection = select::select(gathered.found, all);
    let (printed, refusal) = render_selection(name, paths, &gathered.loaded, selection);
    let mut notes = gathered.notes;
    notes.push_str(&refusal);
    let code = if gathered.failed {
        exit_code::IO
    } else if refusal.is_empty() {
        exit_code::SUCCESS
    } else {
        exit_code::REFUSED
    };
    emit(&printed, &notes, code)
}

/// A file's answer to the name, kept with its text where items answer.
fn kept(answer: Answer, text: String) -> Kept {
    match answer {
        Answer::Answered { outline, items } => Kept::Answered {
            outline,
            items,
            text,
        },
        Answer::Screened | Answer::Unanswered => Kept::Unanswered,
    }
}

/// Every source's answer in walk order, each answering item kept numbered by its file's
/// place among the files kept; a file holding no kept item is dropped with its text.
fn gather(
    collected: Collected,
    answer_source: impl Fn(&Source, String) -> Kept + Sync,
) -> Gathered {
    let failed = !collected.failures.is_empty();
    let mut gathered = Gathered {
        loaded: Vec::new(),
        found: Found::default(),
        notes: collected.failures,
        failed,
    };
    consume_sources(&collected.sources, answer_source, |source, outcome| {
        match outcome {
            Sourced::Used(Kept::Answered {
                outline,
                items,
                text,
            }) => {
                let file = gathered.loaded.len();
                if gathered.found.add(file, &items) {
                    gathered.loaded.push(Loaded {
                        path: source.path.clone(),
                        language: source.language,
                        outline,
                        text,
                    });
                }
            }
            Sourced::Used(Kept::Unanswered) => {}
            Sourced::TooLarge(bytes) => {
                let note = report::skipped(&source.path, &too_large(bytes));
                gathered.notes.push_str(&note);
            }
            Sourced::NotText => {
                let note = report::skipped(&source.path, &Entry::NotText);
                gathered.notes.push_str(&note);
            }
            Sourced::Failed(error) => {
                let note = report::read_failure(&source.path, &error.to_string());
                gathered.notes.push_str(&note);
                gathered.failed = true;
            }
        }
        ControlFlow::Continue(())
    });
    gathered
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
            for matched in chosen {
                let (file, item) = resolve(loaded, matched);
                if select::printable(item) {
                    printed.push(report::item_text(
                        &file.path,
                        file.language,
                        item,
                        &file.text,
                    ));
                } else {
                    refusal.push_str(&too_long(file, matched.item));
                }
            }
            (printed.join("\n"), refusal)
        }
        Selection::Missing => (String::new(), report::missing(name, paths)),
        Selection::Several { matches, count } => {
            let headers: Vec<String> = matches
                .into_iter()
                .map(|matched| {
                    let (file, item) = resolve(loaded, matched);
                    report::header(&file.path, file.language, item)
                })
                .collect();
            (String::new(), report::several(name, &headers, count))
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
