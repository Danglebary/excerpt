//! The command line's grammar, each case an argument list and the command or fault it
//! parses to.

use super::*;

fn parsed(words: &[&str]) -> Result<Command, UsageFault> {
    let arguments: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    parse_arguments(&arguments)
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

mod given_outline {
    use super::*;

    #[test]
    fn when_parsed_holding_paths_then_each_is_outlined() {
        assert_eq!(
            parsed(&["outline", "src/lib.rs", "src/main.rs"]),
            Ok(Command::Outline {
                paths: owned(&["src/lib.rs", "src/main.rs"])
            })
        );
    }

    #[test]
    fn when_parsed_holding_no_path_then_the_working_directory_is_outlined() {
        assert_eq!(
            parsed(&["outline"]),
            Ok(Command::Outline {
                paths: owned(&["."])
            })
        );
    }

    #[test]
    fn when_parsed_holding_a_flag_then_it_is_refused() {
        assert_eq!(
            parsed(&["outline", "--all", "src"]),
            Err(UsageFault::UnknownFlag("--all".to_owned()))
        );
    }
}

mod given_show {
    use super::*;

    #[test]
    fn when_parsed_holding_a_name_and_paths_then_the_name_is_shown_from_them() {
        assert_eq!(
            parsed(&["show", "Cursor::new", "src/plan"]),
            Ok(Command::Show {
                name: "Cursor::new".to_owned(),
                all: false,
                paths: owned(&["src/plan"]),
            })
        );
    }

    #[test]
    fn when_parsed_holding_no_path_then_the_name_is_shown_from_the_working_directory() {
        assert_eq!(
            parsed(&["show", "plan"]),
            Ok(Command::Show {
                name: "plan".to_owned(),
                all: false,
                paths: owned(&["."]),
            })
        );
    }

    #[test]
    fn when_parsed_holding_all_after_the_paths_then_every_match_is_shown() {
        assert_eq!(
            parsed(&["show", "new", "src", "--all"]),
            Ok(Command::Show {
                name: "new".to_owned(),
                all: true,
                paths: owned(&["src"]),
            })
        );
    }

    #[test]
    fn when_parsed_holding_no_name_then_it_is_refused() {
        assert_eq!(parsed(&["show", "--all"]), Err(UsageFault::MissingName));
    }

    #[test]
    fn when_parsed_holding_an_unknown_flag_then_it_is_refused() {
        assert_eq!(
            parsed(&["show", "-n", "plan"]),
            Err(UsageFault::UnknownFlag("-n".to_owned()))
        );
    }
}

mod given_no_subcommand {
    use super::*;

    #[test]
    fn when_parsed_holding_nothing_then_it_is_refused() {
        assert_eq!(parsed(&[]), Err(UsageFault::MissingCommand));
    }

    #[test]
    fn when_parsed_holding_an_unknown_word_then_it_is_refused() {
        assert_eq!(
            parsed(&["find", "plan"]),
            Err(UsageFault::UnknownCommand("find".to_owned()))
        );
    }

    #[test]
    fn when_parsed_holding_help_or_version_then_each_is_its_own_command() {
        assert_eq!(parsed(&["--help"]), Ok(Command::Help));
        assert_eq!(parsed(&["-h"]), Ok(Command::Help));
        assert_eq!(parsed(&["--version"]), Ok(Command::Version));
        assert_eq!(parsed(&["-V"]), Ok(Command::Version));
    }
}
