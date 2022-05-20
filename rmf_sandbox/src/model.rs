use bevy::prelude::*;
use bevy_inspector_egui::Inspectable;

#[derive(serde::Deserialize, Component, Inspectable, Clone, Default)]
#[serde(from = "ModelRaw")]
pub struct Model {
    pub x_raw: f64,
    pub y_raw: f64,
    pub yaw: f64,
    pub x_meters: f64,
    pub y_meters: f64,
    pub instance_name: String,
    pub model_name: String,
    pub z_offset: f64,
}

impl From<ModelRaw> for Model {
    fn from(raw: ModelRaw) -> Model {
        Model {
            x_raw: raw.x,
            y_raw: -raw.y,
            yaw: raw.yaw,
            x_meters: raw.x,
            y_meters: -raw.y,
            instance_name: raw.name,
            model_name: raw.model_name,
            // TODO: implement
            z_offset: 0.,
        }
    }
}

impl Model {
    pub fn transform(&self) -> Transform {
        Transform {
            rotation: Quat::from_rotation_z((self.yaw - 1.5707) as f32),
            translation: Vec3::new(
                self.x_meters as f32,
                self.y_meters as f32,
                self.z_offset as f32,
            ),
            scale: Vec3::ONE,
        }
    }

    pub fn from_xyz_yaw(
        instance_name: &str,
        model_name: &str,
        x: f64,
        y: f64,
        z: f64,
        yaw: f64,
    ) -> Model {
        return Model {
            instance_name: instance_name.to_string(),
            model_name: model_name.to_string(),
            x_raw: x,
            y_raw: y,
            x_meters: x,
            y_meters: y,
            yaw: yaw,
            z_offset: z,
        };
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ModelRaw {
    model_name: String,
    name: String,
    #[serde(rename = "static")]
    static_: bool,
    x: f64,
    y: f64,
    z: f64,
    yaw: f64,
}