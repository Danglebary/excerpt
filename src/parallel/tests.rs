//! Work spread over threads, its results handed on in the work's order. The worker count
//! and the attempt's deferral are checked as values. The cases that need threads run real
//! ones: work whose order is checked pauses on each item so the threads' claims
//! interleave, and threads that must meet, or must not, wait for each other up to a
//! deadline, so a pool running too few or too many threads fails rather than hangs.

use std::collections::HashSet;
use std::sync::{Condvar, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use super::*;

/// How long threads that must run at once wait to meet before the case fails.
const MEETING_SECONDS: u64 = 5;
/// How long work waits for more threads than a bound allows to run beside it.
const OVERLAP_MILLISECONDS: u64 = 200;
/// How long the work on each item takes where the order of results or the threads running
/// at once are checked: long enough that every thread claims items between the other
/// threads' claims.
const WORK_MILLISECONDS: u64 = 5;
/// The item whose results the consumer stops after.
const STOPPING_ITEM: usize = 10;

fn available(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).unwrap()
}

fn pool(count: usize) -> Pool {
    Pool::new(available(count))
}

/// Every result `pool` hands on for `items`, in the order it hands them on.
fn consumed<T, R, Attempted, Deferred>(
    pool: Pool,
    items: &[T],
    work: &Work<Attempted, Deferred>,
) -> Vec<R>
where
    T: Sync,
    R: Send,
    Attempted: Fn(&T) -> Attempt<R> + Sync,
    Deferred: Fn(&T) -> R + Sync,
{
    let mut results = Vec::new();
    pool.consume_in_order(items, work, |_, result| {
        results.push(result);
        ControlFlow::Continue(())
    });
    results
}

/// A count shared between threads, and the signal that it changed.
struct Counter {
    count: Mutex<usize>,
    changed: Condvar,
}

impl Counter {
    fn new() -> Self {
        Self {
            count: Mutex::new(0),
            changed: Condvar::new(),
        }
    }

    fn increment(&self) {
        {
            let mut count = self.count.lock().unwrap();
            *count = count.checked_add(1).unwrap();
        }
        self.changed.notify_all();
    }

    /// Whether the count reached `target` within `deadline`.
    fn reaches(&self, target: usize, deadline: Duration) -> bool {
        let below = |count: &mut usize| *count < target;
        let (count, waited) = self
            .changed
            .wait_timeout_while(self.count.lock().unwrap(), deadline, below)
            .unwrap();
        drop(count);
        !waited.timed_out()
    }

    fn value(&self) -> usize {
        *self.count.lock().unwrap()
    }
}

mod given_items {
    use super::*;

    #[test]
    fn when_consumed_then_the_results_come_in_item_order_whether_attempted_or_deferred() {
        let items: Vec<usize> = (0..64).collect();
        let pause = Duration::from_millis(WORK_MILLISECONDS);
        let work = Work {
            attempt: |item: &usize| {
                thread::sleep(pause);
                if item.is_multiple_of(3) {
                    Attempt::Deferred
                } else {
                    Attempt::Done(format!("attempted {item}"))
                }
            },
            deferred: |item: &usize| {
                thread::sleep(pause);
                format!("deferred {item}")
            },
        };
        let expected: Vec<String> = items
            .iter()
            .map(|item| {
                if item.is_multiple_of(3) {
                    format!("deferred {item}")
                } else {
                    format!("attempted {item}")
                }
            })
            .collect();

        for worker_count in [1, 8] {
            let results = consumed(pool(worker_count), &items, &work);

            assert_eq!(results, expected, "{worker_count} workers");
        }
    }

    #[test]
    fn when_consumed_over_as_many_items_as_workers_then_each_runs_at_once() {
        let worker_count = 4;
        let items = vec![(); worker_count];
        let started = Counter::new();
        let deadline = Duration::from_secs(MEETING_SECONDS);
        let work = Work {
            attempt: |(): &()| {
                started.increment();
                Attempt::Done(started.reaches(worker_count, deadline))
            },
            deferred: |(): &()| -> bool { unreachable!("no item is deferred") },
        };

        let met = consumed(pool(worker_count), &items, &work);

        assert_eq!(met, vec![true; worker_count]);
    }

    #[test]
    fn when_the_consumer_breaks_then_it_is_handed_no_later_result() {
        let items: Vec<usize> = (0..64).collect();
        let work = Work {
            attempt: |item: &usize| Attempt::Done(*item),
            deferred: |_: &usize| -> usize { unreachable!("no item is deferred") },
        };

        for worker_count in [1, 8] {
            let mut handed = Vec::new();
            pool(worker_count).consume_in_order(&items, &work, |_, result| {
                handed.push(result);
                if result == STOPPING_ITEM {
                    return ControlFlow::Break(());
                }
                ControlFlow::Continue(())
            });

            let expected: Vec<usize> = (0..=STOPPING_ITEM).collect();
            assert_eq!(handed, expected, "{worker_count} workers");
        }
    }

    #[test]
    fn when_the_consumer_breaks_on_the_first_result_then_no_item_past_the_claims_bound_is_attempted()
     {
        let items: Vec<usize> = (0..CLAIMS_AHEAD_MAX.checked_mul(4).unwrap()).collect();
        let attempted = Counter::new();
        let work = Work {
            attempt: |item: &usize| {
                attempted.increment();
                Attempt::Done(*item)
            },
            deferred: |_: &usize| -> usize { unreachable!("no item is deferred") },
        };

        pool(8).consume_in_order(&items, &work, |_, _| ControlFlow::Break(()));

        let first_and_claimed_past_it = CLAIMS_AHEAD_MAX.checked_add(1).unwrap();
        assert!(
            attempted.value() <= first_and_claimed_past_it,
            "{} attempted",
            attempted.value()
        );
    }

    #[test]
    fn when_the_first_result_is_held_up_then_items_are_claimed_up_to_the_bound_and_no_further() {
        // The first item's attempt waits for the bound's worth of claims, then for one
        // more, so a pool that claims short of the bound fails the first wait and a pool
        // that claims past it ends the second.
        let items: Vec<usize> = (0..CLAIMS_AHEAD_MAX.checked_mul(2).unwrap()).collect();
        let attempted = Counter::new();
        let meeting = Duration::from_secs(MEETING_SECONDS);
        let overlap = Duration::from_millis(OVERLAP_MILLISECONDS);
        let past_the_bound = CLAIMS_AHEAD_MAX.checked_add(1).unwrap();
        let work = Work {
            attempt: |item: &usize| {
                attempted.increment();
                if *item != 0 {
                    return Attempt::Done((true, false));
                }
                let filled = attempted.reaches(CLAIMS_AHEAD_MAX, meeting);
                let overran = attempted.reaches(past_the_bound, overlap);
                Attempt::Done((filled, overran))
            },
            deferred: |_: &usize| -> (bool, bool) { unreachable!("no item is deferred") },
        };

        let results = consumed(pool(8), &items, &work);

        assert_eq!(results.first(), Some(&(true, false)));
    }

    #[test]
    #[should_panic]
    fn when_an_attempt_panics_on_an_item_then_the_consumption_panics() {
        let items: Vec<usize> = (0..100).collect();
        let work = Work {
            attempt: |item: &usize| {
                assert!(*item != 50, "the attempt refuses item 50");
                Attempt::Done(())
            },
            deferred: |_: &usize| unreachable!("no item is deferred"),
        };

        let _ = consumed(pool(4), &items, &work);
    }

    #[test]
    #[should_panic]
    fn when_the_consumer_panics_on_a_result_then_the_consumption_panics() {
        let items: Vec<usize> = (0..100).collect();
        let work = Work {
            attempt: |item: &usize| Attempt::Done(*item),
            deferred: |_: &usize| -> usize { unreachable!("no item is deferred") },
        };

        pool(4).consume_in_order(&items, &work, |_, result| {
            assert!(result != 50, "the consumer refuses result 50");
            ControlFlow::Continue(())
        });
    }
}

mod given_items_the_attempt_defers {
    use super::*;

    #[test]
    fn when_every_item_is_deferred_then_the_deferred_work_runs_its_cap_of_items_at_once() {
        // Each item waits, up to a deadline, for more items than the cap to be running, so
        // a pool over its cap shows more at once and a pool under it never reaches it.
        let items: Vec<usize> = (0..4).collect();
        let running = Mutex::new(Running { now: 0, peak: 0 });
        let changed = Condvar::new();
        let deadline = Duration::from_millis(OVERLAP_MILLISECONDS);
        let work = Work {
            attempt: |_: &usize| Attempt::Deferred,
            deferred: |_: &usize| {
                {
                    let mut state = running.lock().unwrap();
                    state.now = state.now.checked_add(1).unwrap();
                    state.peak = state.peak.max(state.now);
                }
                changed.notify_all();
                let within_cap = |state: &mut Running| state.now <= DEFERRED_WORKERS_MAX;
                let (mut state, _) = changed
                    .wait_timeout_while(running.lock().unwrap(), deadline, within_cap)
                    .unwrap();
                state.now = state.now.checked_sub(1).unwrap();
                drop(state);
                changed.notify_all();
            },
        };

        let _ = consumed(pool(8), &items, &work);

        assert_eq!(running.lock().unwrap().peak, DEFERRED_WORKERS_MAX);
    }

    #[test]
    fn when_many_items_are_deferred_then_their_work_stays_on_the_deferring_threads() {
        let items: Vec<usize> = (0..32).collect();
        let pause = Duration::from_millis(WORK_MILLISECONDS);
        let threads = Mutex::new(HashSet::new());
        let work = Work {
            attempt: |_: &usize| Attempt::<()>::Deferred,
            deferred: |_: &usize| {
                threads.lock().unwrap().insert(thread::current().id());
                thread::sleep(pause);
            },
        };

        let _ = consumed(pool(8), &items, &work);

        let thread_count = threads.lock().unwrap().len();
        assert!(
            thread_count <= DEFERRED_WORKERS_MAX,
            "{thread_count} threads"
        );
    }

    #[test]
    fn when_the_first_item_is_deferred_then_later_items_are_attempted_beside_its_deferred_work() {
        let items: Vec<usize> = (0..2).collect();
        let deferred_started = Counter::new();
        let deadline = Duration::from_secs(MEETING_SECONDS);
        let work = Work {
            attempt: |item: &usize| {
                if *item == 0 {
                    return Attempt::Deferred;
                }
                Attempt::Done(deferred_started.reaches(1, deadline))
            },
            deferred: |_: &usize| {
                deferred_started.increment();
                true
            },
        };

        let met = consumed(pool(8), &items, &work);

        assert_eq!(met, vec![true, true]);
    }

    #[test]
    #[should_panic]
    fn when_deferred_work_panics_on_an_item_then_the_consumption_panics() {
        let items: Vec<usize> = (0..100).collect();
        let work = Work {
            attempt: |item: &usize| {
                if *item == 50 {
                    return Attempt::Deferred;
                }
                Attempt::Done(())
            },
            deferred: |_: &usize| unreachable!("the deferred work refuses every item"),
        };

        let _ = consumed(pool(4), &items, &work);
    }

    #[test]
    fn when_the_first_item_is_deferred_among_more_items_than_the_claims_bound_then_every_result_is_handed_on()
     {
        // The consumption runs on a thread of its own, so a pool that waits on itself
        // fails the case at the deadline rather than hanging it.
        let item_count = CLAIMS_AHEAD_MAX.checked_mul(2).unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let items: Vec<usize> = (0..item_count).collect();
            let work = Work {
                attempt: |item: &usize| {
                    if *item == 0 {
                        return Attempt::Deferred;
                    }
                    Attempt::Done(*item)
                },
                deferred: |item: &usize| *item,
            };
            let results = consumed(pool(8), &items, &work);
            sender.send(results).unwrap();
        });

        let results = receiver
            .recv_timeout(Duration::from_secs(MEETING_SECONDS))
            .unwrap();

        let expected: Vec<usize> = (0..item_count).collect();
        assert_eq!(results, expected);
    }
}

/// How many deferred items are running now, and the most that ever ran at once.
struct Running {
    now: usize,
    peak: usize,
}

mod given_an_input_size {
    use super::*;

    #[test]
    fn when_weighed_for_an_attempt_then_only_an_input_over_its_bound_is_deferred() {
        let over = ATTEMPT_INPUT_BYTES_MAX.checked_add(1).unwrap();
        let rows = [
            (0, false),
            (ATTEMPT_INPUT_BYTES_MAX, false),
            (over, true),
            (u64::MAX, true),
        ];
        for (input_bytes, deferred) in rows {
            assert_eq!(attempt_defers(input_bytes), deferred, "{input_bytes} bytes");
        }
    }
}

mod given_no_items {
    use super::*;

    #[test]
    fn when_consumed_then_no_result_is_handed_on() {
        let items: [usize; 0] = [];
        let work = Work {
            attempt: |item: &usize| Attempt::Done(*item),
            deferred: |item: &usize| *item,
        };

        let results = consumed(pool(8), &items, &work);

        assert_eq!(results, Vec::<usize>::new());
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
