use ash::vk;
use std::fs;

use bevy::{prelude::*, render::RenderApp};

use crate::{
    render_device::RenderDevice,
    render_texture::{load_texture_from_bytes, padd_pixel_bytes_rgba_unorm, RenderTexture},
};

pub struct BlueNoisePlugin;

#[derive(Resource)]
pub struct BlueNoiseTextures(pub [RenderTexture; 64]);

impl Plugin for BlueNoisePlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        let render_device = render_app.world().get_resource::<RenderDevice>().unwrap();
        let mut bluenoise_textures = [RenderTexture::default(); 64];
        for texture_idx in 0..64 {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
            let fname = format!(
                "{}/assets/textures/bluenoise/stbn_vec2_2Dx1D_128x128x64_{}.png",
                manifest_dir, texture_idx
            );
            let decoder = png::Decoder::new(std::fs::File::open(fname).unwrap());
            let mut reader = decoder.read_info().unwrap();
            // Allocate the output buffer.
            let mut buf = vec![0; reader.output_buffer_size()];
            // Read the next frame. An APNG might contain multiple frames.
            let info = reader.next_frame(&mut buf).unwrap();
            // Grab the bytes of the image.
            let data = &buf[..info.buffer_size()];

            let bytes_per_pixel = data.len() / (128 * 128);
            let padded_data = padd_pixel_bytes_rgba_unorm(&data, bytes_per_pixel as u32, 128, 128);

            bluenoise_textures[texture_idx] = load_texture_from_bytes(
                render_device,
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::STORAGE,
                vk::ImageLayout::GENERAL,
                &padded_data,
                128,
                128,
            );
        }

        render_app.insert_resource(BlueNoiseTextures(bluenoise_textures));
    }
}
