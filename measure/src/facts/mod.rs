//! What one transcript holds that the measures count: each Bash call naming a code file
//! and each Read of one, made inside the window, with what its result came to.
//!
//! Pure: the shell hands in each line as it reads it, so a transcript replays from fixture
//! lines.

use std::collections::HashMap;
use std::path::Path;

use claude_code_transcript::calls::{Awaited, Completed, Paired, Pairing, calls_of, results_of};
use claude_code_transcript::record::{Assistant, Line, ResultPart, ToolResult, ToolUse, User};
use excerpt::guard::{ReadSpan, read_span};
use excerpt::language::{EXTENSIONS, Language};
use excerpt::report::GUARD_DENIAL_OPENING;
use serde_json::Value;

use crate::window::Window;
use crate::{Full, one_more};

/// The most calls one transcript's facts hold.
pub const FACTS_PER_TRANSCRIPT_MAX: usize = 100_000;

/// The tools whose calls the measures count, by the names their calls carry.
const BASH_TOOL: &str = "Bash";
const READ_TOOL: &str = "Read";

/// Claude Code records a `PreToolUse` hook's refusal as the call's error result, its text
/// `PreToolUse:<tool> hook error: [<command>]: <stderr>`, the command being the hook's.
/// Claude Code documents no transcript schema; the form was read off all 496 refusals in
/// this machine's transcripts on 2026-10-30.
const HOOK_ERROR_OPENING: &str = "PreToolUse:";
const GUARD_HOOK_LABEL: &str = " hook error: [excerpt guard]: ";

/// The calls one transcript made inside the window, each with what its result came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// Each Bash call naming a code file, in the order its result arrived.
    pub commands: Vec<CodeCommand>,
    /// Each Read of a code file, in the order its result arrived.
    pub reads: Vec<CodeRead>,
    /// The calls the pairing handed back, each a duplicate of an id awaiting its result or
    /// past the bound of calls awaiting.
    pub unheld: usize,
}

/// A Bash call naming a code file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCommand {
    pub command: String,
    pub refused: bool,
}

/// A Read of a code file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRead {
    pub span: ReadSpan,
    pub refused: bool,
    pub result: ReadResult,
    /// The model calls the transcript records after the one making the Read.
    pub calls_after: usize,
}

/// What a Read's result holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadResult {
    /// The file's lines, numbered from 1: how many, and the result's characters.
    Numbered { lines: usize, characters: usize },
    /// No numbered lines, such as a refusal or a file that could not be read.
    Unnumbered,
}

/// A call the measures count, as its input names it.
enum Counted<'call> {
    Command(&'call str),
    Read,
}

/// A Read whose transcript has not ended, which decides how many calls came after it.
struct PendingRead {
    span: ReadSpan,
    refused: bool,
    result: ReadResult,
    ordinal: usize,
}

/// Folds one transcript's lines, in order, into its facts.
pub struct Fold {
    window: Window,
    pairing: Pairing,
    /// Each model call's place among the transcript's model calls, by its message id.
    ordinals: HashMap<String, usize>,
    /// The place of the model call making each Read awaiting its result, by the Read's id.
    read_ordinals: HashMap<String, usize>,
    commands: Vec<CodeCommand>,
    reads: Vec<PendingRead>,
    unheld: usize,
}

impl Fold {
    /// A fold counting the calls `window` holds, having taken no line.
    #[must_use]
    pub fn new(window: Window) -> Self {
        Self {
            window,
            pairing: Pairing::new(),
            ordinals: HashMap::new(),
            read_ordinals: HashMap::new(),
            commands: Vec::new(),
            reads: Vec::new(),
            unheld: 0,
        }
    }

    /// Takes the transcript's next line.
    ///
    /// # Errors
    ///
    /// [`Full::Facts`] where a result would take the transcript's facts past
    /// [`FACTS_PER_TRANSCRIPT_MAX`].
    pub fn take(&mut self, line: &Line) -> Result<(), Full> {
        match line {
            Line::Assistant(assistant) => {
                self.take_calls(assistant);
                Ok(())
            }
            Line::User(user) => self.take_results(user),
            Line::Attachment(_) | Line::Other | Line::Malformed(_) => Ok(()),
        }
    }

    /// The facts of every line taken, each Read's later calls counted to the transcript's
    /// end.
    ///
    /// # Panics
    ///
    /// When a Read's call stands outside the transcript's model calls, which no sequence of
    /// lines can cause.
    #[must_use]
    pub fn finish(self) -> Facts {
        let calls = self.ordinals.len();
        let mut reads = Vec::with_capacity(self.reads.len());
        for pending in self.reads {
            assert!(pending.ordinal < calls, "a Read's call is the transcript's");
            let Some(calls_after) = calls
                .checked_sub(pending.ordinal)
                .and_then(|from_the_call| from_the_call.checked_sub(1))
            else {
                unreachable!("a Read's call stands before the transcript's end")
            };
            reads.push(CodeRead {
                span: pending.span,
                refused: pending.refused,
                result: pending.result,
                calls_after,
            });
        }
        Facts {
            commands: self.commands,
            reads,
            unheld: self.unheld,
        }
    }

    /// Holds each call the record makes that a measure counts until its result arrives.
    fn take_calls(&mut self, assistant: &Assistant) {
        let ordinal = self.ordinal_of(&assistant.message_id);
        for call in calls_of(assistant) {
            if self.window.holds(call.envelope.timestamp) {
                let is_read = match counted(&call.tool_use) {
                    Some(Counted::Read) => true,
                    Some(Counted::Command(_)) => false,
                    None => continue,
                };
                let id = call.tool_use.id.clone();
                match self.pairing.await_call(call) {
                    Awaited::Held => {
                        if is_read {
                            let earlier = self.read_ordinals.insert(id, ordinal);
                            assert!(earlier.is_none(), "a Read awaits its result once");
                        }
                    }
                    Awaited::Duplicate(_) | Awaited::Full(_) => self.unheld = one_more(self.unheld),
                }
            }
        }
        assert!(
            self.read_ordinals.len() <= self.pairing.awaiting(),
            "each Read with a place awaits its result"
        );
    }

    /// Pairs each result the record holds with the call it returns from.
    fn take_results(&mut self, user: &User) -> Result<(), Full> {
        for result in results_of(user) {
            if let Completed::Paired(paired) = self.pairing.complete(result) {
                let Some(held) = self.commands.len().checked_add(self.reads.len()) else {
                    unreachable!("the facts held stay below their bound")
                };
                if held < FACTS_PER_TRANSCRIPT_MAX {
                    self.take_paired(paired);
                } else {
                    return Err(Full::Facts);
                }
            }
        }
        Ok(())
    }

    /// Keeps a counted call and what its result came to.
    fn take_paired(&mut self, paired: Paired) {
        let tool_use = paired.call.tool_use;
        let text = text_of(&paired.result);
        let refused = is_refusal(&paired.result, &tool_use.name, &text);
        match counted(&tool_use) {
            Some(Counted::Command(command)) => self.commands.push(CodeCommand {
                command: command.to_owned(),
                refused,
            }),
            Some(Counted::Read) => {
                let Some(ordinal) = self.read_ordinals.remove(&tool_use.id) else {
                    unreachable!("a Read held awaits with the place of its call")
                };
                let result = if refused {
                    ReadResult::Unnumbered
                } else {
                    read_result_of(&text)
                };
                self.reads.push(PendingRead {
                    span: read_span(&tool_use.input),
                    refused,
                    result,
                    ordinal,
                });
            }
            None => unreachable!("only a counted call is held"),
        }
    }

    /// The place of the model call `message_id` names among the transcript's model calls.
    fn ordinal_of(&mut self, message_id: &str) -> usize {
        if let Some(ordinal) = self.ordinals.get(message_id) {
            *ordinal
        } else {
            let ordinal = self.ordinals.len();
            self.ordinals.insert(message_id.to_owned(), ordinal);
            ordinal
        }
    }
}

/// Whether a measure counts the call, and as which.
fn counted(tool_use: &ToolUse) -> Option<Counted<'_>> {
    match tool_use.name.as_str() {
        BASH_TOOL => {
            let command = tool_use.input.get("command").and_then(Value::as_str)?;
            if names_code_file(command) {
                Some(Counted::Command(command))
            } else {
                None
            }
        }
        READ_TOOL => {
            let path = tool_use.input.get("file_path").and_then(Value::as_str)?;
            Language::of_path(Path::new(path)).map(|_| Counted::Read)
        }
        _ => None,
    }
}

/// Whether `command` names a file in a language excerpt reads: one of its extensions,
/// after a dot and before no letter, digit, or underscore.
fn names_code_file(command: &str) -> bool {
    for extension in EXTENSIONS {
        let suffix = format!(".{extension}");
        for (start, matched) in command.match_indices(suffix.as_str()) {
            let Some(end) = start.checked_add(matched.len()) else {
                unreachable!("a match ends within its text")
            };
            let next = command.get(end..).and_then(|rest| rest.chars().next());
            let ends_word = match next {
                Some(character) => !is_word_character(character),
                None => true,
            };
            if ends_word {
                return true;
            }
        }
    }
    false
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// The text of a result's text parts, joined.
fn text_of(result: &ToolResult) -> String {
    let mut text = String::new();
    for part in &result.parts {
        if let ResultPart::Text(part_text) = part {
            text.push_str(part_text);
        }
    }
    text
}

/// Whether the result is the guard's refusal of a call to `tool`.
fn is_refusal(result: &ToolResult, tool: &str, text: &str) -> bool {
    if result.is_error {
        let opening = format!("{HOOK_ERROR_OPENING}{tool}{GUARD_HOOK_LABEL}{GUARD_DENIAL_OPENING}");
        text.starts_with(&opening)
    } else {
        false
    }
}

/// What a Read's result holds. The Read tool numbers its lines as `cat -n` does, a
/// right-aligned number and a tab before each line, from 1.
fn read_result_of(text: &str) -> ReadResult {
    let mut lines = text.lines();
    let first = lines.next().and_then(line_number_of);
    if first == Some("1") {
        let mut numbered = 1;
        for line in lines {
            if line_number_of(line).is_some() {
                numbered = one_more(numbered);
            }
        }
        ReadResult::Numbered {
            lines: numbered,
            characters: text.chars().count(),
        }
    } else {
        ReadResult::Unnumbered
    }
}

/// The number a numbered line opens with, or none for a line the Read tool did not number.
fn line_number_of(line: &str) -> Option<&str> {
    let (number, _) = line.trim_start_matches(' ').split_once('\t')?;
    if number.is_empty() {
        None
    } else if number.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(number)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
