//! Minimal bounded-concurrency helpers for batch preparation and encoding.
//!
//! Each stage runs at most `jobs` tasks at once, never more than one worker per
//! task. Results are returned in index (plan) order regardless of completion
//! order, so summaries and stdout reporting stay deterministic. No async
//! runtime, thread pool crate, or other new dependency is involved.
//!
//! Event callbacks always run on the calling thread. Workers only send
//! messages; the caller observes them in arrival order and updates progress
//! state itself, so progress output is never interleaved.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

/// Effective worker count for `tasks` work items under a `jobs` limit.
///
/// Non-empty work always gets at least one worker and never more than
/// `tasks`; an empty task list returns one, and callers short-circuit it.
pub(crate) fn worker_count(jobs: usize, tasks: usize) -> usize {
    jobs.max(1).min(tasks.max(1))
}

/// Event delivered to the calling thread while an indexed map runs.
pub(crate) enum IndexEvent<'a, T> {
    /// Work for this index is about to start.
    Started(usize),
    /// Work for this index finished. The value is borrowed only for this
    /// callback and is stored afterwards.
    Finished(usize, &'a T),
}

/// Run `task(index)` for every index in `0..count` on at most `jobs` workers,
/// returning results in index order.
pub(crate) fn map_indexed<T, F>(count: usize, jobs: usize, task: F) -> Vec<T>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    map_indexed_with_events(count, jobs, |_| false, task, |_| {})
}

/// Run `task(index)` for every index in `0..count` on at most `jobs` workers.
///
/// `should_start(index)` is evaluated before each task; when it returns `true`,
/// [`IndexEvent::Started`] is delivered to `on_event` on the calling thread.
/// A task result is delivered as [`IndexEvent::Finished`] before it is stored.
/// Results are returned in index order.
pub(crate) fn map_indexed_with_events<T, S, F, G>(
    count: usize,
    jobs: usize,
    should_start: S,
    task: F,
    mut on_event: G,
) -> Vec<T>
where
    T: Send,
    S: Fn(usize) -> bool + Sync,
    F: Fn(usize) -> T + Sync,
    G: FnMut(IndexEvent<'_, T>),
{
    if count == 0 {
        return Vec::new();
    }
    let workers = worker_count(jobs, count);
    if workers == 1 {
        let mut values = Vec::with_capacity(count);
        for index in 0..count {
            if should_start(index) {
                on_event(IndexEvent::Started(index));
            }
            let value = task(index);
            on_event(IndexEvent::Finished(index, &value));
            values.push(value);
        }
        return values;
    }

    enum Message<T> {
        Started(usize),
        Finished(usize, T),
    }

    let next = AtomicUsize::new(0);
    let mut slots: Vec<Option<T>> = (0..count).map(|_| None).collect();
    thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel::<Message<T>>();
        for _ in 0..workers {
            let sender = sender.clone();
            let next = &next;
            let should_start = &should_start;
            let task = &task;
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= count {
                    break;
                }
                if should_start(index) && sender.send(Message::Started(index)).is_err() {
                    break;
                }
                let value = task(index);
                if sender.send(Message::Finished(index, value)).is_err() {
                    break;
                }
            });
        }
        drop(sender);
        while let Ok(message) = receiver.recv() {
            match message {
                Message::Started(index) => on_event(IndexEvent::Started(index)),
                Message::Finished(index, value) => {
                    on_event(IndexEvent::Finished(index, &value));
                    slots[index] = Some(value);
                }
            }
        }
    });
    slots
        .into_iter()
        .map(|slot| slot.expect("every worker task sends exactly one result"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn worker_count_is_bounded_and_nonzero() {
        assert_eq!(worker_count(0, 0), 1);
        assert_eq!(worker_count(0, 5), 1);
        assert_eq!(worker_count(1, 5), 1);
        assert_eq!(worker_count(8, 3), 3);
        assert_eq!(worker_count(8, 100), 8);
    }

    #[test]
    fn indexed_map_preserves_input_order() {
        let values = map_indexed(10, 4, |index| index * 3);
        assert_eq!(values, (0..10).map(|index| index * 3).collect::<Vec<_>>());
        assert!(map_indexed(0, 4, |index| index).is_empty());
    }

    #[test]
    fn start_callbacks_follow_index_order_with_one_worker() {
        let mut events = Vec::new();
        let values = map_indexed_with_events(
            3,
            1,
            |index| index != 1,
            |index| index,
            |event| match event {
                IndexEvent::Started(index) => events.push(("start", index)),
                IndexEvent::Finished(index, _) => events.push(("finish", index)),
            },
        );
        assert_eq!(values, vec![0, 1, 2]);
        assert_eq!(
            events,
            vec![
                ("start", 0),
                ("finish", 0),
                ("finish", 1),
                ("start", 2),
                ("finish", 2),
            ]
        );
    }

    #[test]
    fn event_callbacks_report_concurrent_starts() {
        // Same rendezvous as above, but through the event-enabled entry point
        // used by batch encoding: all start callbacks must fire before any
        // worker is released.
        const WORKERS: usize = 3;
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel::<()>();
        let release_receiver = Mutex::new(release_receiver);
        let handle = thread::spawn(move || {
            map_indexed_with_events(
                WORKERS,
                WORKERS,
                |_| true,
                |index| {
                    release_receiver.lock().unwrap().recv().unwrap();
                    index
                },
                |event| {
                    if let IndexEvent::Started(index) = event {
                        started_sender.send(index).unwrap();
                    }
                },
            )
        });
        for _ in 0..WORKERS {
            started_receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("not all event-enabled workers started concurrently");
        }
        for _ in 0..WORKERS {
            release_sender.send(()).unwrap();
        }
        let values = handle.join().unwrap();
        assert_eq!(values, vec![0, 1, 2]);
    }

    #[test]
    fn requested_workers_run_at_the_same_time() {
        // Deterministic rendezvous, not a timing assertion: every worker must
        // announce itself before any of them is released. A pool that runs
        // fewer tasks concurrently never produces all start messages and the
        // test fails on the receive timeout instead of hanging.
        const WORKERS: usize = 3;
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel::<()>();
        let release_receiver = Mutex::new(release_receiver);
        let handle = thread::spawn(move || {
            map_indexed(WORKERS, WORKERS, |index| {
                started_sender.send(index).unwrap();
                release_receiver.lock().unwrap().recv().unwrap();
                index
            })
        });
        for _ in 0..WORKERS {
            started_receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("not all requested workers started concurrently");
        }
        for _ in 0..WORKERS {
            release_sender.send(()).unwrap();
        }
        let values = handle.join().unwrap();
        assert_eq!(values, vec![0, 1, 2]);
    }
}
