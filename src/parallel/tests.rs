//! Work spread over threads, its results in the work's order. The worker count and the
//! first pass's deferral are checked as values. The cases that need threads run real
//! ones: work whose order is checked pauses on each item so the threads' claims
//! interleave, and threads that must meet, or must not, wait for each other up to a
//! deadline, so a pool running too few or too many threads fails rather than hangs.

use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::Duration;

use super::*;

/// How long threads that must run at once wait to meet before the case fails.
const MEETING_SECONDS: u64 = 5;
/// How long each second-pass item waits for more items than the cap to run beside it.
const OVERLAP_MILLISECONDS: u64 = 200;
/// How long the work on each item takes where the order of results or the threads running
/// at once are checked: long enough that every thread claims items between the other
/// threads' claims.
const WORK_MILLISECONDS: u64 = 5;

fn available(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).unwrap()
}

fn pool(count: usize) -> Pool {
    Pool::new(available(count))
}

mod given_items {
    use super::*;

    #[test]
    fn when_mapped_then_the_results_come_in_item_order() {
        let items: Vec<usize> = (0..64).collect();
        let pause = Duration::from_millis(WORK_MILLISECONDS);

        let mapped = pool(8).map_in_order(&items, |item| {
            thread::sleep(pause);
            item.checked_mul(3).unwrap()
        });

        let expected: Vec<usize> = items
            .iter()
            .map(|item| item.checked_mul(3).unwrap())
            .collect();
        assert_eq!(mapped, expected);
    }

    #[test]
    fn when_mapped_over_as_many_items_as_workers_then_each_runs_at_once() {
        let worker_count = 4;
        let items = vec![(); worker_count];
        let started = Mutex::new(0_usize);
        let all_started = Condvar::new();
        let deadline = Duration::from_secs(MEETING_SECONDS);

        let met = pool(worker_count).map_in_order(&items, |()| {
            {
                let mut count = started.lock().unwrap();
                *count = count.checked_add(1).unwrap();
            }
            all_started.notify_all();
            let still_arriving = |count: &mut usize| *count < worker_count;
            let (count, waited) = all_started
                .wait_timeout_while(started.lock().unwrap(), deadline, still_arriving)
                .unwrap();
            drop(count);
            !waited.timed_out()
        });

        assert_eq!(met, vec![true; worker_count]);
    }

    #[test]
    #[should_panic]
    fn when_work_panics_on_an_item_then_the_map_panics() {
        let items: Vec<usize> = (0..100).collect();

        let _ = pool(4).map_in_order(&items, |item| {
            assert!(*item != 50, "the work refuses item 50");
        });
    }
}

mod given_items_the_first_pass_defers {
    use super::*;

    #[test]
    fn when_mapped_in_two_passes_then_each_result_takes_its_item_s_place() {
        let items: Vec<usize> = (0..64).collect();
        let pause = Duration::from_millis(WORK_MILLISECONDS);
        let passes = Passes {
            first: |item: &usize| {
                thread::sleep(pause);
                if item.is_multiple_of(3) {
                    Attempt::Deferred
                } else {
                    Attempt::Done(format!("first {item}"))
                }
            },
            second: |item: &usize| {
                thread::sleep(pause);
                format!("second {item}")
            },
        };

        let mapped = pool(8).map_in_two_passes(&items, passes);

        let expected: Vec<String> = items
            .iter()
            .map(|item| {
                if item.is_multiple_of(3) {
                    format!("second {item}")
                } else {
                    format!("first {item}")
                }
            })
            .collect();
        assert_eq!(mapped, expected);
    }

    #[test]
    fn when_every_item_is_deferred_then_the_second_pass_works_its_cap_of_items_at_once() {
        // Each item waits, up to a deadline, for more items than the cap to be running, so
        // a pass over its cap shows more at once and a pass under it never reaches it.
        let items: Vec<usize> = (0..4).collect();
        let cap = SECOND_PASS_WORKERS_MAX.get();
        let running = Mutex::new(Running { now: 0, peak: 0 });
        let changed = Condvar::new();
        let deadline = Duration::from_millis(OVERLAP_MILLISECONDS);
        let passes = Passes {
            first: |_: &usize| Attempt::Deferred,
            second: |_: &usize| {
                {
                    let mut state = running.lock().unwrap();
                    state.now = state.now.checked_add(1).unwrap();
                    state.peak = state.peak.max(state.now);
                }
                changed.notify_all();
                let within_cap = |state: &mut Running| state.now <= cap;
                let (mut state, _) = changed
                    .wait_timeout_while(running.lock().unwrap(), deadline, within_cap)
                    .unwrap();
                state.now = state.now.checked_sub(1).unwrap();
                drop(state);
                changed.notify_all();
            },
        };

        let _ = pool(8).map_in_two_passes(&items, passes);

        assert_eq!(running.lock().unwrap().peak, cap);
    }
}

/// How many second-pass items are running now, and the most that ever ran at once.
struct Running {
    now: usize,
    peak: usize,
}

mod given_an_input_size {
    use super::*;

    #[test]
    fn when_weighed_for_the_first_pass_then_only_an_input_over_its_bound_is_deferred() {
        let over = FIRST_PASS_INPUT_BYTES_MAX.checked_add(1).unwrap();
        let rows = [
            (0, false),
            (FIRST_PASS_INPUT_BYTES_MAX, false),
            (over, true),
            (u64::MAX, true),
        ];
        for (input_bytes, deferred) in rows {
            assert_eq!(
                first_pass_defers(input_bytes),
                deferred,
                "{input_bytes} bytes"
            );
        }
    }
}

mod given_no_items {
    use super::*;

    #[test]
    fn when_mapped_then_no_result_comes_back() {
        let items: [usize; 0] = [];

        let mapped = pool(8).map_in_order(&items, |item| *item);

        assert_eq!(mapped, Vec::<usize>::new());
    }
}

mod given_a_machine {
    use super::*;

    #[test]
    fn when_workers_are_counted_then_the_machine_the_cap_and_the_items_bound_them() {
        let rows = [
            (32, 200, WORKERS_MAX),
            (4, 200, 4),
            (32, 3, 3),
            (32, 0, 1),
            (1, 200, 1),
        ];
        for (offered, item_count, expected) in rows {
            assert_eq!(
                worker_count(available(offered), item_count),
                expected,
                "{offered} offered, {item_count} items"
            );
        }
    }
}
