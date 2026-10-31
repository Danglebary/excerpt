//! The functional core of excerpt: the languages it reads, the items it finds in a file's
//! text, the name a reader asks for and which items answer it, the argument grammar, every
//! message, and the pool that spreads work over threads in order.
//! Nothing here reads a file, walks a directory, or reads the environment, so each rule is
//! tested against strings; `main.rs` is the shell that does all of that.

pub mod brief;
pub mod cli;
pub mod exit_code;
pub mod guard;
pub mod item;
pub mod language;
pub mod parallel;
pub mod query;
pub mod report;
pub mod select;
