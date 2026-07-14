use std::time::Duration;

// TODO:
// - Should we make use of a Time<Simulation> resource instead? https://docs.rs/bevy/latest/bevy/time/struct.Time.html
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimulationTime(Duration);

impl SimulationTime {
    pub const fn new(elapsed: Duration) -> Self {
        Self(elapsed)
    }

    pub const fn elapsed(&self) -> Duration {
        self.0
    }
}

impl From<Duration> for SimulationTime {
    fn from(elapsed: Duration) -> Self {
        Self(elapsed)
    }
}

impl From<SimulationTime> for Duration {
    fn from(time: SimulationTime) -> Self {
        time.0
    }
}
