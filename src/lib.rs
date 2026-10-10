//! The functional core of excerpt: the languages it reads, the items it finds in a file's
//! text, the name a reader asks for and which items answer it, the argument grammar, and
//! every message. Nothing here reads a file, walks a directory, or reads the environment,
//! so each rule is tested against strings; `main.rs` is the shell that does all of that.

pub mod cli;
pub mod exit_code;
pub mod item;
pub mod language;
pub mod query;
pub mod report;
pub mod select;
