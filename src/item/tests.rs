//! The items found in a file's text, per language. Each case parses a small source and
//! compares every item it finds, in document order, as one row of its lines, kind, and
//! qualified name, so a case states the whole outline it expects and an item found or
//! lost by mistake fails it.

use super::*;

fn rows(language: Language, text: &str) -> Vec<String> {
    let found = outline(language, text);
    found
        .items
        .iter()
        .map(|item| {
            format!(
                "{}-{} {} {}",
                item.lines.first(),
                item.lines.last(),
                item.kind.keyword(),
                item.qualified_name(language)
            )
        })
        .collect()
}

mod given_rust {
    use super::*;

    fn rust(text: &str) -> Vec<String> {
        rows(Language::Rust, text)
    }

    #[test]
    fn when_outlined_then_a_function_spans_its_doc_comment_and_attributes() {
        let text = "\
/// Adds.
#[must_use]
pub fn add(a: u8, b: u8) -> u8 {
    a + b
}
";
        assert_eq!(rust(text), ["1-5 fn add"]);
    }

    #[test]
    fn when_outlined_then_each_declaration_kind_is_named_by_its_keyword() {
        let text = "\
struct Point { x: u8 }
enum Mode { A }
union Bits { word: u32 }
type Pair = (u8, u8);
const LIMIT: usize = 3;
static NAME: &str = \"x\";
macro_rules! twice { ($e:expr) => { $e; $e }; }
";
        assert_eq!(
            rust(text),
            [
                "1-1 struct Point",
                "2-2 enum Mode",
                "3-3 union Bits",
                "4-4 type Pair",
                "5-5 const LIMIT",
                "6-6 static NAME",
                "7-7 macro twice",
            ]
        );
    }

    #[test]
    fn when_outlined_then_an_impl_is_named_by_its_type_and_qualifies_its_methods() {
        let text = "\
impl<'t> Cursor<'t> {
    /// Makes one.
    pub fn new(text: &'t str) -> Self {
        Self { text }
    }

    const STEP: usize = 1;
}
";
        assert_eq!(
            rust(text),
            [
                "1-8 impl Cursor",
                "2-5 fn Cursor::new",
                "7-7 const Cursor::STEP"
            ]
        );
    }

    #[test]
    fn when_outlined_then_a_trait_impl_is_named_for_its_type_and_methods_belong_to_the_type() {
        let text = "\
impl fmt::Display for Cursor<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}
";
        assert_eq!(
            rust(text),
            ["1-5 impl Display for Cursor", "2-4 fn Cursor::fmt"]
        );
    }

    #[test]
    fn when_outlined_then_an_impl_for_a_reference_is_named_by_the_referenced_type() {
        let text = "impl<'a> From<&'a str> for &'a Name {}\n";

        assert_eq!(rust(text), ["1-1 impl From for Name"]);
    }

    #[test]
    fn when_outlined_then_a_trait_qualifies_its_declarations() {
        let text = "\
pub trait Shape {
    const SIDES: usize;
    fn area(&self) -> f64;
    fn name(&self) -> &str { \"shape\" }
}
";
        assert_eq!(
            rust(text),
            [
                "1-5 trait Shape",
                "2-2 const Shape::SIDES",
                "3-3 fn Shape::area",
                "4-4 fn Shape::name",
            ]
        );
    }

    #[test]
    fn when_outlined_then_an_inline_module_qualifies_its_items_and_a_declared_one_is_one_line() {
        let text = "\
mod tests;

#[cfg(test)]
mod given_input {
    fn when_run_then_ok() {}
}
";
        assert_eq!(
            rust(text),
            [
                "1-1 mod tests",
                "3-6 mod given_input",
                "5-5 fn given_input::when_run_then_ok",
            ]
        );
    }

    #[test]
    fn when_outlined_then_a_comment_above_a_blank_line_is_left_out() {
        let text = "\
// A note about the section.

fn alone() {}
";
        assert_eq!(rust(text), ["3-3 fn alone"]);
    }

    #[test]
    fn when_outlined_then_a_module_doc_comment_is_left_out() {
        let text = "\
//! The module.
fn first() {}
";
        assert_eq!(rust(text), ["2-2 fn first"]);
    }

    #[test]
    fn when_outlined_then_a_comment_trailing_the_previous_item_is_left_out() {
        let text = "\
const A: u8 = 1; // one
fn b() {}
";
        assert_eq!(rust(text), ["1-1 const A", "2-2 fn b"]);
    }

    #[test]
    fn when_outlined_then_a_comment_trailing_an_opening_brace_is_left_out() {
        let text = "\
impl Cursor { // The cursor.
    fn new() -> Self { Self }
}
";
        assert_eq!(rust(text), ["1-3 impl Cursor", "2-2 fn Cursor::new"]);
    }

    #[test]
    fn when_outlined_then_a_comment_below_a_trailing_one_is_kept() {
        let text = "\
const A: u8 = 1; // one
// two
fn b() {}
";
        assert_eq!(rust(text), ["1-1 const A", "2-3 fn b"]);
    }

    #[test]
    fn when_outlined_then_a_function_inside_a_function_body_is_not_an_item() {
        let text = "\
fn outer() {
    fn inner() {}
}
";
        assert_eq!(rust(text), ["1-3 fn outer"]);
    }

    #[test]
    fn when_outlined_then_modules_nested_past_the_bound_are_left_out() {
        use std::fmt::Write as _;

        let mut text = String::new();
        for depth in 0..=NESTING_DEPTH_MAX {
            writeln!(text, "mod m{depth} {{").unwrap();
        }
        for _ in 0..=NESTING_DEPTH_MAX {
            text.push_str("}\n");
        }

        let found = outline(Language::Rust, &text);

        assert_eq!(found.items.len(), NESTING_DEPTH_MAX);
    }

    #[test]
    fn when_outlined_holding_no_syntax_error_then_none_is_reported() {
        assert_eq!(
            outline(Language::Rust, "fn ok() {}\n").first_error_line,
            None
        );
    }

    #[test]
    fn when_outlined_holding_a_syntax_error_then_its_first_line_is_reported() {
        let text = "\
fn ok() {}

fn broken( {
";
        assert_eq!(outline(Language::Rust, text).first_error_line, Some(3));
    }
}

mod given_typescript {
    use super::*;

    fn typescript(text: &str) -> Vec<String> {
        rows(Language::TypeScript, text)
    }

    #[test]
    fn when_outlined_then_an_exported_function_spans_its_doc_comment_and_export() {
        let text = "\
/** Adds. */
export function add(a: number, b: number): number {
  return a + b;
}
";
        assert_eq!(typescript(text), ["1-4 function add"]);
    }

    #[test]
    fn when_outlined_then_each_declaration_kind_is_named_by_its_keyword() {
        let text = "\
export interface Port { run(id: string): void; }
export type Id = string;
export enum Mode { A, B }
let count = 0;
var legacy = 1;
";
        assert_eq!(
            typescript(text),
            [
                "1-1 interface Port",
                "2-2 type Id",
                "3-3 enum Mode",
                "4-4 let count",
                "5-5 var legacy",
            ]
        );
    }

    #[test]
    fn when_outlined_then_each_declarator_of_a_declaration_is_an_item() {
        let text = "\
const LIMIT = 3, twice = (n: number) => n * 2;
export const handler = async (event: Event): Promise<void> => {
  await run(event);
};
";
        assert_eq!(
            typescript(text),
            ["1-1 const LIMIT", "1-1 const twice", "2-4 const handler"]
        );
    }

    #[test]
    fn when_outlined_then_a_class_qualifies_its_methods_with_their_comments_and_decorators() {
        let text = "\
/** A service. */
@Injectable()
export class Service extends Base {
  private readonly cache = new Map<string, number>();

  /** Runs. */
  @Get(\":id\")
  async run(id: string): Promise<void> {}

  get size(): number { return 0; }

  static create = () => new Service();
}
";
        assert_eq!(
            typescript(text),
            [
                "1-13 class Service",
                "6-8 method Service.run",
                "10-10 method Service.size",
                "12-12 method Service.create",
            ]
        );
    }

    #[test]
    fn when_outlined_then_an_abstract_class_lists_its_abstract_methods() {
        let text = "export abstract class Shape { abstract area(): number; }\n";

        assert_eq!(
            typescript(text),
            ["1-1 class Shape", "1-1 method Shape.area"]
        );
    }

    #[test]
    fn when_outlined_then_a_namespace_qualifies_its_items() {
        let text = "\
namespace Inner {
  export function deep(): void {}
}
";
        assert_eq!(
            typescript(text),
            ["1-3 namespace Inner", "2-2 function Inner.deep"]
        );
    }

    #[test]
    fn when_outlined_then_an_anonymous_default_export_is_named_default() {
        let text = "export default function () {}\n";

        assert_eq!(typescript(text), ["1-1 function default"]);
    }

    #[test]
    fn when_outlined_then_a_reexport_is_not_an_item() {
        let text = "\
export { add } from \"./math\";
export * from \"./shapes\";
";
        assert_eq!(typescript(text), Vec::<String>::new());
    }

    #[test]
    fn when_outlined_as_tsx_then_a_component_returning_markup_is_an_item() {
        let text = "\
export function Badge({ label }: Props) {
  return <span className=\"badge\">{label}</span>;
}
";
        assert_eq!(rows(Language::Tsx, text), ["1-3 function Badge"]);
    }
}

mod given_an_outline {
    use super::*;

    #[test]
    fn when_members_are_read_then_they_are_the_items_nested_inside_one() {
        let text = "\
impl Cursor {
    fn new() -> Self { Self }
    fn step(&mut self) {}
}
fn after() {}
";
        let found = outline(Language::Rust, text);

        let names: Vec<String> = members(&found.items, 0)
            .iter()
            .map(|item| item.qualified_name(Language::Rust))
            .collect();

        assert_eq!(names, ["Cursor::new", "Cursor::step"]);
    }
}
