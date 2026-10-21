//! REQ-PERF-01's thresholds and the percentiles `thodu bench` reports.

/// The median time from `thodu hints` to labels drawn may not exceed this.
pub const MEDIAN_MS: f64 = 100.0;
/// Nor may the 95th percentile exceed this.
pub const P95_MS: f64 = 250.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub sessions: usize,
    pub median_ms: f64,
    pub p95_ms: f64,
}

impl Summary {
    /// Whether both percentiles are within REQ-PERF-01.
    pub fn within(&self) -> bool {
        self.median_ms <= MEDIAN_MS && self.p95_ms <= P95_MS
    }
}

/// The nearest-rank percentile `p` (0 to 100) of `samples`, which need not
/// be sorted; None when there are none.
pub fn percentile(samples: &[f64], p: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    Some(sorted[rank.clamp(1, sorted.len()) - 1])
}

pub fn summarize(samples: &[f64]) -> Option<Summary> {
    Some(Summary {
        sessions: samples.len(),
        median_ms: percentile(samples, 50.0)?,
        p95_ms: percentile(samples, 95.0)?,
    })
}
