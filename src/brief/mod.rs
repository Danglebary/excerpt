//! The output of the plugin's `SubagentStart` hook, which puts the rule on reading code
//! into each subagent's context before its first prompt. Pure: the context in, the JSON
//! Claude Code reads out.

/// The most characters of a hook's `additionalContext` Claude Code adds as written.
///
/// A longer one is saved to a file, and the subagent sees its path and a preview.
/// <https://code.claude.com/docs/en/hooks> — checked 2026-10-30
pub const ADDITIONAL_CONTEXT_CHARACTERS_MAX: usize = 10_000;

/// The event the output answers, which Claude Code matches against the hook that ran.
const SUBAGENT_START_EVENT: &str = "SubagentStart";

/// The hook's stdout, carrying `context` for the subagent's conversation to start with.
///
/// The output is `{"hookSpecificOutput":{"hookEventName":"SubagentStart",
/// "additionalContext":"..."}}` on one line, which Claude Code reads on exit 0.
/// <https://code.claude.com/docs/en/hooks> — checked 2026-10-30
///
/// # Panics
///
/// When `context` is empty, or longer than `ADDITIONAL_CONTEXT_CHARACTERS_MAX`, where the
/// subagent would see a preview cut short.
#[must_use]
pub fn hook_output(context: &str) -> String {
    assert!(
        !context.is_empty(),
        "a SubagentStart hook adds some context"
    );
    let characters = context.chars().count();
    assert!(
        characters <= ADDITIONAL_CONTEXT_CHARACTERS_MAX,
        "the context runs to {characters} characters, past what Claude Code adds whole"
    );
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": SUBAGENT_START_EVENT,
            "additionalContext": context,
        }
    });
    let line = output.to_string();
    assert!(!line.contains('\n'), "the output is one line of JSON");
    format!("{line}\n")
}

#[cfg(test)]
mod tests;
