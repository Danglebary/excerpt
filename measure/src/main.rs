//! The shell: reads the arguments and the home folder, walks the transcript store, reads
//! each transcript written since the window opened a line at a time, and prints the
//! tables the library rendered.

use std::fs;
use std::io::{self, Write as _};
use std::path::Path;
use std::process::ExitCode;

use claude_code_transcript::lines::{LINES_PER_TRANSCRIPT_MAX, Lines, Piece};
use claude_code_transcript::record::{Line, read_line};
use claude_code_transcript::store::{self, Found};
use excerpt_measure::Full;
use excerpt_measure::cli::{self, Command, Request};
use excerpt_measure::facts::{Facts, Fold};
use excerpt_measure::report::{self, PassedOver};
use excerpt_measure::tally::Tally;
use excerpt_measure::window::Window;
use jiff::Timestamp;

const SUCCESS: u8 = 0;
const FAILED: u8 = 1;
const USAGE: u8 = 2;

/// A measure's counts, and what it passed over reaching them.
struct Measured {
    tally: Tally,
    passed_over: PassedOver,
}

/// One transcript's facts, and what reading it passed over.
struct Read {
    facts: Facts,
    passed_over: PassedOver,
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let code = match cli::parse_arguments(&arguments) {
        Ok(Command::Help) => emit(report::HELP, "", SUCCESS),
        Ok(Command::Measure(request)) => run(&request),
        Err(fault) => emit("", &report::usage_fault(&fault), USAGE),
    };
    ExitCode::from(code)
}

/// Measures the store beneath the home folder and prints the tables, or why it could not.
fn run(request: &Request) -> u8 {
    let Some(home) = std::env::var_os("HOME") else {
        return emit("", &report::no_home(), FAILED);
    };
    let projects = store::projects_folder(Path::new(&home));
    match measure(&projects, request) {
        Ok(measured) => {
            let tables = measured.tally.tables();
            let notes = report::passed_over(&measured.passed_over);
            emit(&tables, &notes, SUCCESS)
        }
        Err(message) => emit("", &message, FAILED),
    }
}

/// Folds every transcript of the store into the measure the request names, or says why
/// the store could not be read or which bound the measure reached.
fn measure(projects: &Path, request: &Request) -> Result<Measured, String> {
    let folders = store::folders(projects).map_err(|fault| report::store_fault(&fault))?;
    let mut tally = Tally::new(request.measure);
    let mut passed_over = PassedOver::default();
    for folder in &folders {
        let transcripts =
            store::transcripts_in(folder).map_err(|fault| report::store_fault(&fault))?;
        for found in &transcripts {
            let read = read_transcript(found, request.window).map_err(report::full)?;
            tally.take(found, &read.facts).map_err(report::full)?;
            passed_over = passed_over.plus(&read.passed_over);
        }
    }
    Ok(Measured { tally, passed_over })
}

/// Reads one transcript into its facts. A transcript last written before the window opened
/// is not read, and one that fails to open or to read is counted unread with no facts.
fn read_transcript(found: &Found, window: Window) -> Result<Read, Full> {
    let unread = PassedOver {
        transcripts_unread: 1,
        ..PassedOver::default()
    };
    if !written_since(found, window) {
        let passed_over = PassedOver::default();
        return Ok(Read {
            facts: Facts::default(),
            passed_over,
        });
    }
    let Ok(mut lines) = Lines::open(&found.path) else {
        return Ok(Read {
            facts: Facts::default(),
            passed_over: unread,
        });
    };
    let mut fold = Fold::new(window);
    let mut passed_over = PassedOver::default();
    for _ in 0..=LINES_PER_TRANSCRIPT_MAX {
        let met = match lines.next_piece() {
            Ok(Piece::Text { text, .. }) => {
                let line = read_line(&text);
                fold.take(&line)?;
                match line {
                    Line::Malformed(_) => PassedOver {
                        lines_malformed: 1,
                        ..PassedOver::default()
                    },
                    Line::Assistant(_) | Line::User(_) | Line::Attachment(_) | Line::Other => {
                        PassedOver::default()
                    }
                }
            }
            Ok(Piece::TooLong { .. }) => PassedOver {
                lines_too_long: 1,
                ..PassedOver::default()
            },
            Ok(Piece::NotText { .. }) => PassedOver {
                lines_not_text: 1,
                ..PassedOver::default()
            },
            Ok(Piece::End) => return Ok(finished(fold, passed_over)),
            Ok(Piece::PastTheBound) => {
                let cut = PassedOver {
                    transcripts_cut: 1,
                    ..PassedOver::default()
                };
                return Ok(finished(fold, passed_over.plus(&cut)));
            }
            Err(_) => {
                return Ok(Read {
                    facts: Facts::default(),
                    passed_over: unread,
                });
            }
        };
        passed_over = passed_over.plus(&met);
    }
    unreachable!("a transcript's read stops at its line bound")
}

/// The facts a fold finished with, and what reading the transcript passed over.
fn finished(fold: Fold, passed_over: PassedOver) -> Read {
    let facts = fold.finish();
    let unheld = PassedOver {
        calls_unheld: facts.unheld,
        ..PassedOver::default()
    };
    Read {
        facts,
        passed_over: passed_over.plus(&unheld),
    }
}

/// Whether the transcript was last written at or after the window opened. One whose last
/// write cannot be read is read regardless, so the read reports it.
fn written_since(found: &Found, window: Window) -> bool {
    let modified = fs::metadata(&found.path).and_then(|metadata| metadata.modified());
    let instant = modified
        .ok()
        .and_then(|time| Timestamp::try_from(time).ok());
    match instant {
        Some(instant) => !window.opens_after(instant),
        None => true,
    }
}

/// Writes the output and the messages and returns `code`, or the failure code when either
/// cannot be written for any reason but a reader that stopped reading.
fn emit(output: &str, messages: &str, code: u8) -> u8 {
    let mut code = code;
    if !output.is_empty() {
        let written = io::stdout().lock().write_all(output.as_bytes());
        if failed_for_a_reader(written) {
            code = FAILED;
        }
    }
    if !messages.is_empty() {
        let written = io::stderr().lock().write_all(messages.as_bytes());
        if failed_for_a_reader(written) {
            code = FAILED;
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
