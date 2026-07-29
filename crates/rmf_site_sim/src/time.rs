use std::ops::{Add, Sub};
use std::time::Duration;

/// A point in simulated time within a simulation, measuring the elapsed duration
/// since the beginning of the simulation, [`SimulationTime::zero`].
///
/// This type is only useful with [`Duration`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimulationTime(Duration);

impl SimulationTime {
    /// Create a new `SimulationTime` from the given elapsed duration.
    pub fn new(elapsed: Duration) -> Self {
        Self(elapsed)
    }

    /// The start of a simulation, when no time has elapsed.
    pub const fn zero() -> Self {
        Self(Duration::ZERO)
    }

    /// The duration that elapsed since the start of the simulation.
    pub fn elapsed(&self) -> Duration {
        self.0
    }
}

impl Add<Duration> for SimulationTime {
    type Output = SimulationTime;

    /// # Panics
    ///
    /// Panics if the resulting time overflows, as [`Duration`] addition does.
    fn add(self, duration: Duration) -> Self {
        Self(self.0 + duration)
    }
}

impl Sub<Duration> for SimulationTime {
    type Output = SimulationTime;

    /// # Panics
    ///
    /// Panics if `duration` is longer than the time elapsed so far, as [`Duration`] subtraction
    /// does.
    fn sub(self, duration: Duration) -> Self {
        Self(self.0 - duration)
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
