//! The `SubagentStart` hook's output, built from contexts a test constructs. Each case
//! parses the output as Claude Code does and reads the fields it takes.

use super::*;

fn parsed(output: &str) -> serde_json::Value {
    serde_json::from_str(output).unwrap()
}

mod given_a_context {
    use super::*;

    #[test]
    fn when_rendered_then_it_answers_the_subagent_start_event() {
        let output = parsed(&hook_output("Read code by item."));

        assert_eq!(
            output["hookSpecificOutput"]["hookEventName"],
            "SubagentStart"
        );
    }

    #[test]
    fn when_rendered_then_the_context_is_carried_verbatim() {
        let context = "Read code by item:\n  excerpt outline PATH\n\"quoted\" and `ticked`.\n";

        let output = parsed(&hook_output(context));

        assert_eq!(output["hookSpecificOutput"]["additionalContext"], context);
    }

    #[test]
    fn when_rendered_then_it_is_one_line_of_json() {
        let output = hook_output("Read code by item:\n  excerpt outline PATH\n");

        assert_eq!(output.lines().count(), 1, "{output}");
        assert!(output.ends_with('\n'), "{output}");
    }

    #[test]
    fn when_rendered_at_the_bound_then_it_is_carried_whole() {
        let context = "x".repeat(ADDITIONAL_CONTEXT_CHARACTERS_MAX);

        let output = parsed(&hook_output(&context));

        assert_eq!(output["hookSpecificOutput"]["additionalContext"], context);
    }
}

mod given_a_context_claude_code_cannot_add_whole {
    use super::*;

    #[test]
    #[should_panic]
    fn when_rendered_past_the_bound_then_it_crashes() {
        let context = "x".repeat(ADDITIONAL_CONTEXT_CHARACTERS_MAX + 1);

        let _ = hook_output(&context);
    }

    #[test]
    #[should_panic]
    fn when_rendered_empty_then_it_crashes() {
        let _ = hook_output("");
    }
}
