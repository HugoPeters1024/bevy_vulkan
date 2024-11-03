use crate::{
    ray_render_plugin::TeardownSchedule,
    render_device::RenderDevice,
    render_texture::{load_texture_from_bytes, RenderTexture},
};
use ash::vk;
use bevy::{prelude::*, render::RenderApp};

pub const WHITE_TEXTURE_IDX: u32 = 0;
pub const DEFAULT_NORMAL_TEXTURE_IDX: u32 = 1;

#[derive(Resource)]
pub struct RenderEnv {
    white_texture: RenderTexture,
    default_normal_texture: RenderTexture,
}

pub struct RenderEnvPlugin;

impl Plugin for RenderEnvPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        let device = render_app.world().get_resource::<RenderDevice>().unwrap();
        let white_texture = load_texture_from_bytes(
            device,
            vk::Format::R8G8B8A8_UNORM,
            vk::ImageUsageFlags::SAMPLED,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            &[255, 255, 255, 255],
            1,
            1,
        );

        let default_normal_texture = load_texture_from_bytes(
            device,
            vk::Format::R8G8B8A8_UNORM,
            vk::ImageUsageFlags::SAMPLED,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            &[128, 128, 255, 0],
            1,
            1,
        );

        assert!(
            device.register_bindless_texture(&white_texture) == WHITE_TEXTURE_IDX,
            "default white texture must be index 0"
        );
        assert!(
            device.register_bindless_texture(&default_normal_texture) == DEFAULT_NORMAL_TEXTURE_IDX,
            "default normal texture must be index 1"
        );

        render_app.world_mut().insert_resource(RenderEnv {
            white_texture,
            default_normal_texture,
        });
        render_app.add_systems(TeardownSchedule, cleanup);
    }
}

fn cleanup(world: &mut World) {
    let env = world.remove_resource::<RenderEnv>().unwrap();
    let device = world.get_resource::<RenderDevice>().unwrap();
    device
        .destroyer
        .destroy_image_view(env.white_texture.image_view);
    device.destroyer.destroy_image(env.white_texture.image);
    device
        .destroyer
        .destroy_image_view(env.default_normal_texture.image_view);
    device
        .destroyer
        .destroy_image(env.default_normal_texture.image);
}
