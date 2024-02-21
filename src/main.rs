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

use ash::vk;
use bevy::{prelude::*, render::RenderApp};
use post_process_filter::PostProcessFilter;
use ray_render_plugin::{Frame, Render, RenderSet};
use render_device::RenderDevice;
use vulkan_asset::VulkanAssets;

use crate::ray_default_plugins::*;

#[derive(Resource)]
struct MyPipeline {
    pipeline: Handle<PostProcessFilter>,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_systems(Startup, setup);

    let render_app = app.get_sub_app_mut(RenderApp).unwrap();
    render_app.add_systems(Render, run_post_process_filter.in_set(RenderSet::Render));

    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(MyPipeline {
        pipeline: asset_server.load("shaders/pprocess.pipeline"),
    });
}

fn run_post_process_filter(
    filters: Res<VulkanAssets<PostProcessFilter>>,
    render_device: Res<RenderDevice>,
    frame: Res<Frame>,
) {
    let Some(pipeline) = filters.get_all().next() else {
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
