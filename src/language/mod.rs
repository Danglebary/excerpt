//! The languages excerpt reads, each named by a file's extension.

use std::path::Path;

/// A language whose grammar is compiled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
}

/// Every extension excerpt reads, for the message naming them.
pub const EXTENSIONS: [&str; 5] = ["rs", "ts", "mts", "cts", "tsx"];

impl Language {
    /// The language a path's extension names, or none for a file excerpt does not read.
    #[must_use]
    pub fn of_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        match extension {
            "rs" => Some(Self::Rust),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    /// The tree-sitter grammar that parses the language.
    #[must_use]
    pub fn grammar(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    /// What joins an item's name to the names of the items it sits in, as the language
    /// writes a path to it.
    #[must_use]
    pub const fn separator(self) -> &'static str {
        match self {
            Self::Rust => "::",
            Self::TypeScript | Self::Tsx => ".",
        }
    }
}

#[cfg(test)]
mod tests;
