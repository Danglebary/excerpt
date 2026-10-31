//! The group each kind of thread is counted in, as a row per thread: the main thread, and a
//! subagent under each thing its meta file can say of its type.

use std::io;

use claude_code_transcript::store::{AgentType, Thread};

use super::Group;

fn subagent(agent_type: AgentType) -> Thread {
    Thread::Subagent {
        agent_id: "a1".to_owned(),
        agent_type,
    }
}

mod given_a_thread {
    use super::*;

    #[test]
    fn when_its_group_is_named_then_a_subagent_is_named_by_its_type_or_why_it_has_none() {
        let rows = [
            (Thread::Main, "main"),
            (subagent(AgentType::Named("Explore".to_owned())), "Explore"),
            (subagent(AgentType::Unnamed), "(unnamed)"),
            (subagent(AgentType::NoMeta), "(no meta file)"),
            (subagent(AgentType::Malformed), "(malformed meta file)"),
            (
                subagent(AgentType::Unread(io::ErrorKind::PermissionDenied)),
                "(unread meta file)",
            ),
        ];
        for (thread, name) in rows {
            assert_eq!(Group::of(&thread).name(), name, "{thread:?}");
        }
    }

    #[test]
    fn when_groups_are_sorted_then_the_main_thread_comes_first() {
        let mut groups = vec![
            Group::Subagent("Explore".to_owned()),
            Group::Main,
            Group::Subagent("Plan".to_owned()),
        ];

        groups.sort();

        assert_eq!(
            groups,
            vec![
                Group::Main,
                Group::Subagent("Explore".to_owned()),
                Group::Subagent("Plan".to_owned()),
            ]
        );
    }
}
