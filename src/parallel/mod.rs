//! Work spread over a bounded number of threads, its results kept in the work's order.
//!
//! Nothing here does IO, so the order and the bounds on threads are tested with plain
//! values.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// The most threads one map does work on, the calling thread among them.
pub const WORKERS_MAX: usize = 16;
/// The most threads the second pass of `Pool::map_in_two_passes` does work on.
pub const SECOND_PASS_WORKERS_MAX: NonZeroUsize = match NonZeroUsize::new(2) {
    Some(count) => count,
    None => unreachable!(),
};
/// The most bytes of input `first_pass_defers` keeps on the first pass.
pub const FIRST_PASS_INPUT_BYTES_MAX: u64 = 262_144;
/// The stack each spawned worker gets, set here rather than left to `RUST_MIN_STACK` so it
/// is the same in every environment.
const WORKER_STACK_BYTES: usize = 8_388_608;

/// The threads work is spread over: as many as the machine offers, up to `WORKERS_MAX`.
#[derive(Debug, Clone, Copy)]
pub struct Pool {
    available: NonZeroUsize,
}

/// The work of `Pool::map_in_two_passes`: `first` runs on every item and may defer it, and
/// `second` runs on each item `first` deferred.
pub struct Passes<First, Second> {
    pub first: First,
    pub second: Second,
}

/// What the first pass made of one item: its result, or a deferral to the second pass.
pub enum Attempt<R> {
    Done(R),
    Deferred,
}

impl Pool {
    /// A pool of up to `available` threads, the calling thread among them.
    #[must_use]
    pub const fn new(available: NonZeroUsize) -> Self {
        Self { available }
    }

    /// `work` applied to every item, the results in the order of `items`.
    ///
    /// The calling thread and the threads spawned beside it, no more in all than the
    /// pool's threads, `WORKERS_MAX`, or the number of items, each claim the next
    /// unclaimed item until none is left.
    ///
    /// # Panics
    ///
    /// When `work` panics on any item.
    #[must_use]
    pub fn map_in_order<T, R, F>(self, items: &[T], work: F) -> Vec<R>
    where
        T: Sync,
        R: Send,
        F: Fn(&T) -> R + Sync,
    {
        let workers = worker_count(self.available, items.len());
        let claims = Claims {
            items,
            next: AtomicUsize::new(0),
        };
        let finished = thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 1..workers {
                let builder = thread::Builder::new().stack_size(WORKER_STACK_BYTES);
                let spawned = builder.spawn_scoped(scope, || claims.work_until_empty(&work));
                // A thread the system refuses to start leaves its share to the threads
                // already working, the calling thread among them.
                let Ok(handle) = spawned else {
                    break;
                };
                handles.push(handle);
            }
            let mut finished = vec![claims.work_until_empty(&work)];
            for handle in handles {
                match handle.join() {
                    Ok(done) => finished.push(done),
                    Err(payload) => std::panic::resume_unwind(payload),
                }
            }
            finished
        });
        in_item_order(finished, items.len())
    }

    /// `passes.first` applied to every item on the pool, then `passes.second` applied to
    /// each item the first pass deferred, on at most `SECOND_PASS_WORKERS_MAX` threads; the
    /// results in the order of `items`.
    ///
    /// # Panics
    ///
    /// When either pass panics on any item.
    #[must_use]
    pub fn map_in_two_passes<T, R, First, Second>(
        self,
        items: &[T],
        passes: Passes<First, Second>,
    ) -> Vec<R>
    where
        T: Sync,
        R: Send,
        First: Fn(&T) -> Attempt<R> + Sync,
        Second: Fn(&T) -> R + Sync,
    {
        let attempted = self.map_in_order(items, passes.first);
        let mut deferred = Vec::new();
        for (index, attempt) in attempted.iter().enumerate() {
            if matches!(attempt, Attempt::Deferred) {
                deferred.push(index);
            }
        }
        let second_pool = Self::new(self.available.min(SECOND_PASS_WORKERS_MAX));
        let second_results = second_pool.map_in_order(&deferred, |index| {
            let Some(item) = items.get(*index) else {
                unreachable!("a deferred index names an item");
            };
            (passes.second)(item)
        });
        merged(attempted, second_results)
    }
}

/// Whether work on an input of `input_bytes` belongs on the second pass of
/// `Pool::map_in_two_passes`, which works few items at once, rather than on the first.
#[must_use]
pub const fn first_pass_defers(input_bytes: u64) -> bool {
    input_bytes > FIRST_PASS_INPUT_BYTES_MAX
}

/// How many threads do the work: as many as are available, up to `WORKERS_MAX`, and no
/// more than there are items, the calling thread always among them.
fn worker_count(available: NonZeroUsize, item_count: usize) -> usize {
    let bounded = available.get().min(WORKERS_MAX).min(item_count);
    let count = bounded.max(1);
    assert!(count <= WORKERS_MAX, "the cap bounds the workers");
    assert!(count <= available.get(), "the machine bounds the workers");
    count
}

/// Items handed out one at a time, each to exactly one thread.
struct Claims<'items, T> {
    items: &'items [T],
    next: AtomicUsize,
}

impl<T> Claims<'_, T> {
    /// Claims items until none is left, returning each claimed index with its result.
    fn work_until_empty<R>(&self, work: &impl Fn(&T) -> R) -> Vec<(usize, R)> {
        let mut done = Vec::new();
        for _ in 0..self.items.len() {
            // Atomicity alone keeps the claims distinct; the results reach the caller
            // through the join.
            let index = self.next.fetch_add(1, Ordering::Relaxed);
            let Some(item) = self.items.get(index) else {
                break;
            };
            done.push((index, work(item)));
        }
        done
    }
}

/// The results each thread returned, put back in the order of the items they came from.
fn in_item_order<R>(finished: Vec<Vec<(usize, R)>>, item_count: usize) -> Vec<R> {
    let mut slots: Vec<Option<R>> = Vec::with_capacity(item_count);
    slots.resize_with(item_count, || None);
    for (index, result) in finished.into_iter().flatten() {
        let Some(slot) = slots.get_mut(index) else {
            unreachable!("a thread claims only the indexes of items");
        };
        assert!(slot.is_none(), "each item is claimed once");
        *slot = Some(result);
    }
    let mut ordered = Vec::with_capacity(item_count);
    for slot in slots {
        let Some(result) = slot else {
            unreachable!("every item is claimed");
        };
        ordered.push(result);
    }
    ordered
}

/// The first pass's results with each deferral replaced by the second pass's result for
/// it, the second pass's results taken in the order of the deferrals.
fn merged<R>(attempted: Vec<Attempt<R>>, second_results: Vec<R>) -> Vec<R> {
    let mut second = second_results.into_iter();
    let mut results = Vec::with_capacity(attempted.len());
    for attempt in attempted {
        match attempt {
            Attempt::Done(result) => results.push(result),
            Attempt::Deferred => {
                let Some(result) = second.next() else {
                    unreachable!("the second pass works every deferred item");
                };
                results.push(result);
            }
        }
    }
    assert!(
        second.next().is_none(),
        "each second-pass result takes a deferred item's place"
    );
    results
}

#[cfg(test)]
mod tests;
