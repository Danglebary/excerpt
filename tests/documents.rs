//! The files beside the code that state facts about it: the plugin manifest's version and
//! the commands the skill and the README show. Each case reads the file from the tree and
//! holds it to the crate, so a release bump or a grammar change that leaves one behind
//! fails here.

#[cfg(test)]
mod support {
    use std::path::Path;

    pub fn read(relative: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        std::fs::read_to_string(path).unwrap()
    }

    /// The arguments of every line in a Markdown file's fenced blocks that runs excerpt,
    /// split as a shell splits words, single quotes included.
    pub fn commands(markdown: &str) -> Vec<Vec<String>> {
        let mut commands = Vec::new();
        for block in markdown.split("```").skip(1).step_by(2) {
            for line in block.lines() {
                if let Some(arguments) = line.strip_prefix("excerpt ") {
                    commands.push(words(arguments));
                }
            }
        }
        commands
    }

    fn words(line: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        for character in line.chars() {
            match character {
                '\'' => quoted = !quoted,
                ' ' if !quoted => {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                }
                other => current.push(other),
            }
        }
        assert!(!quoted, "every quote in `{line}` is closed");
        if !current.is_empty() {
            words.push(current);
        }
        words
    }
}

mod given_the_plugin_manifest {
    use super::support::read;

    #[test]
    fn when_read_then_its_version_is_the_crate_version() {
        let manifest: serde_json::Value =
            serde_json::from_str(&read("claude-code/excerpt/.claude-plugin/plugin.json")).unwrap();

        assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    }
}

mod given_the_documents {
    use super::support::{commands, read};
    use excerpt::cli::{Command, parse_arguments};

    fn assert_every_command_parses(relative: &str, at_least: usize) {
        let shown = commands(&read(relative));
        assert!(shown.len() >= at_least, "{relative} shows its commands");
        for arguments in &shown {
            let parsed = parse_arguments(arguments);
            assert!(parsed.is_ok(), "{relative}: {arguments:?}");
            if let Ok(Command::Show { name, .. }) = parsed {
                assert!(excerpt::query::parse(&name).is_ok(), "{relative}: {name}");
            }
        }
    }

    #[test]
    fn when_the_skill_is_read_then_every_command_it_shows_parses() {
        assert_every_command_parses("claude-code/excerpt/skills/excerpt/SKILL.md", 8);
    }

    #[test]
    fn when_the_readme_is_read_then_every_command_it_shows_parses() {
        assert_every_command_parses("README.md", 2);
    }
}
