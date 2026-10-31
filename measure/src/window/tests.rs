//! Which instants a window holds, and when it opens after a transcript's last write. Each
//! case is a row of an instant against a window, with the edges of the window as rows.

use jiff::Timestamp;

use super::Window;

fn instant(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn window(since: &str, until: Option<&str>) -> Window {
    Window {
        since: instant(since),
        until: until.map(instant),
    }
}

mod given_a_window {
    use super::*;

    #[test]
    fn when_an_instant_is_asked_then_the_window_holds_it_from_its_start_to_before_its_end() {
        let bounded = window("2026-10-18T00:00:00Z", Some("2026-10-30T00:00:00Z"));
        let open = window("2026-10-18T00:00:00Z", None);
        let rows = [
            (bounded, "2026-10-17T23:59:59Z", false),
            (bounded, "2026-10-18T00:00:00Z", true),
            (bounded, "2026-10-29T23:59:59Z", true),
            (bounded, "2026-10-30T00:00:00Z", false),
            (open, "2026-10-17T23:59:59Z", false),
            (open, "2099-01-01T00:00:00Z", true),
        ];
        for (window, asked, held) in rows {
            assert_eq!(window.holds(instant(asked)), held, "{asked} in {window:?}");
        }
    }

    #[test]
    fn when_a_last_write_is_asked_then_it_opens_after_one_before_its_start_alone() {
        let open = window("2026-10-18T00:00:00Z", None);
        let rows = [
            ("2026-10-17T23:59:59Z", true),
            ("2026-10-18T00:00:00Z", false),
            ("2026-10-19T00:00:00Z", false),
        ];
        for (asked, opens_after) in rows {
            assert_eq!(open.opens_after(instant(asked)), opens_after, "{asked}");
        }
    }
}

mod given_a_window_ending_at_its_start {
    use super::*;

    #[test]
    #[should_panic]
    fn when_an_instant_is_asked_then_it_crashes() {
        let empty = window("2026-10-18T00:00:00Z", Some("2026-10-18T00:00:00Z"));
        let _ = empty.holds(instant("2026-10-18T00:00:00Z"));
    }
}
