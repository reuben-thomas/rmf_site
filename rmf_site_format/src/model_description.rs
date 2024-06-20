/*
 * Copyright (C) 2022 Open Source Robotics Foundation
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
pub struct ModelDescriptionMarker;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "bevy", derive(Bundle))]
pub struct ModelDescription {
    /// Name of the model
    pub name: NameInSite,
    /// Where the model should be loaded from
    pub source: AssetSource,
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
    pub marker: ModelDescriptionMarker,
    #[serde(skip)]
    pub group: Group,
}

impl Default for ModelDescription {
    fn default() -> Self {
        Self {
            name: NameInSite("<Unnamed>".to_string()),
            source: AssetSource::default(),
            is_static: IsStatic(false),
            kinematics: Kinematics::Static,
            scale: Scale::default(),
            marker: ModelDescriptionMarker,
            group: Group,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "bevy", derive(Component, Reflect))]
#[cfg_attr(feature = "bevy", reflect(Component))]
pub enum Kinematics {
    #[default]
    Static,
    DifferentialDrive(DifferentialDrive),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "bevy", derive(Component, Reflect))]
#[cfg_attr(feature = "bevy", reflect(Component))]
pub struct DifferentialDrive {
    pub translational_speed: f32,
    pub rotational_speed: f32,
    pub bidirectional: bool,
}

impl Default for DifferentialDrive {
    fn default() -> Self {
        Self {
            translational_speed: 1.0,
            rotational_speed: 1.0,
            bidirectional: true,
        }
    }
}
