//! The declarations of Rust: items at a module's top level, in an inline module, and in
//! an impl's or a trait's body. A function's body is never descended into, so a helper
//! declared inside a function is part of that function.

use tree_sitter::Node;

use super::{Found, Grammar, Kind, field_text, node_text};

pub(super) const GRAMMAR: Grammar = Grammar {
    classify,
    leading: &["line_comment", "block_comment", "attribute_item"],
};

/// How many wrappers a type is unwrapped through, a reference to a generic in a module
/// path among them, before its name is taken as written.
const TYPE_UNWRAPPINGS_MAX: usize = 8;

fn classify<'tree>(node: Node<'tree>, text: &str) -> Vec<Found<'tree>> {
    if node.kind() == "impl_item" {
        return impl_found(node, text).into_iter().collect();
    }
    let kind = match node.kind() {
        "function_item" | "function_signature_item" => Kind::Fn,
        "struct_item" => Kind::Struct,
        "enum_item" => Kind::Enum,
        "union_item" => Kind::Union,
        "type_item" => Kind::Type,
        "const_item" => Kind::Const,
        "static_item" => Kind::Static,
        "macro_definition" => Kind::Macro,
        "trait_item" => Kind::Trait,
        "mod_item" => Kind::Mod,
        _ => return Vec::new(),
    };
    let Some(name) = field_text(node, "name", text) else {
        return Vec::new();
    };
    let body = match node.child_by_field_name("body") {
        Some(body) if matches!(kind, Kind::Trait | Kind::Mod) => Some((body, name.clone())),
        _ => None,
    };
    vec![Found { kind, name, body }]
}

/// An impl is named by its type, `Cursor`, or by its trait and type, `Display for
/// Cursor`; either way the methods inside are qualified by the type alone, which is how
/// a caller names them.
fn impl_found<'tree>(node: Node<'tree>, text: &str) -> Option<Found<'tree>> {
    let type_node = node.child_by_field_name("type")?;
    let type_name = bare_type_name(type_node, text)?;
    let name = match node.child_by_field_name("trait") {
        Some(trait_node) => {
            let trait_name = bare_type_name(trait_node, text)?;
            format!("{trait_name} for {type_name}")
        }
        None => type_name.clone(),
    };
    let body = node
        .child_by_field_name("body")
        .map(|body| (body, type_name));
    Some(Found {
        kind: Kind::Impl,
        name,
        body,
    })
}

/// The bare name of a type: `Cursor` for `Cursor<'t>`, `&'a Cursor`, or `text::Cursor`.
/// A type of any other shape, a tuple or a slice, is named as written.
fn bare_type_name(node: Node<'_>, text: &str) -> Option<String> {
    let mut current = node;
    for _ in 0..TYPE_UNWRAPPINGS_MAX {
        let inner = match current.kind() {
            "generic_type" | "reference_type" | "pointer_type" => {
                current.child_by_field_name("type")
            }
            "scoped_type_identifier" => current.child_by_field_name("name"),
            _ => return Some(node_text(current, text).to_owned()),
        };
        current = inner?;
    }
    None
}
