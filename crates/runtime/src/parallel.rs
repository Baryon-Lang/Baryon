//! The thread pool that runs `prange` loops, and the fixed chunks that keep their results
//! reproducible.
//!
//! A `prange` loop of `n` iterations is split into chunks, contiguous runs of iterations whose
//! boundaries depend only on `n` ([`chunk_count`], [`chunk`]). Chunks never depend on the number
//! of threads, which is what makes a parallel floating-point sum give the same bits on every
//! machine (the reference, "Reproducible reductions"): each chunk reduces its own iterations in
//! order, and compiled code combines the chunk results in a fixed order after the loop.
//!
//! The pool has one worker thread per core but one, since the thread that runs the loop works
//! too. It starts on the first loop with more than one chunk, so a program with no `prange`
//! starts no threads. Threads take chunks from a shared counter until none are left, which
//! balances uneven chunks without moving any boundary. Correctness never depends on the
//! workers: if the system refuses to start them, or there is one core, the calling thread runs
//! every chunk itself, in the same chunks.

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};

/// The most chunks a loop is split into.
///
/// Enough that threads stay busy when chunks take different times: eight chunks per thread on a
/// 128-core machine. Few enough that taking a chunk, one atomic addition and one call, costs
/// nothing measurable, and that a reduction's chunk results fit in 8 KiB. Changing it changes the
/// last bits of parallel floating-point reductions, so it changes only with a benchmark that
/// justifies it (roadmap 0.3).
pub(crate) const MAX_CHUNKS: i64 = 1024;

/// The compiled body of a `prange` loop, called once per chunk.
///
/// It receives the context pointer given to [`baryon_rt_parallel_for`](crate::baryon_rt_parallel_for),
/// the chunk's index, and the chunk's iterations as the half-open range `start..end`, counted
/// from 0. It runs those iterations in order. The compiler outlines the loop body into this
/// function (roadmap 1.9.7) and passes the loop's shared variables through the context.
pub type ChunkBody = unsafe extern "C" fn(context: *mut c_void, chunk: i64, start: i64, end: i64);

/// How many chunks a loop of `iterations` iterations is split into: one per iteration up to
/// [`MAX_CHUNKS`], and none for an empty or negative count.
pub(crate) fn chunk_count(iterations: i64) -> i64 {
    iterations.clamp(0, MAX_CHUNKS)
}

/// One chunk of a loop: its position and the iterations it covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Chunk {
    /// The chunk's index, from 0 to the chunk count.
    pub(crate) index: i64,
    /// The first iteration in the chunk.
    pub(crate) start: i64,
    /// One past the last iteration in the chunk.
    pub(crate) end: i64,
}

/// Chunk `index` of a loop of `iterations` iterations, where `index` is below
/// [`chunk_count`]`(iterations)`.
///
/// The iterations are split as evenly as integers allow: every chunk has `iterations / count`
/// iterations, and the first `iterations % count` chunks have one more. No arithmetic can
/// overflow, since every value computed is at most `iterations`.
pub(crate) fn chunk(iterations: i64, index: i64) -> Chunk {
    let count = chunk_count(iterations);
    let quotient = iterations / count;
    let remainder = iterations % count;
    let start = index * quotient + index.min(remainder);
    let end = start + quotient + i64::from(index < remainder);
    Chunk { index, start, end }
}

/// A loop to run: the outlined body, its context and its iteration count.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Task {
    /// The compiled body, called once per chunk.
    pub(crate) body: ChunkBody,
    /// The pointer passed to every call of `body`.
    pub(crate) context: *mut c_void,
    /// How many iterations the loop has.
    pub(crate) iterations: i64,
}

#[allow(
    unsafe_code,
    reason = "the raw context pointer makes `Task` `!Send` by default"
)]
// SAFETY: a `Task` is two pointers and a count. The runtime never reads through either pointer;
// it only passes them to `body`, whose caller promised that calling it from any thread, for
// different chunks at once, is sound (`baryon_rt_parallel_for`'s `# Safety`).
unsafe impl Send for Task {}

#[allow(
    unsafe_code,
    reason = "the raw context pointer makes `Task` `!Sync` by default"
)]
// SAFETY: as for `Send`: shared access only copies the pointers out, it never reads through them.
unsafe impl Sync for Task {}

/// One loop being run: its task, the next chunk to take, and how many chunks are finished.
struct Job {
    /// The loop.
    task: Task,
    /// How many chunks the loop has.
    chunks: i64,
    /// The index of the next chunk no thread has taken yet; at or past `chunks` when none is
    /// left.
    next: AtomicI64,
    /// How many chunks have finished running.
    finished: Mutex<i64>,
    /// Signalled when `finished` reaches `chunks`.
    all_finished: Condvar,
}

impl Job {
    /// A job for `task`, with no chunk taken yet.
    fn new(task: Task) -> Self {
        Self {
            task,
            chunks: chunk_count(task.iterations),
            next: AtomicI64::new(0),
            finished: Mutex::new(0),
            all_finished: Condvar::new(),
        }
    }

    /// Takes chunks and runs them until none is left, then records how many this thread ran.
    ///
    /// # Safety
    ///
    /// Calling the task's body for any of its chunks, on this thread, while other threads run
    /// other chunks, is sound: the promise `baryon_rt_parallel_for`'s caller makes.
    #[allow(unsafe_code, reason = "calls the compiled loop body")]
    unsafe fn work(&self) {
        let mut ran = 0;
        loop {
            // `Relaxed` is enough: the counter only hands out distinct indices. Writes the
            // body makes are published by the `finished` mutex below.
            let index = self.next.fetch_add(1, Ordering::Relaxed);
            if index >= self.chunks {
                break;
            }
            let Chunk { start, end, .. } = chunk(self.task.iterations, index);
            // SAFETY: `index` is a chunk of this task that no other thread took, and the caller
            // guarantees the body may run it here.
            unsafe { (self.task.body)(self.task.context, index, start, end) };
            ran += 1;
        }
        if ran > 0 {
            let mut finished = lock(&self.finished);
            *finished += ran;
            if *finished == self.chunks {
                self.all_finished.notify_all();
            }
        }
    }

    /// Blocks until every chunk has finished. The mutex makes every write the body made on any
    /// thread visible to the caller once this returns.
    fn wait(&self) {
        let mut finished = lock(&self.finished);
        while *finished < self.chunks {
            finished = self
                .all_finished
                .wait(finished)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// What the workers wait on: the current job and a count of jobs published so far.
struct Board {
    /// The job being run, if any.
    job: Option<Arc<Job>>,
    /// Increases by one each time a job is published, so a worker can tell a new job from one it
    /// has already worked on.
    generation: u64,
}

/// The state the pool's threads share.
struct Shared {
    /// The current job.
    board: Mutex<Board>,
    /// Signalled when a new job is published.
    published: Condvar,
}

/// The worker threads and the lock that lets one loop run at a time.
struct Pool {
    /// Held while a loop runs, so two threads that start loops at once take turns.
    running: Mutex<()>,
    /// The state the workers share.
    shared: Arc<Shared>,
}

/// The pool, started by the first loop that needs it: `None` when it would have no workers.
///
/// This is the runtime's one piece of global state, allowed by decision 0021: a program has one
/// set of cores, and a pool per loop would start threads on every loop.
static POOL: OnceLock<Option<Pool>> = OnceLock::new();

thread_local! {
    /// Whether this thread is running a chunk. A loop started inside a chunk runs on its own
    /// thread rather than waiting for the pool it is part of. The compiler rejects a `prange`
    /// inside a `prange`, so only a compiler bug gets here, and it gets a slow loop instead of a
    /// hang.
    static IN_CHUNK: Cell<bool> = const { Cell::new(false) };

    /// Whether this thread is running a parallel loop's body: a worker running chunks, or the
    /// thread that started the loop, whether the pool helps it or not.
    static IN_LOOP: Cell<bool> = const { Cell::new(false) };
}

/// Whether this thread is inside the body of a parallel loop. `sys.exit` asks, since it writes
/// out every open file and other iterations may be writing theirs at the same moment
/// (roadmap 1.13.17).
pub(crate) fn in_loop() -> bool {
    IN_LOOP.get()
}

impl Pool {
    /// Starts one worker per core but one. Returns `None` on a single core, or when the system
    /// refuses every thread, since the caller then runs every chunk anyway.
    fn start() -> Option<Self> {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        let shared = Arc::new(Shared {
            board: Mutex::new(Board {
                job: None,
                generation: 0,
            }),
            published: Condvar::new(),
        });
        let mut started = 0;
        for number in 1..cores {
            let shared = Arc::clone(&shared);
            let thread = std::thread::Builder::new()
                .name(format!("baryon-worker-{number}"))
                .spawn(move || work_forever(&shared));
            started += usize::from(thread.is_ok());
        }
        (started > 0).then(|| Self {
            running: Mutex::new(()),
            shared,
        })
    }

    /// Runs `job` on the workers and the calling thread, and returns once every chunk finished.
    ///
    /// # Safety
    ///
    /// As for [`Job::work`].
    #[allow(unsafe_code, reason = "runs the compiled loop body")]
    unsafe fn run(&self, job: &Arc<Job>) {
        let _running = lock(&self.running);
        {
            let mut board = lock(&self.shared.board);
            board.job = Some(Arc::clone(job));
            board.generation += 1;
        }
        self.shared.published.notify_all();
        // SAFETY: the caller's promise, passed on.
        unsafe { run_chunks(job) };
        job.wait();
        // A worker that wakes late finds no job instead of this one's empty counter.
        lock(&self.shared.board).job = None;
    }
}

/// A worker's whole life: wait for a job it has not seen, work on it, repeat.
fn work_forever(shared: &Shared) {
    let mut seen = 0;
    loop {
        let job = {
            let mut board = lock(&shared.board);
            while board.generation == seen {
                board = shared
                    .published
                    .wait(board)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            seen = board.generation;
            board.job.clone()
        };
        if let Some(job) = job {
            // SAFETY: the job was published by `Pool::run`, whose caller promised that its body
            // may run on any thread.
            #[allow(unsafe_code, reason = "runs the compiled loop body")]
            unsafe {
                run_chunks(&job);
            }
        }
    }
}

/// Runs chunks of `job` on this thread, marked as inside a chunk while it does.
///
/// # Safety
///
/// As for [`Job::work`].
#[allow(unsafe_code, reason = "runs the compiled loop body")]
unsafe fn run_chunks(job: &Job) {
    IN_CHUNK.set(true);
    let outer = IN_LOOP.replace(true);
    // SAFETY: the caller's promise, passed on.
    unsafe { job.work() };
    IN_LOOP.set(outer);
    IN_CHUNK.set(false);
}

/// Runs every chunk of `task`, on the pool when there is more than one chunk, and returns once
/// all have finished.
///
/// # Safety
///
/// Calling `task.body` with `task.context` for each chunk, from any thread, with different
/// chunks running at the same time, is sound.
#[allow(unsafe_code, reason = "runs the compiled loop body")]
pub(crate) unsafe fn parallel_for(task: Task) {
    let job = Arc::new(Job::new(task));
    let pool = if job.chunks > 1 && !IN_CHUNK.get() {
        POOL.get_or_init(Pool::start).as_ref()
    } else {
        None
    };
    let outer = IN_LOOP.replace(true);
    match pool {
        // SAFETY: the caller's promise, passed on.
        Some(pool) => unsafe { pool.run(&job) },
        // SAFETY: the caller's promise, passed on; here every chunk runs on this thread.
        None => unsafe { job.work() },
    }
    IN_LOOP.set(outer);
}

/// Locks `mutex`, ignoring poisoning: a thread can only panic while holding one of the pool's
/// locks through a runtime bug, and the counts they guard stay consistent even then.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[allow(unsafe_code, reason = "the tests call the pool as compiled code does")]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// Runs `body` over `iterations` iterations on the pool, as compiled code would, with the
    /// closure as the context.
    fn run<F: Fn(Chunk) + Sync>(iterations: i64, body: &F) {
        /// Calls the closure the context points to.
        unsafe extern "C" fn trampoline<F: Fn(Chunk) + Sync>(
            context: *mut c_void,
            index: i64,
            start: i64,
            end: i64,
        ) {
            // SAFETY: `context` is the `&F` passed below, alive until `parallel_for` returns.
            let body = unsafe { &*context.cast::<F>() };
            body(Chunk { index, start, end });
        }
        let task = Task {
            body: trampoline::<F>,
            context: std::ptr::from_ref(body).cast_mut().cast(),
            iterations,
        };
        // SAFETY: `F` is `Sync`, so calling it from several threads at once is sound.
        unsafe { parallel_for(task) };
    }

    #[test]
    fn chunks_cover_every_iteration_once_in_order() {
        for iterations in [1, 2, 7, 1023, 1024, 1025, 4097, 1_000_000] {
            let count = chunk_count(iterations);
            let mut expected_start = 0;
            for index in 0..count {
                let chunk = chunk(iterations, index);
                assert_eq!(chunk.start, expected_start);
                assert!(chunk.end > chunk.start);
                expected_start = chunk.end;
            }
            assert_eq!(expected_start, iterations);
        }
    }

    #[test]
    fn chunks_differ_in_size_by_at_most_one() {
        let sizes: Vec<i64> = (0..chunk_count(4097))
            .map(|index| {
                let chunk = chunk(4097, index);
                chunk.end - chunk.start
            })
            .collect();
        assert_eq!(sizes.first(), Some(&5));
        assert_eq!(sizes.last(), Some(&4));
        assert_eq!(sizes.iter().sum::<i64>(), 4097);
    }

    #[test]
    fn an_empty_or_negative_loop_has_no_chunks() {
        assert_eq!(chunk_count(0), 0);
        assert_eq!(chunk_count(-5), 0);
        assert_eq!(chunk_count(i64::MAX), MAX_CHUNKS);
        run(0, &|_| unreachable!());
        run(-5, &|_| unreachable!());
    }

    #[test]
    fn runs_every_iteration_exactly_once() {
        for iterations in [1, 3, 1024, 100_000] {
            let visits: Vec<AtomicU32> = (0..iterations).map(|_| AtomicU32::new(0)).collect();
            run(iterations, &|chunk: Chunk| {
                for iteration in chunk.start..chunk.end {
                    visits[usize::try_from(iteration).unwrap()].fetch_add(1, Ordering::Relaxed);
                }
            });
            assert!(
                visits
                    .iter()
                    .all(|count| count.load(Ordering::Relaxed) == 1)
            );
        }
    }

    #[test]
    fn a_float_reduction_gives_the_same_bits_every_run() {
        let iterations = 1_000_003;
        let sum = || {
            let partials: Vec<Mutex<f64>> = (0..chunk_count(iterations))
                .map(|_| Mutex::new(0.0))
                .collect();
            run(iterations, &|chunk: Chunk| {
                let mut partial = 0.0;
                for iteration in chunk.start..chunk.end {
                    partial += 1.0 / f64::from(u32::try_from(iteration).unwrap() + 1);
                }
                *lock(&partials[usize::try_from(chunk.index).unwrap()]) = partial;
            });
            partials.iter().map(|partial| *lock(partial)).sum::<f64>()
        };
        let first = sum();
        for _ in 0..20 {
            assert_eq!(sum().to_bits(), first.to_bits());
        }
    }

    #[test]
    fn a_loop_inside_a_chunk_runs_on_its_thread() {
        let inner = AtomicU32::new(0);
        run(8, &|_| {
            run(4, &|chunk: Chunk| {
                let count = u32::try_from(chunk.end - chunk.start).unwrap();
                inner.fetch_add(count, Ordering::Relaxed);
            });
        });
        assert_eq!(inner.load(Ordering::Relaxed), 32);
    }
}
