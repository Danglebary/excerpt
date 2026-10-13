---
name: excerpt
description: Read Rust and TypeScript code by item with `excerpt` instead of `grep -n` followed by `sed -n A,Bp`, `head`, or `cat`. Use whenever you would read a function, method, struct, enum, trait, impl, class, interface, or module from a .rs, .ts, or .tsx file, find which file defines an item, or survey what a file or directory declares - `excerpt outline PATH` lists every item with the lines it spans, and `excerpt show NAME [PATH]` prints one whole, its doc comments and attributes included, every line numbered.
---

# Reading code with excerpt

`excerpt` parses Rust and TypeScript and answers two questions in one call each: what does this file or directory declare, and what does this one item say. It replaces the two-step read of finding a line with `grep -n` and guessing a range for `sed -n`, which cuts a function off as often as it reads too much.

Reach for it whenever the thing to read is a declaration in a `.rs`, `.ts`, `.mts`, `.cts`, or `.tsx` file. Text search across files stays with `rg`, and a file that is not code, such as Markdown, TOML, or JSON, stays with the Read tool.

## Survey: outline

```bash
excerpt outline src/plan
```

Prints each source file under the path, in name order, and under it every item with its first and last line, nested items indented beneath their container:

```
src/plan/mod.rs
  94-165 fn plan
  240-300 impl Cursor
    251-260 fn Cursor::new
```

A directory is walked as git sees it, so ignored and hidden files are skipped. An outline stops at 500 items and counts what it left out; outline a narrower path when it does.

## Read one item: show

```bash
excerpt show Cursor::new src/plan
```

Prints the item whole, from the first doc comment or attribute above it to its last line, each line numbered the way the Read tool numbers them:

```
src/plan/mod.rs:251-260 fn Cursor::new
   251	    /// Makes a cursor at the start of the text.
   252	    pub fn new(text: &'t str) -> Self {
```

The path defaults to the working directory, so `excerpt show plan` searches the whole tree; naming a narrower path is faster and finds fewer namesakes.

## Names

A name is what an outline prints after the line numbers, and any trailing part of it:

```bash
excerpt show plan
excerpt show Cursor::new
excerpt show Service.run web
excerpt show 'impl Cursor'
excerpt show 'impl Display for Cursor'
excerpt show 'fn given_input::when_run_then_ok' src/cli
```

Lead with the kind (`fn`, `struct`, `impl`, `class`, `method`, …) to pick one of several items sharing a name, such as a struct and its impl. Quote a name holding spaces.

## When it refuses

A refusal exits 1, prints nothing on stdout, and says on stderr what to do next:

- **Several items answer.** It lists each with its file, lines, and kind. Qualify the name (`Cursor::new` rather than `new`) or lead with its kind, or pass `--all` to print every match, up to 20.
- **No item answers.** Outline the path to see what is there; the name is matched exactly, segment by segment.
- **The item spans more than 1,000 lines.** It lists the item's members; show one of them instead.

A path that does not exist exits 3, and arguments it cannot parse exit 2.

## Editing what you read

The numbered lines feed `splice` directly: a line number becomes `@@ line N`, and a line's text after the tab is a context line once it carries splice's leading space.

```bash
excerpt show Cursor::new src/plan
splice <<'EOF' && cargo test
=== src/plan/mod.rs
@@ line 252
     pub fn new(text: &'t str) -> Self {
-        Self { text, position: 0 }
+        Self { text, position: 0, line: 1 }
EOF
```
