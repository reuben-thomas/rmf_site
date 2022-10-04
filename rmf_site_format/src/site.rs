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
use bevy::prelude::{Component, Entity};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

pub use ron::ser::PrettyConfig as Style;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(feature = "bevy", derive(Component))]
pub struct SiteProperties {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Site {
    pub format_version: SemVer,
    pub properties: SiteProperties,
    pub levels: BTreeMap<u32, Level>,
    pub lifts: BTreeMap<u32, Lift<u32>>,
    pub nav_graphs: BTreeMap<u32, NavGraph>,
    pub agents: BTreeMap<u32, Agent>,
}

fn default_style_config() -> Style {
    Style::new()
        .depth_limit(4)
        .new_line("\n".to_string())
        .indentor("  ".to_string())
        .struct_names(false)
}

impl Site {
    pub fn to_writer<W: io::Write>(&self, writer: W) -> ron::Result<()> {
        ron::ser::to_writer_pretty(writer, self, default_style_config())
    }

    pub fn to_writer_custom<W: io::Write>(&self, writer: W, style: Style) -> ron::Result<()> {
        ron::ser::to_writer_pretty(writer, self, style)
    }

    pub fn to_string(&self) -> ron::Result<String> {
        ron::ser::to_string_pretty(self, default_style_config())
    }

    pub fn to_string_custom(&self, style: Style) -> ron::Result<String> {
        ron::ser::to_string_pretty(self, style)
    }

    pub fn from_reader<R: io::Read>(reader: R) -> ron::Result<Self> {
        // TODO(MXG): Validate the parsed data, e.g. make sure anchor pairs
        // belong to the same level.
        ron::de::from_reader(reader)
    }

    pub fn from_str<'a>(s: &'a str) -> ron::Result<Self> {
        ron::de::from_str(s)
    }

    pub fn from_bytes<'a>(s: &'a [u8]) -> ron::Result<Self> {
        ron::de::from_bytes(s)
    }
}

pub trait RefTrait: Ord + Eq + Copy + Send + Sync + 'static {}

impl RefTrait for u32 {}

#[cfg(feature = "bevy")]
impl RefTrait for Entity {}