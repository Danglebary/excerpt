//! The extension table, each row a path and the language it names or none.

use super::*;

mod given_a_path {
    use super::*;

    #[test]
    fn when_classified_then_its_extension_names_the_language() {
        let rows = [
            ("src/lib.rs", Some(Language::Rust)),
            ("web/app.ts", Some(Language::TypeScript)),
            ("web/app.mts", Some(Language::TypeScript)),
            ("web/app.cts", Some(Language::TypeScript)),
            ("web/view.tsx", Some(Language::Tsx)),
            ("web/app.js", None),
            ("README.md", None),
            ("Makefile", None),
        ];
        for (path, language) in rows {
            assert_eq!(Language::of_path(Path::new(path)), language, "{path}");
        }
    }
}
