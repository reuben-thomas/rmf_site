use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimTime(Duration);

impl SimTime {
    pub const fn new(elapsed: Duration) -> Self {
        Self(elapsed)
    }

    pub const fn elapsed(&self) -> Duration {
        self.0
    }
}

impl From<Duration> for SimTime {
    fn from(elapsed: Duration) -> Self {
        Self(elapsed)
    }
}

impl From<SimTime> for Duration {
    fn from(time: SimTime) -> Self {
        time.0
    }
}
