use ash::vk;

use bevy::{prelude::*, render::RenderApp};

use crate::{
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
    render_texture::padd_pixel_bytes_rgba_unorm,
};

pub struct BlueNoisePlugin;

#[derive(Resource)]
pub struct BlueNoiseBuffer(pub Buffer<u8>);

impl Plugin for BlueNoisePlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        let render_device = render_app.world().get_resource::<RenderDevice>().unwrap();
        let mut bluenoise_buffer_host = render_device.create_host_buffer(64 * 128 * 128 * 2, vk::BufferUsageFlags::TRANSFER_SRC);
        let mut bluenoise_data = render_device.map_buffer(&mut bluenoise_buffer_host);
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

            for y in 0..128 {
                for x in 0..128 {
                    bluenoise_data[128 * 128 * 2 * texture_idx + 128 * 2 * y + 2 * x + 0] = padded_data[128 * 4 * y + 4 * x + 0];
                    bluenoise_data[128 * 128 * 2 * texture_idx + 128 * 2 * y + 2 * x + 1] = padded_data[128 * 4 * y + 4 * x + 1];
                }
            }
        }

        let bluenoise_buffer_device = render_device.create_device_buffer(bluenoise_data.nr_elements, vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER);
        render_device.run_transfer_commands(|cmd_buffer| {
            render_device.upload_buffer(cmd_buffer, &bluenoise_buffer_host, &bluenoise_buffer_device);
        });

        render_app.insert_resource(BlueNoiseBuffer(bluenoise_buffer_device));
    }
}
