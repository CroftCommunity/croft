//! The measurement port (O10, E117 P2): §11.11's two unearned measurements and
//! the §11.10.1 experiment matrix instrument through this trait instead of
//! re-plumbing the fold. The default is a no-op; the core never requires a
//! sink to exist.

/// Counters the fold volunteers as it works. Implementations must be cheap;
/// the fold calls these on hot paths.
pub trait Metrics {
    /// One governance fact folded.
    fn fact_folded(&self) {}
    /// A deterministic replay ran, over this many facts.
    fn replay(&self, _facts: usize) {}
    /// The number of currently-open contested pairs after a transition.
    fn contested_open(&self, _pairs: usize) {}
}

/// The default sink: measures nothing, costs nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMetrics;

impl Metrics for NoopMetrics {}
