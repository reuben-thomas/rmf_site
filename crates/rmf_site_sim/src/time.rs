use bevy::prelude::Resource;
use std::time::Duration;

// TODO:
// - Should we make use of a Time<Simulation> resource instead? https://docs.rs/bevy/latest/bevy/time/struct.Time.html
// - Previously discussed avoiding naming conflicts with Bevy, would it be unwise to rely more heavily on module names?
//   e.g.
//   use rmf_site_sim::time::Time as SimulationTime;
//   use bevy::time::Time;
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
