/*
 * Copyright (C) 2023 Open Source Robotics Foundation
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

use crate::*;
#[cfg(feature = "bevy")]
use bevy::prelude::{Bundle, Component, Reflect, ReflectComponent};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "bevy", derive(Component, Reflect))]
#[cfg_attr(feature = "bevy", reflect(Component))]
pub struct ModelInstanceMarker;

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "bevy", derive(Bundle))]
pub struct ModelInstanceBundle {
    /// Name of the model
    pub name: NameInSite,
    /// Where the model should be loaded from
    pub source: AssetSource,
    /// Ths pose of the model
    pub pose: Pose,
    /// The motion properties of this model
    pub kinematics: Kinematics,
    /// Whether this model should be able to move in simulation
    #[serde(default, skip_serializing_if = "is_default")]
    pub is_static: IsStatic,
    /// Scale to be applied to the model
    #[serde(default, skip_serializing_if = "is_default")]
    pub scale: Scale,
    /// Only relevant for bevy
    #[serde(skip)]
    pub marker: ModelInstanceMarker,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelInstance {
    /// This may be the entity ID of a level or an anchor.
    pub parent: u32,
    /// The entity ID of the model (e.g. MobileRobot, StationaryRobot)
    /// that this is instantiating.
    pub model_description: u32,
    #[serde(flatten)]
    pub bundle: ModelInstanceBundle,
}
