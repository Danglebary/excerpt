//! The declarations of TypeScript and TSX: statements at a module's top level and in a
//! namespace, and the methods of a class. An exported declaration spans its `export`
//! keyword and the decorators before it, which the export statement holds.

use tree_sitter::Node;

use super::{Found, Grammar, Kind, field_text, node_text};

pub(super) const GRAMMAR: Grammar = Grammar {
    classify,
    leading: &["comment", "decorator"],
};

/// The name an anonymous default export is asked for by.
const DEFAULT_EXPORT_NAME: &str = "default";

fn classify<'tree>(node: Node<'tree>, text: &str) -> Vec<Found<'tree>> {
    let declaration = match node.kind() {
        "export_statement" => node
            .child_by_field_name("declaration")
            .or_else(|| node.child_by_field_name("value")),
        // A namespace written as a statement parses as an expression.
        "expression_statement" => node.named_child(0),
        _ => Some(node),
    };
    let Some(declaration) = declaration else {
        return Vec::new();
    };
    match declaration.kind() {
        "function_declaration" | "generator_function_declaration" => {
            named(Kind::Function, node, declaration, text)
        }
        "function_expression" | "function" => anonymous(Kind::Function, node, declaration, text),
        "class_declaration" | "abstract_class_declaration" => {
            container(Kind::Class, node, declaration, text)
        }
        "class" => anonymous(Kind::Class, node, declaration, text),
        "interface_declaration" => named(Kind::Interface, node, declaration, text),
        "type_alias_declaration" => named(Kind::Type, node, declaration, text),
        "enum_declaration" => named(Kind::Enum, node, declaration, text),
        "internal_module" | "module" => container(Kind::Namespace, node, declaration, text),
        "lexical_declaration" | "variable_declaration" => declarators(node, declaration, text),
        "method_definition" | "abstract_method_signature" => {
            named(Kind::Method, node, declaration, text)
        }
        "public_field_definition" => function_field(node, declaration, text),
        _ => Vec::new(),
    }
}

/// A declaration named by its `name` field, which spans `node`.
fn named<'tree>(
    kind: Kind,
    node: Node<'tree>,
    declaration: Node<'_>,
    text: &str,
) -> Vec<Found<'tree>> {
    let Some(name) = field_text(declaration, "name", text) else {
        return Vec::new();
    };
    vec![Found {
        kind,
        name,
        node,
        body: None,
    }]
}

/// A default export's function or class, named `default` when it carries no name.
fn anonymous<'tree>(
    kind: Kind,
    node: Node<'tree>,
    declaration: Node<'tree>,
    text: &str,
) -> Vec<Found<'tree>> {
    let name =
        field_text(declaration, "name", text).unwrap_or_else(|| DEFAULT_EXPORT_NAME.to_owned());
    let body = declaration
        .child_by_field_name("body")
        .map(|body| (body, name.clone()));
    vec![Found {
        kind,
        name,
        node,
        body,
    }]
}

/// A class or namespace, whose body's declarations are qualified by its name.
fn container<'tree>(
    kind: Kind,
    node: Node<'tree>,
    declaration: Node<'tree>,
    text: &str,
) -> Vec<Found<'tree>> {
    let Some(name) = field_text(declaration, "name", text) else {
        return Vec::new();
    };
    let body = declaration
        .child_by_field_name("body")
        .map(|body| (body, name.clone()));
    vec![Found {
        kind,
        name,
        node,
        body,
    }]
}

/// One item per name a `const`, `let`, or `var` statement declares, each spanning the
/// whole statement; a destructuring pattern declares no single name and is left out.
fn declarators<'tree>(node: Node<'tree>, declaration: Node<'_>, text: &str) -> Vec<Found<'tree>> {
    let keyword = field_text(declaration, "kind", text).unwrap_or_else(|| "var".to_owned());
    let Some(kind) = Kind::of_keyword(&keyword) else {
        return Vec::new();
    };
    let mut cursor = declaration.walk();
    let mut found = Vec::new();
    for declarator in declaration.named_children(&mut cursor) {
        if declarator.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue;
        }
        found.push(Found {
            kind,
            name: node_text(name_node, text).to_owned(),
            node,
            body: None,
        });
    }
    found
}

/// A class field holding a function is a method in all but syntax; a field holding any
/// other value is state, and is left out.
fn function_field<'tree>(
    node: Node<'tree>,
    declaration: Node<'_>,
    text: &str,
) -> Vec<Found<'tree>> {
    let Some(value) = declaration.child_by_field_name("value") else {
        return Vec::new();
    };
    match value.kind() {
        "arrow_function" | "function_expression" | "function" => {
            named(Kind::Method, node, declaration, text)
        }
        _ => Vec::new(),
    }
}
