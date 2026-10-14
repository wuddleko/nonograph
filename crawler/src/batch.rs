use std::io;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use crate::{save_crawl, Pool, Queue, Visit, VisitHook};

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

pub fn run_batch<H, F>(
    queue: &mut Queue,
    workers: usize,
    now: i64,
    make_hook: F,
    path: &Path,
) -> io::Result<()>
where
    H: VisitHook,
    F: Fn() -> H + Sync,
{
    let shared = Arc::new(Shared {
        queue: Mutex::new(std::mem::replace(
            queue,
            Queue::new(Pool { rows: Vec::new() }),
        )),
        wake: Condvar::new(),
    });

    let run = catch_unwind(AssertUnwindSafe(|| {
        thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    let shared = Arc::clone(&shared);
                    let mut hook = make_hook();
                    run_worker(&shared, &mut hook, now);
                });
            }
        });
    }));

    let shared = Arc::try_unwrap(shared).unwrap_or_else(|_| {
        panic!("workers still hold the queue");
    });
    *queue = shared
        .queue
        .into_inner()
        .unwrap_or_else(|poison| poison.into_inner());

    match run {
        Ok(()) => save_crawl(path, queue.pool_mut()),
        Err(panic) => resume_unwind(panic),
    }
}

fn run_worker<H: VisitHook>(shared: &Shared, hook: &mut H, now: i64) {
    loop {
        let url = {
            let mut queue = shared.queue.lock().expect("queue lock");
            loop {
                if let Some(url) = queue.pop() {
                    break url;
                }
                if queue.idle() {
                    shared.wake.notify_all();
                    return;
                }
                queue = shared.wake.wait(queue).expect("queue lock");
            }
        };

        let visited = catch_unwind(AssertUnwindSafe(|| hook.visit(&url, now)));
        let panic = match visited {
            Ok((visit, discovered)) => {
                commit(shared, &url, visit, discovered, now);
                None
            }
            Err(panic) => {
                commit(shared, &url, missed(now), Vec::<String>::new(), now);
                Some(panic)
            }
        };
        if let Some(panic) = panic {
            resume_unwind(panic);
        }
    }
}

fn commit(
    shared: &Shared,
    url: &str,
    visit: Visit,
    discovered: impl IntoIterator<Item = impl AsRef<str>>,
    now: i64,
) {
    let mut queue = shared.queue.lock().expect("queue lock");
    queue.finish(url, visit, discovered, now);
    shared.wake.notify_all();
}

fn missed(now: i64) -> Visit {
    Visit {
        at: now,
        socket_answered: false,
        nip11: None,
        probes: Vec::new(),
    }
}
