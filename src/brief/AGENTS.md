# The Claude Code `SubagentStart` hook contract

`brief` integrates with Claude Code's command hooks: `excerpt brief` is the `SubagentStart` hook the plugin declares in `claude-code/excerpt/hooks/hooks.json`, with no matcher, so it runs for every agent type.

## The API

- Documentation: <https://code.claude.com/docs/en/hooks> (the `SubagentStart` event, its input, `hookSpecificOutput.additionalContext`, the 10,000-character cap on that string, and exit codes) — checked 2026-10-30.
- Input: one JSON object on stdin naming `agent_id` and `agent_type`. `excerpt brief` leaves it unread, since every subagent gets the same context.
- Output: on exit 0, stdout holding nothing but `{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"..."}}`, whose context Claude Code adds to the subagent's conversation before its first prompt. Plain text on stdout is not added for this event. A context past `ADDITIONAL_CONTEXT_CHARACTERS_MAX` is saved to a file and replaced by its path and a preview, so `hook_output` refuses one. The hook cannot block a subagent's start: a non-zero exit is a non-blocking error, exit 2's notice shows in the subagent's own transcript, and the subagent proceeds without the context.
- Auth and configuration: none. The hook needs the `excerpt` binary on `PATH`.

## Using it from the rest of the code

`hook_output` is pure and takes the context; `report::subagent_brief` renders that context; `main.rs` prints the two composed and exits 0.
