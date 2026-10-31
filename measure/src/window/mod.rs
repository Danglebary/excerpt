//! The span of time a measure counts, by the instant each call was made.

use jiff::Timestamp;

/// The instants a measure counts: from `since`, and before `until` where one is named.
/// `until`, where named, falls after `since`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub since: Timestamp,
    pub until: Option<Timestamp>,
}

impl Window {
    /// Whether the window holds `instant`.
    ///
    /// # Panics
    ///
    /// When `until` falls at or before `since`, which the argument grammar refuses.
    #[must_use]
    pub fn holds(&self, instant: Timestamp) -> bool {
        if let Some(until) = self.until {
            assert!(until > self.since, "a window ends after it begins");
        }
        if instant >= self.since {
            match self.until {
                Some(until) => instant < until,
                None => true,
            }
        } else {
            false
        }
    }

    /// Whether the window opens after `instant`, so a transcript last written then holds
    /// no record the window holds.
    #[must_use]
    pub fn opens_after(&self, instant: Timestamp) -> bool {
        self.since > instant
    }
}

#[cfg(test)]
mod tests;
