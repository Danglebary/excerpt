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
