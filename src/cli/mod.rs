//! The command line's grammar: the arguments in, the command to run or a usage fault out.

/// The path read when a command names none: the working directory.
const DEFAULT_PATH: &str = ".";

/// What one invocation does.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Outline {
        paths: Vec<String>,
    },
    Show {
        name: String,
        all: bool,
        paths: Vec<String>,
    },
    Help,
    Version,
    Guard,
    Brief,
}

/// Every way the arguments break the grammar.
#[derive(Debug, PartialEq, Eq)]
pub enum UsageFault {
    MissingCommand,
    UnknownCommand(String),
    UnknownFlag(String),
    MissingName,
    GuardArgument(String),
    BriefArgument(String),
}

/// Parses the arguments after the program name.
///
/// # Errors
///
/// Returns the first argument the grammar refuses.
pub fn parse_arguments(arguments: &[String]) -> Result<Command, UsageFault> {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] => Err(UsageFault::MissingCommand),
        ["--help" | "-h"] => Ok(Command::Help),
        ["--version" | "-V"] => Ok(Command::Version),
        ["outline", rest @ ..] => parse_outline(rest),
        ["show", rest @ ..] => parse_show(rest),
        ["guard"] => Ok(Command::Guard),
        ["guard", first, ..] => Err(UsageFault::GuardArgument((*first).to_owned())),
        ["brief"] => Ok(Command::Brief),
        ["brief", first, ..] => Err(UsageFault::BriefArgument((*first).to_owned())),
        [first, ..] => Err(UsageFault::UnknownCommand((*first).to_owned())),
    }
}

fn parse_outline(words: &[&str]) -> Result<Command, UsageFault> {
    let mut paths = Vec::new();
    for word in words {
        if is_flag(word) {
            return Err(UsageFault::UnknownFlag((*word).to_owned()));
        }
        paths.push((*word).to_owned());
    }
    Ok(Command::Outline {
        paths: or_working_directory(paths),
    })
}

/// `--all` may stand anywhere, since agents append a flag as often as they lead with it;
/// the first other word is the name and the rest are paths.
fn parse_show(words: &[&str]) -> Result<Command, UsageFault> {
    let mut all = false;
    let mut name = None;
    let mut paths = Vec::new();
    for word in words {
        if *word == "--all" {
            all = true;
            continue;
        }
        if is_flag(word) {
            return Err(UsageFault::UnknownFlag((*word).to_owned()));
        }
        if name.is_none() {
            name = Some((*word).to_owned());
        } else {
            paths.push((*word).to_owned());
        }
    }
    let Some(name) = name else {
        return Err(UsageFault::MissingName);
    };
    Ok(Command::Show {
        name,
        all,
        paths: or_working_directory(paths),
    })
}

fn is_flag(word: &str) -> bool {
    word.starts_with('-') && word != "-"
}

fn or_working_directory(paths: Vec<String>) -> Vec<String> {
    if paths.is_empty() {
        return vec![DEFAULT_PATH.to_owned()];
    }
    paths
}

#[cfg(test)]
mod tests;
