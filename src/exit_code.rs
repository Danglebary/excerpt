//! Every exit code excerpt ends with, each with one owner.

pub const SUCCESS: u8 = 0;
/// The request was understood and cannot be answered as asked: no item answers the name,
/// several do, one is too long to print, or a named file is in a language excerpt does
/// not read.
pub const REFUSED: u8 = 1;
/// The arguments break the grammar.
pub const USAGE: u8 = 2;
/// Reading a path failed, so what was printed may be incomplete.
pub const IO: u8 = 3;

// The two codes `excerpt guard` speaks as a Claude Code command hook: 2 blocks the tool
// call and hands stderr to the model, and any other non-zero code is a non-blocking error
// the user sees.
// https://code.claude.com/docs/en/hooks — checked 2026-10-15
pub const HOOK_BLOCK: u8 = 2;
pub const HOOK_ERROR: u8 = 1;
