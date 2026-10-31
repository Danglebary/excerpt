# excerpt

A static Rust binary that prints a source file's outline, or one of its items whole by name, and a Claude Code plugin (a skill and a hook) that routes agents to it. `README.md` says what it is for; `excerpt --help`, generated from `src/report/mod.rs`, is the grammar.

The engineering style guide at `~/.claude/CLAUDE.md` governs. This file states only what is specific to this repository.

## Running things

`just check` is the gate and must stay green: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` under the lint table in `Cargo.toml`, `cargo test`, and `claude plugin validate --strict` over the marketplace and the plugin. The Rust recipes run inside the flake's shell, entering `nix develop` themselves unless `EXCERPT_SHELL` shows they already stand in it; the validator runs the `claude` on `PATH`. `just excerpt <arguments>` runs the binary from the tree. The gate compiles for the host's target alone, so code under `cfg(target_env = "musl")`, where the binary takes mimalloc as its allocator, is linted and tested only by CI's `musl` job.

`just bench` runs the criterion benchmarks in `benches/`, which stand outside the gate because wall-clock timings vary between runs; `cargo clippy --all-targets` compiles them, so the gate still holds them to the lint table. Each case checks the outcome it declares before it is timed. `benches/outline.rs` times parsing in process and counts throughput in lines, so linear work holds its lines per second as a file grows; `benches/process.rs` spawns the built binary against a generated tree, so its timings include starting the process and walking the directory. To compare a change, record `just bench -- --save-baseline before` on the parent commit and run `just bench -- --baseline before` on the change.

`just profile <bench> <filter>` records the cases the filter matches under samply, built with the `profiling` profile so frames carry names, and `just profile-view <bench>` opens the recording in the browser. On Linux samply needs `kernel.perf_event_paranoid` at 1 or lower, and on a host of many CPUs `kernel.perf_event_mlock_kb` at 1028 or more.

The toolchain is pinned once, in `rust-toolchain.toml`, which both the flake and CI's rustup read. The grammars are C that cargo compiles through the `cc` crate, so a build needs a C compiler; the flake's shell and its package build carry one, and so do CI's runners.

## Shape

`src/lib.rs` is the functional core and does no IO: the languages and their extensions (`language`), parsing a file's text into its items (`item`, with one classifier per language beside it), the name a reader asks for and which items answer it (`query`), what `show` prints of the matches (`select`), the hook's verdict on a Bash command (`guard`), the output of the hook that briefs each subagent as it starts (`brief`), the argument grammar (`cli`), every message (`report`), spreading work over threads in order (`parallel`), and the exit codes (`exit_code`). `src/main.rs` is the shell: it walks the paths, reads the files and the hook's input under a size bound, prints what the library rendered, and maps outcomes to exit codes. A new decision goes in the library with a unit test; `main.rs` holds wiring, which `tests/cli.rs` covers against real files.

An item's lines run from the first comment or attribute directly above it, with no blank line between, to its last line; a comment trailing code on its own first line and a `//!` module doc belong to no item. Every word of an item's qualified name is text of its file, which is what lets `show` skip parsing a file that lacks a word of the name; a classifier that names an item with text the file does not hold breaks that screen, and the screen's soundness rows in `src/query/tests.rs` are where such a classifier must add a row.

The lint table denies `unwrap`, `expect`, `panic!`, indexing, slicing, string slicing, `as` casts, and unchecked arithmetic. A programmer error crashes through `assert!` or `unreachable!` with a message; arithmetic goes through `checked_*` with a `let ... else { unreachable!(..) }`, and slicing through `.get(..)`.

## Tests

A test is a function named `when_<operation>_then_<result>` in a module named `given_<input>`, and each test file opens with its goal and method as a `//!` comment. Each case asserts on a returned value, a refusal's reason, or what the binary printed; the `report` tests read messages because a message is what that module produces. The `item` tests state a whole outline per case, one row per item, so an item found or lost by mistake fails the case.

`tests/documents.rs` holds the plugin manifest's version equal to the crate's and parses every `excerpt` command the skill and the README show, names included, so neither can drift from the grammar.

The hook's verdict was checked against every Bash command in this machine's Claude Code transcripts; when `guard` changes, rerun that check rather than trusting the unit rows alone: feed each command through `excerpt guard` as hook JSON and read what it blocks.

The README's measured problem comes from this machine's Claude Code transcripts: every Bash call joined to its result, counting `sed -n A,Bp` range reads, `grep -n` calls followed directly by a `sed -n` call, and range reads of a file read within the three calls before. Redo the count when the README's numbers are touched, and rerun it after the plugin has been in use to see whether those numbers fell.

## Releases

Raise `version` in `Cargo.toml` and in `claude-code/excerpt/.claude-plugin/plugin.json` together and merge to `main`. `.github/workflows/release.yml` publishes a release when the version has no release yet and builds nothing otherwise. Once it publishes, it sends Danglebary/dotfiles an `input-released` repository dispatch carrying the input name, the version, and the released commit, with the `DOTFILES_TOKEN` secret. Archives are named by target alone, which `install.sh` depends on.
