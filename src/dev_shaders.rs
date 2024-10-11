use bevy::prelude::*;

use crate::{
    post_process_filter::PostProcessFilter, ray_render_plugin::RenderConfig,
    raytracing_pipeline::RaytracingPipeline,
};

pub struct DevShaderPlugin;

impl Plugin for DevShaderPlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().get_resource::<AssetServer>().unwrap();

        let filter = PostProcessFilter {
            vertex_shader: asset_server.load("shaders/quad.vert"),
            fragment_shader: asset_server.load("shaders/quad.frag"),
        };

        let rtx_pipeline = RaytracingPipeline {
            raygen_shader: asset_server.load("shaders/raygen.rgen"),
            miss_shader: asset_server.load("shaders/miss.rmiss"),
            hit_shader: asset_server.load("shaders/closest_hit.rchit"),
            sphere_intersection_shader: asset_server.load("shaders/sphere_intersection.rint"),
            sphere_hit_shader: asset_server.load("shaders/sphere_hit.rchit"),
        };

        let render_config = RenderConfig {
            rtx_pipeline: asset_server.add(rtx_pipeline),
            postprocess_pipeline: asset_server.add(filter),
            skydome: Some(asset_server.load("textures/sky.hdr")),
            ..default()
        };

        app.world_mut().insert_resource(render_config);
    }
}
