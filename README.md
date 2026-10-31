# excerpt

Coding agents read code from Bash in two steps: `grep -n` to find the line a function starts on, then `sed -n 330,478p` to print a range they guess. The guess misses both ways. A range that stops short cuts the function off and costs another read, and a range that runs long fills the context with code nobody asked for. In my personal homelab server's Claude Code sessions through 2026-10-10, agents printed 2,846 line ranges with `sed -n`, 1,108 of them from Rust files; 1,170 times a `grep -n` call was followed directly by a `sed -n` call, and 200 range reads re-read a file within three calls of the last, starting inside or just past the range before.

excerpt reads by item instead. It parses Rust and TypeScript with [tree-sitter](https://tree-sitter.github.io) and answers in one call what a file or directory declares, or what one item says, from the first doc comment above it to its last line.

```bash
excerpt outline src/plan
```

```
src/plan/mod.rs
  94-165 fn plan
  240-300 impl Cursor
    251-260 fn Cursor::new
```

```bash
excerpt show Cursor::new src/plan
```

```
src/plan/mod.rs:251-260 fn Cursor::new
   251	    /// Makes a cursor at the start of the text.
   252	    pub fn new(text: &'t str) -> Self {
   ...
```

A name is what an outline prints after the line numbers, or any trailing part of it, optionally led by its kind:

| Name | Answered by |
| --- | --- |
| `plan` | every item named `plan` |
| `Cursor::new`, `Service.run` | the method `new` of `Cursor`, the method `run` of `Service` |
| `impl Cursor` | the impl block, not the struct it shares a name with |
| `impl Display for Cursor` | a trait impl |

When several items answer a name, `show` lists them and exits 1 rather than choosing one; `--all` prints every match, up to 20. An item longer than 1,000 lines is refused with a list of its members. A directory is walked as git sees it, skipping ignored and hidden files, and an outline stops at 500 items, counting the items left in the file it stops in and the files after it. `excerpt --help` prints the whole grammar.

excerpt reads `.rs`, `.ts`, `.mts`, `.cts`, and `.tsx`. A grammar is compiled into the binary, so a language joins when transcripts show agents reading it by range.

## Install

The binary, from the latest release (Linux and macOS, x86_64 and aarch64):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/Danglebary/excerpt/main/install.sh | sh
```

It installs to `~/.local/bin`, or to `$EXCERPT_INSTALL_DIR`. With Nix, `nix profile install github:Danglebary/excerpt`; with cargo, `cargo install --locked --git https://github.com/Danglebary/excerpt`.

The Claude Code plugin, once the binary is on `PATH`, adds a skill that teaches agents both commands and a hook that holds them to it:

```
/plugin marketplace add Danglebary/excerpt
/plugin install excerpt@excerpt
```

The hook runs `excerpt guard` before every Bash call. It blocks a command that prints lines of a file excerpt reads through `sed -n` line numbers, `head`, `tail`, or `cat`, and its refusal hands the agent the `outline` and `show` commands for that file. Standard input, pattern prints, output redirected to a file, and every other kind of file pass. Without the binary on `PATH` every Bash call reports a non-blocking hook error, which is the signal to install it.

## Exit status

| Code | Meaning |
| --- | --- |
| 0 | printed |
| 1 | refused: no item answers the name, several do, one is too long to print, or a named file is in a language excerpt does not read |
| 2 | the arguments are malformed |
| 3 | reading a path failed; what was printed may be incomplete |

`excerpt guard` speaks Claude Code's hook codes instead: 0 lets the command run, 2 blocks it, and 1 reports a hook input it cannot read without blocking the command.

## Development

`just check` is the gate: the format check, clippy with every warning denied, the tests, and Claude Code's plugin validator. The toolchain is the one `rust-toolchain.toml` pins; the flake's shell provides it, and the recipes enter that shell themselves.

A release is cut by raising `version` in `Cargo.toml` and in `claude-code/excerpt/.claude-plugin/plugin.json` (a test holds the two equal) and merging to `main`; the release workflow builds and publishes it.

## License

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
