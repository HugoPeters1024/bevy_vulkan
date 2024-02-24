mod extract;
mod post_process_filter;
mod ray_default_plugins;
mod ray_render_plugin;
mod raytracing_pipeline;
mod render_device;
mod shader;
mod swapchain;
mod vk_init;
mod vk_utils;
mod vulkan_asset;

use bevy::prelude::*;
use post_process_filter::PostProcessFilter;
use ray_render_plugin::RenderConfig;
use raytracing_pipeline::RaytracingPipeline;

use crate::ray_default_plugins::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_systems(Startup, setup);
    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let filter = PostProcessFilter {
        vertex_shader: asset_server.load("shaders/quad.vert"),
        fragment_shader: asset_server.load("shaders/quad.frag"),
    };

    let rtx_pipeline = RaytracingPipeline {
        raygen_shader: asset_server.load("shaders/raygen.rgen"),
        miss_shader: asset_server.load("shaders/miss.rmiss"),
        hit_shader: asset_server.load("shaders/closest_hit.rchit"),
    };

    commands.insert_resource(RenderConfig {
        rtx_pipeline: asset_server.add(rtx_pipeline),
        postprocess_pipeline: asset_server.add(filter),
    })
}
