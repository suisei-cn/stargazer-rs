use std::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct Config {
    /// Interval between schedule attempts.
    schedule_interval: Duration,
    /// Max allowed duration for an entry to be an orphan.
    max_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schedule_interval: Duration::from_secs(3),
            max_interval: Duration::from_secs(10),
        }
    }
}

impl Config {
    pub const fn new(schedule_interval: Duration, max_interval: Duration) -> Self {
        Self {
            schedule_interval,
            max_interval,
        }
    }
}

impl Config {
    pub const fn schedule_interval(&self) -> Duration {
        self.schedule_interval
    }
    pub const fn max_interval(&self) -> Duration {
        self.max_interval
    }
}
