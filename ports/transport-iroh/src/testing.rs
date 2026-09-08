//! A cross-process lock, so that only one test at a time forms a gossip swarm.
//!
//! # Why this exists, with the evidence
//!
//! Swarm-forming tests are not unit tests. Each one binds real UDP sockets,
//! discovers local addresses, and runs a QUIC handshake and a membership
//! protocol against another endpoint on the same host. `cargo test --workspace`
//! runs test BINARIES in parallel, and two of them — `transport-iroh` and
//! `croft-ffi` — each start several endpoints, so a full gate run has roughly
//! twenty live endpoints across several processes competing at once.
//!
//! Under that load swarm formation does not merely slow down, it stalls past
//! sixty seconds. Measured, three separate occurrences:
//!
//! - alone, the transport suite forms every swarm in 500–2500ms and passes four
//!   runs running;
//! - two standalone probes, six transports sequential and six concurrent, form
//!   in 500–2500ms;
//! - during `make gate`, one test burned the full sixty-second patience.
//!
//! Two earlier explanations were tested and refuted rather than assumed: runtime
//! oversubscription (pinning `worker_threads(2)` did not fix it) and plain
//! machine load (the suite passes on a loaded box when run alone). What
//! survives is the count of concurrently live endpoints, and no per-binary
//! setting can help with that — `--test-threads=1` governs one process, and the
//! collision is BETWEEN processes.
//!
//! So the tests serialize themselves against each other through the filesystem.
//! This is a test-harness concern and lives behind a clearly-named module for
//! that reason: nothing in the product needs it, and a reader should not have to
//! wonder whether the transport takes a global lock. It does not.
//!
//! Correctness note: a lock is only honest if it cannot wedge the suite. A
//! holder that dies without unlocking would otherwise block every later run
//! forever, so the lock is stolen once it is provably stale.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// How long a lock may be held before a waiter treats it as abandoned.
///
/// Comfortably longer than any honest swarm test (the slowest observed is a few
/// seconds) and shorter than a person's patience with a wedged suite.
const STALE_AFTER: Duration = Duration::from_secs(180);

/// How long to wait for the lock before giving up and running anyway.
///
/// Running anyway rather than failing: the lock exists to make a flaky test
/// reliable, and turning it into a NEW way for the suite to fail would be a
/// poor trade. A test that proceeds without the lock is merely as reliable as
/// it was before this module existed.
const WAIT_LIMIT: Duration = Duration::from_secs(240);

/// Held for as long as a test needs exclusive use of the host's network.
///
/// Acquire it before starting any [`crate::GossipTransport`] in a test, and let
/// it drop at the end of the test.
#[derive(Debug)]
pub struct SwarmLock {
    dir: PathBuf,
    /// False when the wait limit expired and the test proceeded regardless, in
    /// which case dropping must not remove someone else's lock.
    held: bool,
}

impl SwarmLock {
    /// Take the lock, waiting for whoever holds it.
    ///
    /// A directory is the lock because `create_dir` is atomic on every platform
    /// this runs on — it either creates or fails, with no window between the
    /// check and the create that two processes could both pass through.
    #[must_use]
    pub fn acquire() -> Self {
        let dir = std::env::temp_dir().join("croft-swarm-test.lock");
        let started = Instant::now();

        loop {
            match std::fs::create_dir(&dir) {
                Ok(()) => return SwarmLock { dir, held: true },
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Self::is_stale(&dir) {
                        // Whoever held this did not live to release it. Remove
                        // and retry rather than leaving every future run to
                        // wait out the full limit.
                        let _ = std::fs::remove_dir(&dir);
                        continue;
                    }
                    if started.elapsed() > WAIT_LIMIT {
                        return SwarmLock { dir, held: false };
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                // An unexpected error (a read-only temp dir, say) should not
                // stop the suite; proceed unlocked, as before this existed.
                Err(_) => return SwarmLock { dir, held: false },
            }
        }
    }

    fn is_stale(dir: &PathBuf) -> bool {
        let Ok(meta) = std::fs::metadata(dir) else {
            return false;
        };
        let Ok(created) = meta.modified() else {
            return false;
        };
        SystemTime::now()
            .duration_since(created)
            .map(|age| age > STALE_AFTER)
            .unwrap_or(false)
    }
}

impl Drop for SwarmLock {
    fn drop(&mut self) {
        // Runs on the unwinding path too, so a failing assertion releases the
        // lock rather than stalling every test after it.
        if self.held {
            let _ = std::fs::remove_dir(&self.dir);
        }
    }
}
