//! The items in one file's text.
//!
//! An item is each declaration a reader would ask for by name, with the lines it spans and
//! the items it sits in. Parsing is pure, so a file's outline is a function of its text and
//! its language alone.

use crate::language::Language;
use tree_sitter::{Node, Parser};

mod rust;
mod typescript;

/// How deep items nest before the ones inside are left out of an outline.
pub const NESTING_DEPTH_MAX: usize = 16;

/// The lines an item spans, numbered from 1, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lines {
    first: usize,
    last: usize,
}

impl Lines {
    /// # Panics
    ///
    /// When `first` is 0 or `last` comes before `first`: lines are numbered from 1 and a
    /// span holds at least one.
    #[must_use]
    pub fn new(first: usize, last: usize) -> Self {
        assert!(first >= 1, "lines are numbered from 1");
        assert!(last >= first, "a span ends at or after its first line");
        Self { first, last }
    }

    #[must_use]
    pub const fn first(self) -> usize {
        self.first
    }

    #[must_use]
    pub const fn last(self) -> usize {
        self.last
    }

    /// How many lines the span holds.
    #[must_use]
    pub fn count(self) -> usize {
        let Some(difference) = self.last.checked_sub(self.first) else {
            unreachable!("a span ends at or after its first line");
        };
        let Some(count) = difference.checked_add(1) else {
            unreachable!("a line count fits in a usize");
        };
        count
    }
}

/// What a declaration is, named by the keyword that declares it; a class member, which no
/// keyword declares, is a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Fn,
    Struct,
    Enum,
    Union,
    Trait,
    Impl,
    Type,
    Const,
    Static,
    Mod,
    Macro,
    Function,
    Let,
    Var,
    Class,
    Interface,
    Namespace,
    Method,
}

/// Every kind, in the order an outline's reader meets them in the help.
pub const KINDS: [Kind; 18] = [
    Kind::Fn,
    Kind::Struct,
    Kind::Enum,
    Kind::Union,
    Kind::Trait,
    Kind::Impl,
    Kind::Type,
    Kind::Const,
    Kind::Static,
    Kind::Mod,
    Kind::Macro,
    Kind::Function,
    Kind::Let,
    Kind::Var,
    Kind::Class,
    Kind::Interface,
    Kind::Namespace,
    Kind::Method,
];

impl Kind {
    /// The word an outline prints before an item's name, which a query may lead with.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Fn => "fn",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Union => "union",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Type => "type",
            Self::Const => "const",
            Self::Static => "static",
            Self::Mod => "mod",
            Self::Macro => "macro",
            Self::Function => "function",
            Self::Let => "let",
            Self::Var => "var",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Namespace => "namespace",
            Self::Method => "method",
        }
    }

    /// The kind a keyword names, or none for a word that names no kind.
    #[must_use]
    pub fn of_keyword(word: &str) -> Option<Self> {
        KINDS.into_iter().find(|kind| kind.keyword() == word)
    }
}

/// One declaration: its kind, its name qualified by the items it sits in, and its lines,
/// which run from the first comment or attribute directly above it to its last line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    kind: Kind,
    path: Vec<String>,
    lines: Lines,
}

impl Item {
    /// `path` is the item's name preceded by the names it sits in, outermost first.
    ///
    /// # Panics
    ///
    /// When `path` is empty or holds an empty name.
    #[must_use]
    pub fn new(kind: Kind, path: Vec<String>, lines: Lines) -> Self {
        assert!(!path.is_empty(), "an item has a name");
        assert!(
            path.iter().all(|segment| !segment.is_empty()),
            "no name in an item's path is empty"
        );
        Self { kind, path, lines }
    }

    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    #[must_use]
    pub fn path(&self) -> &[String] {
        &self.path
    }

    #[must_use]
    pub const fn lines(&self) -> Lines {
        self.lines
    }

    /// How many items this one sits inside.
    #[must_use]
    pub fn depth(&self) -> usize {
        let Some(depth) = self.path.len().checked_sub(1) else {
            unreachable!("an item has a name");
        };
        depth
    }

    /// The path written as the language writes it: `Cursor::new`, `Service.run`.
    #[must_use]
    pub fn qualified_name(&self, language: Language) -> String {
        self.path.join(language.separator())
    }
}

/// A file's items in document order, each container before what it holds, and the first
/// line holding a syntax error, past which items may be missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    pub items: Vec<Item>,
    pub first_error_line: Option<usize>,
}

/// Parses `text` as `language` and lists its items.
///
/// # Panics
///
/// When a compiled-in grammar is one this tree-sitter version cannot load, which the
/// crate's tests would have caught.
#[must_use]
pub fn outline(language: Language, text: &str) -> Outline {
    let mut parser = Parser::new();
    let grammar = language.grammar();
    let loaded = parser.set_language(&grammar);
    assert!(
        loaded.is_ok(),
        "a compiled-in grammar is one this tree-sitter version loads"
    );
    let Some(tree) = parser.parse(text, None) else {
        unreachable!("a parser holding a language and no cancellation returns a tree");
    };
    let root = tree.root_node();
    let items = match language {
        Language::Rust => collect(root, text, &rust::GRAMMAR),
        Language::TypeScript | Language::Tsx => collect(root, text, &typescript::GRAMMAR),
    };
    let first_error_line = first_error_line(root);
    Outline {
        items,
        first_error_line,
    }
}

/// The items nested inside the one at `index`: those after it, up to the first that sits
/// no deeper than it does.
///
/// # Panics
///
/// When `index` names no item in `items`.
#[must_use]
pub fn members(items: &[Item], index: usize) -> &[Item] {
    let Some(container) = items.get(index) else {
        unreachable!("members are read for an item in the list");
    };
    let depth = container.depth();
    let Some(after) = index.checked_add(1) else {
        unreachable!("an index in a list is below usize::MAX");
    };
    let Some(rest) = items.get(after..) else {
        unreachable!("the position after an item in a list is at most its length");
    };
    let count = rest.iter().take_while(|item| item.depth() > depth).count();
    let Some(nested) = rest.get(..count) else {
        unreachable!("a count of leading items is at most the list's length");
    };
    nested
}

/// A declaration one language's classifier found at a node.
struct Found<'tree> {
    kind: Kind,
    name: String,
    /// The node whose lines the item spans before the comments above it are added.
    node: Node<'tree>,
    /// The node holding the declarations nested in this one, and the name they are
    /// qualified by, which for a trait impl is its type rather than its own name.
    body: Option<(Node<'tree>, String)>,
}

/// What one language contributes to the walk every language shares.
struct Grammar {
    /// The declarations at a node, several where one statement declares several names.
    classify: for<'tree> fn(Node<'tree>, &str) -> Vec<Found<'tree>>,
    /// The kinds of node that belong to the declaration directly below them.
    leading: &'static [&'static str],
}

/// Every item under `root`, in document order. The walk keeps its own stack, so a deep
/// file cannot exhaust the thread's, and it stops descending at `NESTING_DEPTH_MAX`.
fn collect(root: Node<'_>, text: &str, grammar: &Grammar) -> Vec<Item> {
    let mut items = Vec::new();
    let mut pending: Vec<(Node<'_>, Vec<String>)> = Vec::new();
    push_children(&mut pending, root, &[]);
    while let Some((node, scope)) = pending.pop() {
        for found in (grammar.classify)(node, text) {
            let first = first_line_with_leading(found.node, grammar.leading);
            let lines = Lines::new(first, last_line(found.node));
            let mut path = scope.clone();
            path.push(found.name);
            items.push(Item::new(found.kind, path, lines));
            if let Some((body, qualifier)) = found.body {
                let mut inner = scope.clone();
                inner.push(qualifier);
                if inner.len() < NESTING_DEPTH_MAX {
                    push_children(&mut pending, body, &inner);
                }
            }
        }
    }
    items
}

/// Pushes a node's named children last first, so the stack pops them in document order.
fn push_children<'tree>(
    pending: &mut Vec<(Node<'tree>, Vec<String>)>,
    node: Node<'tree>,
    scope: &[String],
) {
    let mut cursor = node.walk();
    let start = pending.len();
    for child in node.named_children(&mut cursor) {
        pending.push((child, scope.to_vec()));
    }
    let Some(pushed) = pending.get_mut(start..) else {
        unreachable!("the stack's length before the pushes is at most its length after");
    };
    pushed.reverse();
}

/// The first line of a declaration once the comments and attributes directly above it
/// are added: each one counts while no blank line separates it from what follows, it
/// documents its enclosing module (`//!`), or it trails the code on its own first line.
fn first_line_with_leading(node: Node<'_>, leading: &[&str]) -> usize {
    let mut first = start_line(node);
    let mut candidate = node.prev_named_sibling();
    while let Some(sibling) = candidate {
        if !leading.contains(&sibling.kind()) {
            break;
        }
        if is_inner_doc_comment(sibling) {
            break;
        }
        if trails_code(sibling) {
            break;
        }
        let Some(after) = last_line(sibling).checked_add(1) else {
            unreachable!("a line number is below usize::MAX");
        };
        if after < first {
            break;
        }
        first = start_line(sibling);
        candidate = sibling.prev_named_sibling();
    }
    first
}

fn is_inner_doc_comment(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    let mut children = node.named_children(&mut cursor);
    children.any(|child| child.kind() == "inner_doc_comment_marker")
}

/// Whether a node starts on the line where the node before it ends.
fn trails_code(node: Node<'_>) -> bool {
    match node.prev_sibling() {
        Some(previous) => last_line(previous) == start_line(node),
        None => false,
    }
}

fn start_line(node: Node<'_>) -> usize {
    let Some(line) = node.start_position().row.checked_add(1) else {
        unreachable!("a row is below usize::MAX");
    };
    line
}

/// The last line holding any of a node's text. A line comment's span ends at column 0 of
/// the row after it, which holds none of its text.
fn last_line(node: Node<'_>) -> usize {
    let start = node.start_position();
    let end = node.end_position();
    if end.column == 0 {
        if end.row > start.row {
            return end.row;
        }
    }
    let Some(line) = end.row.checked_add(1) else {
        unreachable!("a row is below usize::MAX");
    };
    line
}

/// A node's text; the source is a `str`, so every node's bytes are UTF-8.
fn node_text<'text>(node: Node<'_>, text: &'text str) -> &'text str {
    let Ok(slice) = node.utf8_text(text.as_bytes()) else {
        unreachable!("a node of a parsed str spans whole characters");
    };
    slice
}

/// The text of a node's field, or none where the field is absent.
fn field_text(node: Node<'_>, field: &str, text: &str) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    Some(node_text(child, text).to_owned())
}

/// The line of the first syntax error, searching only the subtrees that hold one.
fn first_error_line(root: Node<'_>) -> Option<usize> {
    if !root.has_error() {
        return None;
    }
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.is_error() || node.is_missing() {
            return Some(start_line(node));
        }
        let mut cursor = node.walk();
        let start = pending.len();
        pending.extend(node.children(&mut cursor).filter(Node::has_error));
        let Some(pushed) = pending.get_mut(start..) else {
            unreachable!("the stack's length before the pushes is at most its length after");
        };
        pushed.reverse();
    }
    unreachable!("a tree that has an error holds an error or a missing node");
}

#[cfg(test)]
mod tests;
