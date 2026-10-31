//! The hook's verdicts on a Bash command and on a Read.
//!
//! On a Bash command, the verdict is whether it prints lines of a file excerpt reads
//! through a reader agents reach for in place of `outline` and `show`; on a Read, whether
//! a subagent takes a long code file whole.
//!
//! Pure and deliberately narrow. The command is parsed with bash's own grammar, and every
//! simple command in it is judged wherever it stands: in a list, a pipeline, a loop, a
//! condition, or a command substitution, an unquoted heredoc's included. Quoted text, a
//! comment, and a quoted heredoc's body are text to that grammar, so a message or a script
//! that only mentions a reader passes. A command is denied when it is `sed -n` printing a
//! line range, bounded by line numbers or by a pattern at either end, `awk` printing lines
//! by their numbers, `head`, `tail` other than following a file, or `cat`; it reads a file
//! in a language excerpt reads, named as an argument or redirected into it; and its
//! standard output is not redirected to a file. Standard input, a print of the lines one
//! pattern matches, a file named through a variable, and every other file pass.
//!
//! A Read is denied when a subagent makes it, it names no `offset` or `limit`, its file is
//! in a language excerpt reads, and the file runs past `SUBAGENT_READ_LINES_MAX` lines.
//! The shell counts the lines; every other Read passes without its file being opened.

use std::path::Path;

use tree_sitter::{Node, Parser, TreeCursor};

use crate::language::Language;

/// The most bytes of hook input the shell reads. A Bash command carrying a heredoc that
/// creates a large file fits under it; an input past it is a non-blocking error.
pub const HOOK_INPUT_BYTES_MAX: u64 = 4_194_304;

/// The most lines of a code file a subagent's Read takes whole, naming no `offset` or
/// `limit`; a longer file is read by item.
pub const SUBAGENT_READ_LINES_MAX: usize = 300;

/// Whether the hook lets a command run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(Denial),
}

/// A command the hook blocks: the reader it ran and the source file it read, spelled as
/// the command wrote it, so the refusal's examples name that file the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    pub reader: Reader,
    pub path: String,
}

/// The programs the verdict judges as readers of a file's lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    SedRange,
    AwkRange,
    WholeFileRead,
    Head,
    Tail,
    Cat,
}

/// What a `PreToolUse` hook input carries for the verdict.
#[derive(Debug, PartialEq, Eq)]
pub enum HookInput {
    Bash(String),
    Read(ReadCall),
    OtherTool,
}

/// A Read call as the hook judges it: the file it names, how much of it, and who makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadCall {
    pub path: String,
    pub span: ReadSpan,
    pub caller: Caller,
}

/// How much of a file a Read takes: all of it, or the lines an `offset` or a `limit` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadSpan {
    Whole,
    Ranged,
}

/// Which agent loop makes a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    MainThread,
    Subagent,
}

/// Whether a Read is let through as it stands, or waits on its file's line count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadScreen {
    Allowed,
    LineCountDecides,
}

/// Every way a hook input fails to be what Claude Code sends.
#[derive(Debug, PartialEq, Eq)]
pub enum HookInputFault {
    NotJson(String),
    NoToolName,
    NoCommand,
    NoFilePath,
}

/// Words that run the command after them, so the program is read past them.
const COMMAND_PREFIXES: [&str; 8] = [
    "builtin", "command", "env", "exec", "nice", "nohup", "sudo", "time",
];

/// The operators that send standard output to a file, when no descriptor or descriptor 1
/// stands before them.
const OUTPUT_OPERATORS: [&str; 5] = [">", ">>", ">|", "&>", "&>>"];

/// The descriptor of standard output, written before a redirect.
const STANDARD_OUTPUT: &str = "1";

/// Reads the tool and what the verdict judges of it from a `PreToolUse` hook's stdin.
///
/// The input's shape is `{"tool_name": "Bash", "tool_input": {"command": "..."}, ...}`
/// for a Bash call and `{"tool_name": "Read", "tool_input": {"file_path": "...",
/// "offset": 10, "limit": 50}, ...}` for a Read, `offset` and `limit` optional. Inside a
/// subagent the input carries `agent_id`; the main thread's carries none, and carries
/// `agent_type` under `--agent`.
/// <https://code.claude.com/docs/en/hooks> — checked 2026-10-30
///
/// # Errors
///
/// Returns the fault when the input is not JSON or lacks the fields a Bash or Read call
/// carries.
pub fn hook_input(input: &[u8]) -> Result<HookInput, HookInputFault> {
    let value: serde_json::Value = serde_json::from_slice(input)
        .map_err(|error| HookInputFault::NotJson(error.to_string()))?;
    let Some(tool) = value.get("tool_name").and_then(serde_json::Value::as_str) else {
        return Err(HookInputFault::NoToolName);
    };
    match tool {
        "Bash" => bash_command(&value).map(HookInput::Bash),
        "Read" => read_call(&value).map(HookInput::Read),
        _ => Ok(HookInput::OtherTool),
    }
}

/// The command a Bash call's input carries.
fn bash_command(value: &serde_json::Value) -> Result<String, HookInputFault> {
    let command = value
        .get("tool_input")
        .and_then(|tool_input| tool_input.get("command"));
    match command.and_then(serde_json::Value::as_str) {
        Some(command) => Ok(command.to_owned()),
        None => Err(HookInputFault::NoCommand),
    }
}

/// The file a Read call's input names, how much of it, and whether a subagent makes it.
/// A null `offset` or `limit` names none.
fn read_call(value: &serde_json::Value) -> Result<ReadCall, HookInputFault> {
    let tool_input = value.get("tool_input");
    let path = tool_input
        .and_then(|tool_input| tool_input.get("file_path"))
        .and_then(serde_json::Value::as_str);
    let Some(path) = path else {
        return Err(HookInputFault::NoFilePath);
    };
    let names = |field: &str| {
        tool_input
            .and_then(|tool_input| tool_input.get(field))
            .is_some_and(|named| !named.is_null())
    };
    let span = if names("offset") || names("limit") {
        ReadSpan::Ranged
    } else {
        ReadSpan::Whole
    };
    let caller = match value.get("agent_id").and_then(serde_json::Value::as_str) {
        Some(_) => Caller::Subagent,
        None => Caller::MainThread,
    };
    Ok(ReadCall {
        path: path.to_owned(),
        span,
        caller,
    })
}

/// Judges one Bash command as the hook receives it: the first reader in it that prints a
/// source file's lines, in the order the command is written.
///
/// # Panics
///
/// When the compiled-in shell grammar is one this tree-sitter version cannot load, which
/// the crate's tests would have caught.
#[must_use]
pub fn verdict(command: &str) -> Verdict {
    let mut parser = Parser::new();
    let grammar: tree_sitter::Language = tree_sitter_bash::LANGUAGE.into();
    let loaded = parser.set_language(&grammar);
    assert!(
        loaded.is_ok(),
        "the compiled-in shell grammar is one this tree-sitter version loads"
    );
    let Some(tree) = parser.parse(command, None) else {
        unreachable!("a parser holding a language and no cancellation returns a tree");
    };
    let root = tree.root_node();
    let node_count = root.descendant_count();
    let mut cursor = tree.walk();
    for _ in 0..=node_count {
        let node = cursor.node();
        if node.kind() == "command" {
            if let Some(denial) = judge(node, command, node_count) {
                return Verdict::Deny(denial);
            }
        }
        if !advance(&mut cursor, node_count) {
            return Verdict::Allow;
        }
    }
    unreachable!("the walk visits each of the tree's nodes once and then leaves the root");
}

/// Moves the cursor to the next node in preorder; false once every node has been visited.
fn advance(cursor: &mut TreeCursor<'_>, node_count: usize) -> bool {
    if cursor.goto_first_child() {
        return true;
    }
    for _ in 0..=node_count {
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
        }
    }
    unreachable!("a node sits fewer levels deep than the tree has nodes");
}

/// One word of a simple command: the value it stands for where the command spells it out,
/// none where an expansion or a substitution decides it at run time, and its text as
/// written.
struct Word<'t> {
    value: Option<String>,
    text: &'t str,
}

impl<'t> Word<'t> {
    /// The word the nodes spell together. The shell grammar can split one word into nodes
    /// that touch, as it splits `$b/$k/mod.rs`; to the shell they are one word, and its
    /// value is known only where every node's is.
    fn of(nodes: &[Node<'_>], source: &'t str) -> Self {
        let (Some(first), Some(last)) = (nodes.first(), nodes.last()) else {
            unreachable!("a word is spelled by at least one node");
        };
        let Some(text) = source.get(first.start_byte()..last.end_byte()) else {
            unreachable!("nodes of a parsed string span whole characters of it");
        };
        let mut joined = String::new();
        for node in nodes {
            let Some(part) = value(*node, source) else {
                return Self { value: None, text };
            };
            joined.push_str(&part);
        }
        Self {
            value: Some(joined),
            text,
        }
    }

    fn is_source(&self) -> bool {
        self.value
            .as_deref()
            .is_some_and(|value| Language::of_path(Path::new(value)).is_some())
    }
}

/// A file redirect read apart: its operator, its descriptor where one is written, and the
/// nodes that spell its destination.
struct FileRedirect<'n, 't> {
    operator: &'n str,
    descriptor: Option<&'t str>,
    destination: Vec<Node<'n>>,
}

/// Judges one simple command: a denial when it is a reader printing a source file.
fn judge(command: Node<'_>, source: &str, node_count: usize) -> Option<Denial> {
    assert_eq!(command.kind(), "command", "only a simple command is judged");
    let words = words(command, source)?;
    let (program, arguments) = program(&words)?;
    if sends_output_to_file(command, source, node_count) {
        return None;
    }
    let (reader, read) = match program {
        "sed" | "gsed" => (Reader::SedRange, sed_line_range_read(arguments)?),
        "awk" | "gawk" | "mawk" | "nawk" => (Reader::AwkRange, awk_line_range_read(arguments)?),
        "head" => (Reader::Head, operands(arguments)),
        "tail" if follows(arguments) => return None,
        "tail" => (Reader::Tail, operands(arguments)),
        "cat" => (Reader::Cat, operands(arguments)),
        _ => return None,
    };
    let inputs = inputs(command, source);
    let path = read
        .into_iter()
        .chain(inputs.iter())
        .find(|word| word.is_source())?;
    Some(Denial {
        reader,
        path: path.text.to_owned(),
    })
}

/// The words of a simple command, its name first, each argument node joined to the ones
/// it touches; none when it has no name.
fn words<'t>(command: Node<'_>, source: &'t str) -> Option<Vec<Word<'t>>> {
    let name = command.child_by_field_name("name")?;
    let mut groups = vec![vec![name]];
    let mut cursor = command.walk();
    for argument in command.children_by_field_name("argument", &mut cursor) {
        let touches = groups
            .last()
            .and_then(|group| group.last())
            .is_some_and(|previous| previous.end_byte() == argument.start_byte());
        match groups.last_mut() {
            Some(group) if touches => group.push(argument),
            _ => groups.push(vec![argument]),
        }
    }
    let words = groups.iter().map(|group| Word::of(group, source)).collect();
    Some(words)
}

/// The program a simple command runs, by its base name, and the words after it; none when
/// the name is decided at run time.
fn program<'w, 't>(words: &'w [Word<'t>]) -> Option<(&'w str, &'w [Word<'t>])> {
    let mut rest = words;
    for _ in 0..=words.len() {
        let (first, tail) = rest.split_first()?;
        let name = first.value.as_deref()?;
        let base = name.rsplit('/').next().unwrap_or(name);
        if COMMAND_PREFIXES.contains(&base) {
            rest = skip_prefix_words(tail);
        } else if base == "timeout" {
            let options_skipped = skip_prefix_words(tail);
            rest = options_skipped
                .split_first()
                .map_or(options_skipped, |(_, after_duration)| after_duration);
        } else if is_assignment(name) {
            rest = tail;
        } else {
            return Some((base, tail));
        }
    }
    unreachable!("every pass consumes at least one word, so the words run out first");
}

/// The words after a prefix up to the program it runs: its options and, for `env`, its
/// assignments.
fn skip_prefix_words<'w, 't>(words: &'w [Word<'t>]) -> &'w [Word<'t>] {
    let position = words
        .iter()
        .position(|word| {
            word.value
                .as_deref()
                .is_none_or(|value| !value.starts_with('-') && !is_assignment(value))
        })
        .unwrap_or(words.len());
    words.get(position..).unwrap_or_default()
}

fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut characters = name.chars();
    let starts = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    starts && characters.all(|next| next.is_ascii_alphanumeric() || next == '_')
}

/// The files redirected into a simple command's standard input, by its own redirects or
/// by those of the statement it is the body of.
fn inputs<'t>(command: Node<'_>, source: &'t str) -> Vec<Word<'t>> {
    let mut redirects = file_redirects(command, source);
    if let Some(parent) = command.parent() {
        if parent.kind() == "redirected_statement" {
            if parent.child_by_field_name("body") == Some(command) {
                redirects.extend(file_redirects(parent, source));
            }
        }
    }
    let mut found = Vec::new();
    for redirect in redirects {
        if redirect.operator == "<" {
            if redirect.descriptor.is_none() {
                found.push(Word::of(&redirect.destination, source));
            }
        }
    }
    found
}

/// Whether a simple command's standard output goes to a file: through a redirect of its
/// own, or of a statement it stands in, below the command substitution that would take
/// its output.
///
/// The grammar hangs a redirect written after `a && b` on the whole list, where bash
/// binds it to `b` alone, so a redirect above a list reaches only the list's last
/// command. A pipeline passes it through, since each command's output feeds the next
/// and the last one's goes where the redirect sends it, and a group, a loop, or a
/// condition takes it for everything inside.
fn sends_output_to_file(command: Node<'_>, source: &str, node_count: usize) -> bool {
    if redirects_output_to_file(command, source) {
        return true;
    }
    let mut node = command;
    let mut is_reached = true;
    for _ in 0..=node_count {
        let Some(parent) = node.parent() else {
            return false;
        };
        match parent.kind() {
            "command_substitution" | "process_substitution" => return false,
            "list" => is_reached = is_reached && is_last_named_child(parent, node),
            "pipeline" => {}
            "redirected_statement" => {
                let is_body = parent.child_by_field_name("body") == Some(node);
                if is_body {
                    if is_reached {
                        if redirects_output_to_file(parent, source) {
                            return true;
                        }
                    }
                }
            }
            _ => is_reached = true,
        }
        node = parent;
    }
    unreachable!("a node has fewer ancestors than the tree has nodes");
}

fn is_last_named_child(parent: Node<'_>, node: Node<'_>) -> bool {
    let mut cursor = parent.walk();
    parent.named_children(&mut cursor).last() == Some(node)
}

/// Whether a command's or a redirected statement's own redirects send standard output to
/// a file.
fn redirects_output_to_file(node: Node<'_>, source: &str) -> bool {
    file_redirects(node, source).iter().any(|redirect| {
        OUTPUT_OPERATORS.contains(&redirect.operator)
            && redirect
                .descriptor
                .is_none_or(|descriptor| descriptor == STANDARD_OUTPUT)
    })
}

/// The file redirects a command or a redirected statement carries, a heredoc redirect's
/// own included, each read apart.
fn file_redirects<'n, 't>(node: Node<'n>, source: &'t str) -> Vec<FileRedirect<'n, 't>> {
    let mut found = Vec::new();
    let mut cursor = node.walk();
    for redirect in node.children_by_field_name("redirect", &mut cursor) {
        if let Some(read) = file_redirect(redirect, source) {
            found.push(read);
        }
        let mut nested_cursor = redirect.walk();
        for nested in redirect.children_by_field_name("redirect", &mut nested_cursor) {
            if let Some(read) = file_redirect(nested, source) {
                found.push(read);
            }
        }
    }
    found
}

/// One redirect read apart, or none when it is no file redirect or the grammar recovered
/// from an error inside it.
fn file_redirect<'n, 't>(redirect: Node<'n>, source: &'t str) -> Option<FileRedirect<'n, 't>> {
    if redirect.kind() != "file_redirect" {
        return None;
    }
    let mut cursor = redirect.walk();
    let operator = redirect
        .children(&mut cursor)
        .find(|child| !child.is_named())?
        .kind();
    let descriptor = redirect
        .child_by_field_name("descriptor")
        .map(|node| node_text(node, source));
    let mut destination_cursor = redirect.walk();
    let destination: Vec<Node<'n>> = redirect
        .children_by_field_name("destination", &mut destination_cursor)
        .collect();
    if destination.is_empty() {
        return None;
    }
    Some(FileRedirect {
        operator,
        descriptor,
        destination,
    })
}

/// The words a `head`, `tail`, or `cat` reads as files: every word that is not an option.
fn operands<'w, 't>(arguments: &'w [Word<'t>]) -> Vec<&'w Word<'t>> {
    let mut found = Vec::new();
    let mut options_ended = false;
    for word in arguments {
        let Some(value) = word.value.as_deref() else {
            continue;
        };
        if !options_ended && value == "--" {
            options_ended = true;
        } else if options_ended || !is_option(value) {
            found.push(word);
        } else {
            assert!(
                is_option(value),
                "a word before `--` is an option or an operand"
            );
        }
    }
    found
}

/// A word led by `-` other than `-` alone, which names standard input.
fn is_option(value: &str) -> bool {
    value.starts_with('-') && value != "-"
}

/// Whether a `tail` follows its file as it grows rather than printing its last lines.
fn follows(arguments: &[Word<'_>]) -> bool {
    arguments.iter().any(|word| {
        let Some(value) = word.value.as_deref() else {
            return false;
        };
        if let Some(long) = value.strip_prefix("--") {
            long == "follow" || long.starts_with("follow=")
        } else if let Some(cluster) = value.strip_prefix('-') {
            cluster.chars().any(|flag| flag == 'f' || flag == 'F')
        } else {
            false
        }
    })
}

/// What a `sed` command's flags and script say about it: whether it is quiet, the scripts
/// it runs when every one is spelled out, and the words it reads as files.
struct SedCall<'w, 't> {
    is_quiet: bool,
    scripts: Option<Vec<&'w str>>,
    files: Vec<&'w Word<'t>>,
}

/// The files a `sed` command prints line ranges from; none when it is not quiet, edits in
/// place, or runs any script that is not line-range prints.
fn sed_line_range_read<'w, 't>(arguments: &'w [Word<'t>]) -> Option<Vec<&'w Word<'t>>> {
    let call = sed_call(arguments)?;
    if !call.is_quiet {
        return None;
    }
    let scripts = call.scripts?;
    if scripts.is_empty() {
        return None;
    }
    if scripts.iter().all(|script| prints_line_ranges(script)) {
        Some(call.files)
    } else {
        None
    }
}

/// Reads a `sed` command's words; none when it edits in place, since that is no read.
fn sed_call<'w, 't>(arguments: &'w [Word<'t>]) -> Option<SedCall<'w, 't>> {
    let mut call = SedCall {
        is_quiet: false,
        scripts: Some(Vec::new()),
        files: Vec::new(),
    };
    let mut has_script_flag = false;
    let mut expects_script = false;
    let mut options_ended = false;
    for word in arguments {
        let Some(value) = word.value.as_deref() else {
            if expects_script || awaits_script(&call, has_script_flag) {
                call.scripts = None;
            }
            expects_script = false;
            continue;
        };
        if expects_script {
            push_script(&mut call, value);
            expects_script = false;
        } else if options_ended || !is_option(value) {
            if awaits_script(&call, has_script_flag) {
                push_script(&mut call, value);
            } else {
                call.files.push(word);
            }
        } else if value == "--" {
            options_ended = true;
        } else {
            let flag = sed_flag(value)?;
            call.is_quiet = call.is_quiet || flag.is_quiet;
            has_script_flag = has_script_flag || flag.script != FlagScript::None;
            match flag.script {
                FlagScript::None => {}
                FlagScript::Next => expects_script = true,
                FlagScript::Inline(script) => push_script(&mut call, script),
                FlagScript::Unknown => call.scripts = None,
            }
        }
    }
    Some(call)
}

/// Whether the next word that is no option is the script: no flag named one, and no word
/// has been taken as it.
fn awaits_script(call: &SedCall<'_, '_>, has_script_flag: bool) -> bool {
    !has_script_flag && call.scripts.as_ref().is_some_and(Vec::is_empty)
}

fn push_script<'w>(call: &mut SedCall<'w, '_>, script: &'w str) {
    if let Some(scripts) = call.scripts.as_mut() {
        scripts.push(script);
    }
}

/// Where a `sed` flag puts a script: nowhere, in the next word, inside the flag itself,
/// or in a file the verdict does not read.
#[derive(Debug, PartialEq, Eq)]
enum FlagScript<'w> {
    None,
    Next,
    Inline(&'w str),
    Unknown,
}

/// What one `sed` flag word says: whether it quiets `sed` and where it puts a script.
struct SedFlag<'w> {
    is_quiet: bool,
    script: FlagScript<'w>,
}

/// Reads one `sed` flag word, long or a cluster of short flags; none when it edits in
/// place.
fn sed_flag(value: &str) -> Option<SedFlag<'_>> {
    if let Some(long) = value.strip_prefix("--") {
        return long_sed_flag(long);
    }
    let Some(cluster) = value.strip_prefix('-') else {
        unreachable!("a sed flag is led by `-`");
    };
    let mut flag = SedFlag {
        is_quiet: false,
        script: FlagScript::None,
    };
    for (index, character) in cluster.char_indices() {
        match character {
            'n' => flag.is_quiet = true,
            'i' => return None,
            'e' | 'f' => {
                let Some(after) = index.checked_add(character.len_utf8()) else {
                    unreachable!("an index inside a word plus one character fits a usize");
                };
                let rest = cluster.get(after..).unwrap_or_default();
                flag.script = match (character, rest.is_empty()) {
                    ('e', true) => FlagScript::Next,
                    ('e', false) => FlagScript::Inline(rest),
                    _ => FlagScript::Unknown,
                };
                return Some(flag);
            }
            _ => {}
        }
    }
    Some(flag)
}

/// Reads one long `sed` flag, without its `--`; none when it edits in place.
fn long_sed_flag(long: &str) -> Option<SedFlag<'_>> {
    let (name, attached) = match long.split_once('=') {
        Some((name, attached)) => (name, Some(attached)),
        None => (long, None),
    };
    let script = match (name, attached) {
        ("in-place", _) => return None,
        ("expression", Some(script)) => FlagScript::Inline(script),
        ("expression", None) => FlagScript::Next,
        ("file", _) => FlagScript::Unknown,
        _ => FlagScript::None,
    };
    Some(SedFlag {
        is_quiet: matches!(name, "quiet" | "silent"),
        script,
    })
}

/// Whether a `sed` script is nothing but prints of line ranges, `10,40p` or `12p` or
/// `/fn plan/,/^}/p` or `1,40p;40q`, with at least one print among them.
fn prints_line_ranges(script: &str) -> bool {
    let mut has_print = false;
    for command in script.split([';', '\n']).map(str::trim) {
        if command.is_empty() {
            continue;
        }
        if let Some(address) = command.strip_suffix('p') {
            if !is_line_range(address.trim_end()) {
                return false;
            }
            has_print = true;
        } else if let Some(address) = command.strip_suffix('q') {
            if !is_line_number(address.trim_end()) {
                return false;
            }
        } else {
            return false;
        }
    }
    has_print
}

/// One address of a `sed` range: a line number or `$`, or a pattern between slashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SedAddress {
    Line,
    Pattern,
}

/// `12`, `10,40`, `100,$`, `5,+10`, or a range a pattern bounds at either end,
/// `/fn plan/,/^}/` or `/let x/,+2` or `10,/^}/`. A lone pattern selects the lines that
/// match it, as `grep` does, and is no range.
fn is_line_range(address: &str) -> bool {
    let Some((first, rest)) = sed_address(address) else {
        return false;
    };
    let Some(last) = rest.strip_prefix(',') else {
        if rest.is_empty() {
            return first == SedAddress::Line;
        }
        return false;
    };
    if last.strip_prefix('+').is_some_and(is_digits) {
        return true;
    }
    sed_address(last).is_some_and(|(_, after)| after.is_empty())
}

/// The address `text` opens with and the text after it; none when it opens with no
/// address or a pattern left open.
fn sed_address(text: &str) -> Option<(SedAddress, &str)> {
    if let Some(pattern) = text.strip_prefix('/') {
        let end = pattern_end(pattern)?;
        let rest = pattern.get(end..)?;
        assert!(
            rest.len() < pattern.len(),
            "a pattern's closing slash is consumed with it"
        );
        return Some((SedAddress::Pattern, rest));
    }
    if let Some(rest) = text.strip_prefix('$') {
        return Some((SedAddress::Line, rest));
    }
    let digits_end = text
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(text.len());
    let (number, rest) = text.split_at_checked(digits_end)?;
    if !is_digits(number) {
        return None;
    }
    assert!(
        rest.len() < text.len(),
        "a line number is at least one digit"
    );
    Some((SedAddress::Line, rest))
}

/// The byte just past the slash that closes a pattern, read from after its opening slash;
/// none when no unescaped slash closes it.
fn pattern_end(pattern: &str) -> Option<usize> {
    let mut is_escaped = false;
    for (index, character) in pattern.char_indices() {
        if is_escaped {
            is_escaped = false;
        } else if character == '\\' {
            is_escaped = true;
        } else if character == '/' {
            let Some(end) = index.checked_add(1) else {
                unreachable!("an index inside a word plus one byte fits a usize");
            };
            return Some(end);
        }
    }
    None
}

/// A line number or `$`, the last line.
fn is_line_number(address: &str) -> bool {
    address == "$" || is_digits(address)
}

/// The comparisons an `awk` condition may hold, each two-character operator before the
/// one-character operator it begins with.
const AWK_COMPARISONS: [&str; 6] = ["==", "!=", "<=", ">=", "<", ">"];

/// The files an `awk` command prints lines of by their numbers, `NR>=10 && NR<=40` or
/// `NR==5 {print; exit}`; none when its program comes from a file, is decided at run time,
/// or does anything else.
fn awk_line_range_read<'w, 't>(arguments: &'w [Word<'t>]) -> Option<Vec<&'w Word<'t>>> {
    let mut program = None;
    let mut files = Vec::new();
    let mut expects_value = false;
    let mut options_ended = false;
    for word in arguments {
        if program.is_some() {
            let is_assignment_operand = word.value.as_deref().is_some_and(is_assignment);
            if !is_assignment_operand {
                files.push(word);
            }
            continue;
        }
        let value = word.value.as_deref()?;
        if expects_value {
            expects_value = false;
        } else if options_ended || !is_option(value) {
            program = Some(value);
        } else if value == "--" {
            options_ended = true;
        } else {
            expects_value = awk_flag_takes_next_word(value)?;
        }
    }
    let program = program?;
    if is_line_number_program(program) {
        Some(files)
    } else {
        None
    }
}

/// Whether an `awk` flag takes the next word as its value; none for a flag the verdict
/// does not read, a program file's among them.
fn awk_flag_takes_next_word(flag: &str) -> Option<bool> {
    assert!(is_option(flag), "only a flag is read as one");
    match flag {
        "-F" | "-v" | "--field-separator" | "--assign" => Some(true),
        _ if flag.starts_with("--field-separator=") || flag.starts_with("--assign=") => Some(false),
        _ if flag.starts_with("--") => None,
        _ if flag.starts_with("-F") || flag.starts_with("-v") => Some(false),
        _ => None,
    }
}

/// Whether an `awk` program is one rule selecting lines by number alone and printing each
/// line it selects: `NR>=10 && NR<=40`, `NR==5 {print; exit}`, or `NR<=9 {print NR": "$0}`.
fn is_line_number_program(program: &str) -> bool {
    let (condition, action) = match program.split_once('{') {
        Some((condition, rest)) => {
            let Some(body) = rest.trim_end().strip_suffix('}') else {
                return false;
            };
            (condition, Some(body))
        }
        None => (program, None),
    };
    let condition = condition.trim();
    if condition.is_empty() {
        return false;
    }
    let is_by_line_number = condition
        .split("&&")
        .flat_map(|part| part.split("||"))
        .all(is_line_number_comparison);
    if !is_by_line_number {
        return false;
    }
    action.is_none_or(prints_each_line)
}

/// `NR>=10`, `FNR==5`, or `40>=NR`: a record number against a number.
fn is_line_number_comparison(term: &str) -> bool {
    let Some((left, right)) = awk_comparison(term) else {
        return false;
    };
    match (is_record_number(left), is_record_number(right)) {
        (true, false) => is_digits(right),
        (false, true) => is_digits(left),
        (true, true) | (false, false) => false,
    }
}

/// The two sides of a comparison, trimmed; none when the term holds no comparison.
fn awk_comparison(term: &str) -> Option<(&str, &str)> {
    let position = term.find(['<', '>', '=', '!'])?;
    let (left, from_operator) = term.split_at_checked(position)?;
    let operator = AWK_COMPARISONS
        .iter()
        .find(|operator| from_operator.starts_with(**operator))?;
    let right = from_operator.strip_prefix(*operator)?;
    Some((left.trim(), right.trim()))
}

fn is_record_number(text: &str) -> bool {
    matches!(text, "NR" | "FNR")
}

/// Whether an `awk` action prints each line it runs on and does nothing else but stop:
/// `print`, `print $0`, or `print NR": "$0`, and `exit`.
fn prints_each_line(body: &str) -> bool {
    let mut has_print = false;
    for statement in body.split([';', '\n']).map(str::trim) {
        if statement.is_empty() || statement == "exit" {
            continue;
        }
        let Some(arguments) = statement.strip_prefix("print") else {
            return false;
        };
        if !prints_the_line(arguments) {
            return false;
        }
        has_print = true;
    }
    has_print
}

/// Whether a print's arguments print the line: none, which prints `$0`, or `$0` among
/// record numbers and string literals.
fn prints_the_line(arguments: &str) -> bool {
    if arguments.trim().is_empty() {
        return true;
    }
    let Some(terms) = awk_print_terms(arguments) else {
        return false;
    };
    let is_line_or_number = terms
        .iter()
        .all(|term| term == "$0" || is_record_number(term));
    if !is_line_or_number {
        return false;
    }
    terms.iter().any(|term| term == "$0")
}

/// The terms of an `awk` print list outside its string literals, split at whitespace, at
/// commas, and at each literal; none when a literal is left open.
fn awk_print_terms(arguments: &str) -> Option<Vec<String>> {
    let mut terms = Vec::new();
    let mut term = String::new();
    let mut is_in_string = false;
    let mut is_escaped = false;
    for character in arguments.chars() {
        if is_in_string {
            if is_escaped {
                is_escaped = false;
            } else if character == '\\' {
                is_escaped = true;
            } else if character == '"' {
                is_in_string = false;
            }
        } else if character == '"' || character == ',' || character.is_whitespace() {
            is_in_string = character == '"';
            if !term.is_empty() {
                terms.push(std::mem::take(&mut term));
            }
        } else {
            term.push(character);
        }
    }
    if is_in_string {
        return None;
    }
    if !term.is_empty() {
        terms.push(term);
    }
    assert!(
        terms.iter().all(|term| !term.contains('"')),
        "a term holds no part of a string literal"
    );
    Some(terms)
}

/// The characters a shell word holds bare, beside letters and digits: none of them is
/// special to the shell.
const BARE_WORD_PUNCTUATION: &str = "/._-+:@%,=";

/// Whether a Read is let through as it stands or waits on its file's line count: a
/// subagent's Read of a code file naming no `offset` or `limit` waits, and every other
/// Read is let through.
#[must_use]
pub fn read_screen(call: &ReadCall) -> ReadScreen {
    match (call.caller, call.span) {
        (Caller::Subagent, ReadSpan::Whole) => {}
        (Caller::Subagent, ReadSpan::Ranged) | (Caller::MainThread, _) => {
            return ReadScreen::Allowed;
        }
    }
    match Language::of_path(Path::new(&call.path)) {
        Some(_) => ReadScreen::LineCountDecides,
        None => ReadScreen::Allowed,
    }
}

/// Judges a Read its line count decides, its file holding `line_count` lines: denied past
/// `SUBAGENT_READ_LINES_MAX`, naming the file as one shell word so the refusal's commands
/// run as written.
///
/// # Panics
///
/// When the call is one `read_screen` lets through, which no line count decides.
#[must_use]
pub fn read_verdict(call: &ReadCall, line_count: usize) -> Verdict {
    assert_eq!(
        read_screen(call),
        ReadScreen::LineCountDecides,
        "only a Read its line count decides is judged by it"
    );
    if line_count > SUBAGENT_READ_LINES_MAX {
        Verdict::Deny(Denial {
            reader: Reader::WholeFileRead,
            path: shell_word(&call.path),
        })
    } else {
        Verdict::Allow
    }
}

/// `path` as one shell word: bare where every character is plain, else single-quoted, a
/// single quote inside it written as `'\''`.
fn shell_word(path: &str) -> String {
    let is_plain = |character: char| {
        character.is_ascii_alphanumeric() || BARE_WORD_PUNCTUATION.contains(character)
    };
    if path.chars().all(is_plain) {
        if !path.is_empty() {
            return path.to_owned();
        }
    }
    let quoted = format!("'{}'", path.replace('\'', "'\\''"));
    assert!(quoted.len() > path.len(), "quoting adds the quotes");
    quoted
}

/// The lines `bytes` hold: one per newline, and one more for a last line no newline ends.
#[must_use]
pub fn line_count(bytes: &[u8]) -> usize {
    let segments = bytes.split(|byte| *byte == b'\n').count();
    let Some(newlines) = segments.checked_sub(1) else {
        unreachable!("splitting a slice yields at least one segment");
    };
    match bytes.last() {
        Some(last) if *last != b'\n' => {
            let Some(lines) = newlines.checked_add(1) else {
                unreachable!("a count of a buffer's bytes plus one fits a usize");
            };
            lines
        }
        Some(_) | None => newlines,
    }
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| character.is_ascii_digit())
}

/// The value a word stands for where the command spells it out: quotes removed and
/// escapes resolved. None where an expansion or a substitution decides it at run time.
/// A command's name stands inside a `command_name` node, which is read through.
fn value(node: Node<'_>, source: &str) -> Option<String> {
    let word = if node.kind() == "command_name" {
        node.named_child(0)?
    } else {
        node
    };
    match word.kind() {
        "concatenation" => {
            let mut joined = String::new();
            let mut cursor = word.walk();
            for part in word.children(&mut cursor) {
                if !part.is_named() {
                    return None;
                }
                joined.push_str(&part_value(part, source)?);
            }
            Some(joined)
        }
        _ => part_value(word, source),
    }
}

/// The value of one part of a word: a bare word, a number, or a quoted string.
fn part_value(node: Node<'_>, source: &str) -> Option<String> {
    let text = node_text(node, source);
    match node.kind() {
        "word" | "number" => Some(unescaped(text, |_| true)),
        "raw_string" => text
            .strip_prefix('\'')
            .and_then(|inner| inner.strip_suffix('\''))
            .map(str::to_owned),
        "string" => {
            let mut cursor = node.walk();
            let is_literal = node
                .named_children(&mut cursor)
                .all(|child| child.kind() == "string_content");
            if !is_literal {
                return None;
            }
            let inner = text
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))?;
            Some(unescaped(inner, |escaped| {
                matches!(escaped, '"' | '\\' | '$' | '`')
            }))
        }
        _ => None,
    }
}

/// The text with each backslash before a character `escapes` accepts removed, as the
/// shell removes it.
fn unescaped(text: &str, escapes: impl Fn(char) -> bool) -> String {
    let mut value = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\' {
            if let Some(escaped) = characters.next_if(|next| escapes(*next)) {
                value.push(escaped);
                continue;
            }
        }
        value.push(character);
    }
    value
}

fn node_text<'t>(node: Node<'_>, source: &'t str) -> &'t str {
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        unreachable!("a node of a parsed string spans whole characters of it");
    };
    text
}

#[cfg(test)]
mod tests;
