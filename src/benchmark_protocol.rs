//! Pure monotonic benchmark protocol, independent of Bevy schedules.
#[derive(Clone, Copy, Default)]
pub struct Frame {
    pub elapsed: f64,
    pub interval: f64,
    pub animation: f64,
    pub effects: f64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Waiting,
    Warmup,
    Measure,
    Finished,
}
pub struct Run {
    pub phase: Phase,
    pub epoch: u64,
    pub previous: u64,
    pub anchor: u64,
    pub valid: bool,
    pub reason: Option<String>,
    pub frames: Vec<Frame>,
}
impl Run {
    pub fn new() -> Self {
        Self {
            phase: Phase::Waiting,
            epoch: 0,
            previous: 0,
            anchor: 0,
            valid: false,
            reason: None,
            frames: Vec::with_capacity(36000),
        }
    }
    pub fn stop(&mut self, reason: &str) {
        self.valid = false;
        self.reason = Some(reason.into());
        self.phase = Phase::Finished;
    }
    pub fn sample(&mut self, now: u64, ready: bool, mut frame: Frame) -> bool {
        match self.phase {
            Phase::Waiting => {
                if ready {
                    self.epoch = now;
                    self.previous = now;
                    self.phase = Phase::Warmup;
                    return true;
                }
            }
            Phase::Warmup | Phase::Measure => {
                if now <= self.previous {
                    self.stop("nonmonotonic application clock");
                    return false;
                }
                if !ready {
                    if self.phase == Phase::Measure {
                        self.stop("initialization changed during measurement");
                    } else {
                        self.phase = Phase::Waiting;
                    }
                    return false;
                }
                if self.phase == Phase::Warmup {
                    self.previous = now;
                    if now - self.epoch >= 30_000_000_000 {
                        self.anchor = now;
                        self.phase = Phase::Measure;
                    }
                } else {
                    if self.frames.len() >= 36000 {
                        self.stop("frame capacity exceeded");
                        return false;
                    }
                    frame.elapsed = (now - self.anchor) as f64 * 1e-6;
                    frame.interval = (now - self.previous) as f64 * 1e-6;
                    self.frames.push(frame);
                    self.previous = now;
                    if now - self.anchor >= 60_000_000_000 {
                        self.valid = true;
                        self.phase = Phase::Finished;
                    }
                }
            }
            Phase::Finished => {}
        }
        false
    }
}
pub fn statistics(values: &[f64]) -> Vec<(&'static str, f64)> {
    assert!(!values.is_empty() && values.iter().all(|v| v.is_finite() && *v > 0.));
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let total: f64 = sorted.iter().sum();
    let low = (n as f64 * 0.01).ceil() as usize;
    let percentile = |p: f64| sorted[(n as f64 * p).ceil() as usize - 1];
    vec![
        ("sample_count", n as f64),
        ("duration_ms", total),
        ("average_fps", 1000. * n as f64 / total),
        (
            "low_1_fps",
            1000. * low as f64 / sorted[n - low..].iter().sum::<f64>(),
        ),
        ("mean_ms", total / n as f64),
        ("p50_ms", percentile(0.5)),
        ("p95_ms", percentile(0.95)),
        ("p99_ms", percentile(0.99)),
        ("max_ms", sorted[n - 1]),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tail_and_percentiles() {
        let mut v = vec![10.; 100];
        v[99] = 100.;
        let s = statistics(&v);
        assert_eq!(s.iter().find(|v| v.0 == "p99_ms").unwrap().1, 10.);
        assert_eq!(s.iter().find(|v| v.0 == "low_1_fps").unwrap().1, 10.);
    }
    #[test]
    fn fractional_tail() {
        let mut values = vec![10.; 102];
        values[100] = 50.;
        values[101] = 100.;
        let low = statistics(&values)
            .into_iter()
            .find(|v| v.0 == "low_1_fps")
            .unwrap()
            .1;
        assert!((low - 1000. / 75.).abs() < 1e-10);
    }
    #[test]
    fn warmup_and_duration() {
        let mut r = Run::new();
        assert!(r.sample(100, true, Frame::default()));
        r.sample(30_000_000_099, true, Frame::default());
        assert!(r.phase == Phase::Warmup && r.frames.is_empty());
        r.sample(30_000_000_100, true, Frame::default());
        for i in 1..=600 {
            r.sample(30_000_000_100 + i * 100_000_000, true, Frame::default());
        }
        assert!(r.valid && r.frames.len() == 600);
    }
    #[test]
    fn invalid_clock_and_readiness() {
        let mut r = Run::new();
        r.sample(100, true, Frame::default());
        r.sample(100, true, Frame::default());
        assert!(r.phase == Phase::Finished && !r.valid);
        let mut r = Run::new();
        r.sample(100, true, Frame::default());
        r.sample(30_000_000_100, true, Frame::default());
        r.sample(30_100_000_100, false, Frame::default());
        assert!(r.phase == Phase::Finished && !r.valid && r.frames.is_empty());
    }
}
