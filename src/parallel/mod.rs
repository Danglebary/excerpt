//! Work spread over a bounded number of threads, each result handed on in the work's
//! order once every earlier one has been.
//!
//! Nothing here does IO, so the order and the bounds on threads are tested with plain
//! values.

use std::collections::{BTreeSet, VecDeque};
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

/// The most threads that work items at once.
pub const WORKERS_MAX: usize = 16;
/// The most threads that work deferred items at once.
pub const DEFERRED_WORKERS_MAX: usize = 2;
/// The most bytes of input `attempt_defers` keeps on an attempt.
pub const ATTEMPT_INPUT_BYTES_MAX: u64 = 262_144;
/// The most items claimed from the first whose result is not yet handed on, which bounds
/// the results held waiting for their turn.
pub const CLAIMS_AHEAD_MAX: usize = 256;
/// The stack each spawned worker gets, set here rather than left to `RUST_MIN_STACK` so it
/// is the same in every environment.
const WORKER_STACK_BYTES: usize = 8_388_608;

/// The threads work is spread over: as many as the machine offers, up to `WORKERS_MAX`.
#[derive(Debug, Clone, Copy)]
pub struct Pool {
    available: NonZeroUsize,
}

/// The work of `Pool::consume_in_order`: `attempt` runs on every item and may defer it,
/// and `deferred` runs on each item `attempt` deferred.
pub struct Work<Attempted, Deferred> {
    pub attempt: Attempted,
    pub deferred: Deferred,
}

/// What an attempt made of one item: its result, or a deferral to the deferred work.
pub enum Attempt<R> {
    Done(R),
    Deferred,
}

impl Pool {
    /// A pool of up to `available` threads.
    #[must_use]
    pub const fn new(available: NonZeroUsize) -> Self {
        Self { available }
    }

    /// Works every item and hands each result to `consume`, with its item, in the order of
    /// `items`, until `consume` breaks; no item is claimed after it does.
    ///
    /// The threads, no more than the pool's, `WORKERS_MAX`, or the number of items, each
    /// claim the next unclaimed item so long as fewer than `CLAIMS_AHEAD_MAX` claimed items
    /// wait to be handed on; the first `DEFERRED_WORKERS_MAX` threads alone work deferred
    /// items, and claim them first. Work for one thread runs on the calling thread alone;
    /// otherwise the calling thread only hands results on.
    ///
    /// # Panics
    ///
    /// When `work` or `consume` panics on any item.
    pub fn consume_in_order<'items, T, R, Attempted, Deferred, Consume>(
        self,
        items: &'items [T],
        work: &Work<Attempted, Deferred>,
        consume: Consume,
    ) where
        T: Sync,
        R: Send,
        Attempted: Fn(&T) -> Attempt<R> + Sync,
        Deferred: Fn(&T) -> R + Sync,
        Consume: FnMut(&'items T, R) -> ControlFlow<()>,
    {
        let workers = worker_count(self.available, items.len());
        if workers == 1 {
            consume_alone(items, work, consume);
            return;
        }
        let shared = Shared::new(items.len());
        let shared_by_workers = &shared;
        thread::scope(|scope| {
            let mut handles = Vec::new();
            for index in 0..workers {
                let worker = Worker::of_index(index);
                let builder = thread::Builder::new().stack_size(WORKER_STACK_BYTES);
                let spawned = builder
                    .spawn_scoped(scope, move || shared_by_workers.work(items, work, worker));
                // A thread the system refuses to start leaves its share to the threads
                // already working.
                let Ok(handle) = spawned else {
                    break;
                };
                handles.push(handle);
            }
            if handles.is_empty() {
                consume_alone(items, work, consume);
            } else {
                shared.consume(items, consume);
            }
            for handle in handles {
                if let Err(payload) = handle.join() {
                    std::panic::resume_unwind(payload);
                }
            }
        });
    }
}

/// Whether work on an input of `input_bytes` belongs on the deferred work of
/// `Pool::consume_in_order`, which works few items at once, rather than on an attempt.
#[must_use]
pub const fn attempt_defers(input_bytes: u64) -> bool {
    input_bytes > ATTEMPT_INPUT_BYTES_MAX
}

/// How many threads do the work: as many as are available, up to `WORKERS_MAX`, and no
/// more than there are items, one at the least.
fn worker_count(available: NonZeroUsize, item_count: usize) -> usize {
    let bounded = available.get().min(WORKERS_MAX).min(item_count);
    let count = bounded.max(1);
    assert!(count <= WORKERS_MAX, "the cap bounds the workers");
    assert!(count <= available.get(), "the machine bounds the workers");
    count
}

/// Every item worked and handed on in order on the calling thread.
fn consume_alone<'items, T, R, Attempted, Deferred>(
    items: &'items [T],
    work: &Work<Attempted, Deferred>,
    mut consume: impl FnMut(&'items T, R) -> ControlFlow<()>,
) where
    Attempted: Fn(&T) -> Attempt<R>,
    Deferred: Fn(&T) -> R,
{
    for item in items {
        let result = match (work.attempt)(item) {
            Attempt::Done(result) => result,
            Attempt::Deferred => (work.deferred)(item),
        };
        if consume(item, result).is_break() {
            return;
        }
    }
}

/// The next thing a worker does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    Attempt(usize),
    Deferred(usize),
    Finished,
}

/// Which items a worker claims. The memory a parse frees stays with the thread's
/// allocator for its next parse, so a large input's work is kept to the few deferring
/// workers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Worker {
    /// Claims deferred items first, then attempts.
    Deferring,
    /// Claims attempts alone.
    Attempting,
}

impl Worker {
    /// The first `DEFERRED_WORKERS_MAX` workers defer, and the rest attempt.
    const fn of_index(index: usize) -> Self {
        if index < DEFERRED_WORKERS_MAX {
            Self::Deferring
        } else {
            Self::Attempting
        }
    }
}

/// The claims and results the threads of one `Pool::consume_in_order` share, and the
/// signal that they changed.
struct Shared<R> {
    state: Mutex<State<R>>,
    changed: Condvar,
}

impl<R> Shared<R> {
    fn new(item_count: usize) -> Self {
        Self {
            state: Mutex::new(State::new(item_count)),
            changed: Condvar::new(),
        }
    }

    /// The state, as it stands even where a thread panicked holding it; that thread's
    /// panic ends the consumption when it is joined.
    fn locked(&self) -> MutexGuard<'_, State<R>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The state once `condition` holds of it, waiting for each change until it does.
    fn locked_once(&self, condition: impl Fn(&State<R>) -> bool) -> MutexGuard<'_, State<R>> {
        let locked = self.locked();
        let waited = self.changed.wait_while(locked, |state| !condition(state));
        waited.unwrap_or_else(PoisonError::into_inner)
    }

    /// Claims and works items until none is left to claim or the consumer has stopped.
    fn work<T, Attempted, Deferred>(
        &self,
        items: &[T],
        work: &Work<Attempted, Deferred>,
        worker: Worker,
    ) where
        Attempted: Fn(&T) -> Attempt<R>,
        Deferred: Fn(&T) -> R,
    {
        let _stopping = StoppingOnPanic(self);
        // Each item is claimed once to attempt and at most once more to defer, and the
        // last claim finishes.
        let Some(claims_max) = items
            .len()
            .checked_mul(2)
            .and_then(|claims| claims.checked_add(1))
        else {
            unreachable!("twice the items fits in a usize");
        };
        for _ in 0..claims_max {
            match self.claim(worker) {
                Claim::Attempt(index) => {
                    let attempt = (work.attempt)(item_at(items, index));
                    self.attempted(index, attempt);
                }
                Claim::Deferred(index) => {
                    let result = (work.deferred)(item_at(items, index));
                    self.finished_deferred(index, result);
                }
                Claim::Finished => return,
            }
        }
        unreachable!("a thread claims each item at most twice before it finishes");
    }

    /// The worker's next claim, waiting until there is one, taken.
    fn claim(&self, worker: Worker) -> Claim {
        let mut state = self.locked_once(|state| state.next_claim(worker).is_some());
        let Some(claim) = state.next_claim(worker) else {
            unreachable!("the wait ends on a claim");
        };
        state.take(claim);
        claim
    }

    fn attempted(&self, index: usize, attempt: Attempt<R>) {
        let mut state = self.locked();
        state.finish_attempt(index, attempt);
        drop(state);
        self.changed.notify_all();
    }

    fn finished_deferred(&self, index: usize, result: R) {
        let mut state = self.locked();
        state.fill(index, result);
        drop(state);
        self.changed.notify_all();
    }

    /// Hands each result to `consume` in item order until it breaks, every item has been
    /// handed on, or a worker has panicked.
    fn consume<'items, T>(
        &self,
        items: &'items [T],
        mut consume: impl FnMut(&'items T, R) -> ControlFlow<()>,
    ) {
        let _stopping = Stopping(self);
        for (expected, item) in items.iter().enumerate() {
            let Some((index, result)) = self.next_result() else {
                return;
            };
            assert_eq!(index, expected, "results are handed on in item order");
            if consume(item, result).is_break() {
                return;
            }
        }
    }

    /// The next result in item order with its index, waiting until it is ready, or nothing
    /// where the consumption has stopped.
    fn next_result(&self) -> Option<(usize, R)> {
        let mut state = self.locked_once(|state| state.stopped || state.front_ready());
        if state.stopped {
            return None;
        }
        let taken = state.take_front();
        drop(state);
        self.changed.notify_all();
        Some(taken)
    }

    fn stop(&self) {
        let mut state = self.locked();
        state.stopped = true;
        drop(state);
        self.changed.notify_all();
    }
}

/// Stops the consumption when dropped, as the consumer leaves for any reason, so every
/// worker finishes rather than waiting for room to claim.
struct Stopping<'shared, R>(&'shared Shared<R>);

impl<R> Drop for Stopping<'_, R> {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Stops the consumption when dropped by a panicking worker, so the consumer does not wait
/// for a result that will never come.
struct StoppingOnPanic<'shared, R>(&'shared Shared<R>);

impl<R> Drop for StoppingOnPanic<'_, R> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.stop();
        }
    }
}

/// Which items are claimed, which wait for a deferring worker, and the results not yet
/// handed on.
struct State<R> {
    /// The index of the next result to hand on.
    head: usize,
    /// The result of each claimed item from `head` on, `None` while it is worked; the next
    /// unclaimed item is the one after the last.
    held: VecDeque<Option<R>>,
    item_count: usize,
    /// The deferred items no worker has claimed.
    deferred: BTreeSet<usize>,
    /// How many attempts are being worked, each of which may yet defer its item.
    attempts_running: usize,
    /// Whether the consumer has left, after which nothing is claimed.
    stopped: bool,
}

impl<R> State<R> {
    fn new(item_count: usize) -> Self {
        Self {
            head: 0,
            held: VecDeque::with_capacity(CLAIMS_AHEAD_MAX.min(item_count)),
            item_count,
            deferred: BTreeSet::new(),
            attempts_running: 0,
            stopped: false,
        }
    }

    /// What `worker` does next, or nothing while it must wait: for a deferring worker the
    /// lowest deferred item, since the earliest results hold up the rest, and then for
    /// either the next unclaimed item while there is room to hold its result.
    fn next_claim(&self, worker: Worker) -> Option<Claim> {
        if self.stopped {
            return Some(Claim::Finished);
        }
        if worker == Worker::Deferring {
            if let Some(&index) = self.deferred.first() {
                return Some(Claim::Deferred(index));
            }
        }
        let unclaimed = add(self.head, self.held.len());
        if unclaimed < self.item_count {
            if self.held.len() < CLAIMS_AHEAD_MAX {
                return Some(Claim::Attempt(unclaimed));
            }
            return None;
        }
        // Every item is claimed. A deferring worker stays while an item waits to be
        // deferred or an attempt may yet defer one.
        match worker {
            Worker::Attempting => Some(Claim::Finished),
            Worker::Deferring => {
                if !self.deferred.is_empty() {
                    return None;
                }
                if self.attempts_running > 0 {
                    return None;
                }
                Some(Claim::Finished)
            }
        }
    }

    fn take(&mut self, claim: Claim) {
        match claim {
            Claim::Attempt(index) => {
                assert_eq!(
                    index,
                    add(self.head, self.held.len()),
                    "an attempt claims the next unclaimed item"
                );
                assert!(
                    self.held.len() < CLAIMS_AHEAD_MAX,
                    "the claims ahead are bounded"
                );
                self.held.push_back(None);
                self.attempts_running = add(self.attempts_running, 1);
            }
            Claim::Deferred(index) => {
                let removed = self.deferred.remove(&index);
                assert!(removed, "a deferred claim takes a deferred item");
            }
            Claim::Finished => {}
        }
    }

    fn finish_attempt(&mut self, index: usize, attempt: Attempt<R>) {
        let Some(running) = self.attempts_running.checked_sub(1) else {
            unreachable!("an attempt finishes after it is claimed");
        };
        self.attempts_running = running;
        match attempt {
            Attempt::Done(result) => self.fill(index, result),
            Attempt::Deferred => {
                assert!(self.slot(index).is_none(), "a deferred item has no result");
                let inserted = self.deferred.insert(index);
                assert!(inserted, "an item is deferred once");
            }
        }
    }

    fn fill(&mut self, index: usize, result: R) {
        let slot = self.slot(index);
        assert!(slot.is_none(), "an item is worked once");
        *slot = Some(result);
    }

    /// The place held for a claimed item's result.
    fn slot(&mut self, index: usize) -> &mut Option<R> {
        let Some(offset) = index.checked_sub(self.head) else {
            unreachable!("an item is claimed after its result is handed on");
        };
        let Some(slot) = self.held.get_mut(offset) else {
            unreachable!("an item is worked once it is claimed");
        };
        slot
    }

    fn front_ready(&self) -> bool {
        matches!(self.held.front(), Some(Some(_)))
    }

    /// The next result in item order with its index, which must be ready.
    fn take_front(&mut self) -> (usize, R) {
        let Some(Some(result)) = self.held.pop_front() else {
            unreachable!("the next result is taken once it is ready");
        };
        let index = self.head;
        self.head = add(self.head, 1);
        (index, result)
    }
}

fn item_at<T>(items: &[T], index: usize) -> &T {
    let Some(item) = items.get(index) else {
        unreachable!("a claim names an item");
    };
    item
}

fn add(left: usize, right: usize) -> usize {
    let Some(sum) = left.checked_add(right) else {
        unreachable!("a count of items fits in a usize");
    };
    sum
}

#[cfg(test)]
mod tests;
