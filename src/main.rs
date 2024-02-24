mod extract;
mod post_process_filter;
mod ray_default_plugins;
mod ray_render_plugin;
mod render_device;
mod shader;
mod swapchain;
mod vk_init;
mod vk_utils;
mod vulkan_asset;
mod raytracing_pipeline;

use ash::vk;
use bevy::{prelude::*, render::RenderApp};
use post_process_filter::PostProcessFilter;
use ray_render_plugin::{Frame, Render, RenderSet};
use raytracing_pipeline::RaytracingPipeline;
use render_device::RenderDevice;
use vulkan_asset::VulkanAssets;

use crate::ray_default_plugins::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_systems(Startup, setup);

    let render_app = app.get_sub_app_mut(RenderApp).unwrap();
    render_app.add_systems(Render, run_render.in_set(RenderSet::Render));

    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let filter = PostProcessFilter {
        vertex_shader: asset_server.load("shaders/quad.vert"),
        fragment_shader: asset_server.load("shaders/quad.frag"),
    };

    commands.spawn(asset_server.add(filter));

    let rtx_pipeline = RaytracingPipeline {
        raygen_shader: asset_server.load("shaders/raygen.rgen"),
        miss_shader: asset_server.load("shaders/miss.rmiss"),
        hit_shader: asset_server.load("shaders/closest_hit.rchit"),
    };

    commands.spawn(asset_server.add(rtx_pipeline));
}

fn run_render(
    pipelines: Res<VulkanAssets<PostProcessFilter>>,
    render_device: Res<RenderDevice>,
    frame: Res<Frame>,
) {
    let Some(pipeline) = pipelines.get_all().next() else {
        return;
    };

    unsafe {
        render_device.cmd_bind_pipeline(
            frame.cmd_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline.pipeline,
        );

        render_device.cmd_draw(frame.cmd_buffer, 3, 1, 0, 0);
    }
}
