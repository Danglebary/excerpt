# The Claude Code hook contract

`guard` integrates with Claude Code's command hooks: `excerpt guard` is the `PreToolUse` hook the plugin declares in `claude-code/excerpt/hooks/hooks.json`, matched on the `Bash` tool.

## The API

- Documentation: <https://code.claude.com/docs/en/hooks> (hook input, exit codes, and how a plugin declares hooks in `hooks/hooks.json`) — checked 2026-10-15.
- Input: one JSON object on stdin. `tool_name` names the tool; for `Bash`, `tool_input.command` holds the command. `hook_input` reads those two fields and nothing else.
- Output: exit 0 lets the call proceed; exit 2 blocks it and hands stderr to the model as the reason; any other non-zero exit is a non-blocking error the user sees, and the call proceeds. `exit_code::HOOK_BLOCK` and `exit_code::HOOK_ERROR` hold the two codes excerpt uses.
- Auth and configuration: none. The hook needs the `excerpt` binary on `PATH`; without it the shell's exit 127 surfaces as a non-blocking error on every Bash call.

## Using it from the rest of the code

`verdict` is pure and takes the command string; `hook_input` is pure and takes the raw stdin bytes; `main.rs` reads stdin under `HOOK_INPUT_BYTES_MAX`, calls both, and prints `report::guard_denial` on a block. A malformed input is a non-blocking error rather than a block, so a change in Claude Code's input shape never stops every Bash call.

The verdict is deliberately narrow: `sed -n` printing by line number, `head`, `tail` other than following, and `cat`, reading a file whose extension `Language::of_path` names, with standard output not redirected to a file. It reads the command with tree-sitter-bash rather than a lexer of its own, so quoting, heredocs, and substitutions are the shell's. Widen the readers it judges only against real commands, as `AGENTS.md` at the root says.
