/*
 * Copyright (C) 2026 Open Source Robotics Foundation
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
*/

//! # rmf_site_sim
//!
//! `rmf_site_sim` provides the ability to perform
//! [discrete-event simulation](https://en.wikipedia.org/wiki/Discrete-event_simulation) (DES) in Bevy,
//! and is primarily intended for use with the [rmf_site_editor](https://github.com/open-rmf/rmf_site).
//!
//! The following illustrates how the core components of a DES are represented, or can be accessd in
//! this library.
//!
//! | Concept | Definition | Representation |
//! | --- | --- | --- |
//! | System | A collection of entities being modelled over time. | - |
//! | System Model | An abstract representation of the system. | A collection of Bevy systems ([`bevy::prelude::System`]) configured in a [`SimulationBuilder``]. |
//! | System State | A state vector describing the state of the world at any time-instance. | Entities, components, and resources in [`bevy::prelude::World`]. |
//! | Entity | An instance that requires representation in the system model. | [`bevy::prelude::Entity`] |
//! | Attributes | Properties of an entity, or the system as a whole. | [`bevy::prelude::Component`], [`bevy::prelude::Resource`] |
//! | Event | An instantaneous occurrence that changes the state of the system. | An [`event::DiscreteEvent`], implemented automatically for any cloneable [`bevy::prelude::Command`]. |
//! | Event Notice | A record of an event that may occur at some simulation time and the necessary parameters to execute it. | Created using the [`event::DiscreteChangeWriter`], [`event::DiscreteComponentWriter`], and [`event::DiscreteResourceWriter`]. |
//! | Future Event List (FEL) | A list of event notices for future events, ordered by time of occurrence. | The [`event::DiscreteEvents`] resource holds a list of event notices.. |
//! | Clock | A variable representing the current value of simulated time. | The [`compute::SimulationComputeClock`] resource. |
//! | Output | Data produced by running the simulation. | A [`SimulationStep`] for each computed simulation time, collected by the [`Simulation`] component. |
pub mod compute;
pub mod event;
pub mod playback;
pub mod schedule;
pub mod simulation;
pub mod sync;
pub mod time;

pub use schedule::*;
pub use simulation::*;
