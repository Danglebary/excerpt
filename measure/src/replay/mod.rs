//! Bash calls naming a code file, by the guard's verdict when they ran and now.
//!
//! The verdict then is whether the call drew a refusal, and the verdict now is what the
//! guard built from this tree says of the command. The sessions holding a call that passed
//! then and is denied now are counted by whether the session drew any refusal.

use std::collections::{BTreeMap, HashMap};

use excerpt::guard::{self, Reader, Verdict};

use crate::facts::Facts;
use crate::{Full, one_more, sum};

/// The most sessions the tally counts apart.
pub const SESSIONS_MAX: usize = 100_000;

/// The first table's header row.
const VERDICT_HEADER: &str = "then\tnow\tcalls\n";

/// The second table's header row.
const SESSION_HEADER: &str = "session_refusals\tsessions\tcalls\n";

/// What the guard did with a call when it ran. Passed sorts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Then {
    Passed,
    Refused,
}

/// The refusals one session drew, and its calls that passed when they ran and are denied
/// now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Session {
    refusals: usize,
    newly_denied: usize,
}

/// The sessions holding a call newly denied, of one kind, and those calls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Sessions {
    sessions: usize,
    calls: usize,
}

/// The calls counted so far by verdict then and now, and the sessions they ran in.
#[derive(Debug, Default)]
pub struct Replay {
    verdicts: BTreeMap<(Then, &'static str), usize>,
    sessions: HashMap<String, Session>,
}

impl Then {
    const fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Refused => "refused",
        }
    }
}

impl Replay {
    /// A tally having counted nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Judges each Bash call naming a code file that a transcript of `session` made, and
    /// counts its refusals, Reads' among them.
    ///
    /// # Errors
    ///
    /// [`Full::Sessions`] where a new session would pass [`SESSIONS_MAX`].
    pub fn take(&mut self, session: &str, facts: &Facts) -> Result<(), Full> {
        let mut drawn = Session::default();
        for command in &facts.commands {
            let then = if command.refused {
                Then::Refused
            } else {
                Then::Passed
            };
            let verdict = guard::verdict(&command.command);
            let now = now_of(&verdict);
            let calls = self.verdicts.entry((then, now)).or_insert(0);
            *calls = one_more(*calls);
            match (then, verdict) {
                (Then::Refused, _) => drawn.refusals = one_more(drawn.refusals),
                (Then::Passed, Verdict::Deny(_)) => {
                    drawn.newly_denied = one_more(drawn.newly_denied);
                }
                (Then::Passed, Verdict::Allow) => {}
            }
        }
        for read in &facts.reads {
            if read.refused {
                drawn.refusals = one_more(drawn.refusals);
            }
        }
        if drawn == Session::default() {
            Ok(())
        } else {
            self.count_session(session, drawn)
        }
    }

    /// The two tables: by verdict then and now, passed first; and the sessions holding a
    /// call newly denied, those that drew no refusal first.
    #[must_use]
    pub fn tables(&self) -> String {
        let mut tables = String::from(VERDICT_HEADER);
        for ((then, now), calls) in &self.verdicts {
            let row = format!("{}\t{now}\t{calls}\n", then.name());
            tables.push_str(&row);
        }
        let mut quiet = Sessions::default();
        let mut refused = Sessions::default();
        for session in self.sessions.values() {
            if session.newly_denied > 0 {
                let counted = if session.refusals == 0 {
                    &mut quiet
                } else {
                    &mut refused
                };
                counted.sessions = one_more(counted.sessions);
                counted.calls = sum(counted.calls, session.newly_denied);
            }
        }
        tables.push('\n');
        tables.push_str(SESSION_HEADER);
        let quiet_row = format!("none\t{}\t{}\n", quiet.sessions, quiet.calls);
        let refused_row = format!("some\t{}\t{}\n", refused.sessions, refused.calls);
        tables.push_str(&quiet_row);
        tables.push_str(&refused_row);
        tables
    }

    /// Adds what one transcript drew to its session's counts.
    fn count_session(&mut self, session: &str, drawn: Session) -> Result<(), Full> {
        let known = self.sessions.contains_key(session);
        if known || self.sessions.len() < SESSIONS_MAX {
            let counts = self.sessions.entry(session.to_owned()).or_default();
            counts.refusals = sum(counts.refusals, drawn.refusals);
            counts.newly_denied = sum(counts.newly_denied, drawn.newly_denied);
            assert!(
                self.sessions.len() <= SESSIONS_MAX,
                "the sessions stop at their bound"
            );
            Ok(())
        } else {
            Err(Full::Sessions)
        }
    }
}

/// What the guard says of a call now, as the first table names it.
const fn now_of(verdict: &Verdict) -> &'static str {
    match verdict {
        Verdict::Allow => "allowed",
        Verdict::Deny(denial) => match denial.reader {
            Reader::SedRange => "denied: sed range",
            Reader::AwkRange => "denied: awk range",
            Reader::WholeFileRead => "denied: whole-file Read",
            Reader::Head => "denied: head",
            Reader::Tail => "denied: tail",
            Reader::Cat => "denied: cat",
        },
    }
}

#[cfg(test)]
mod tests;
