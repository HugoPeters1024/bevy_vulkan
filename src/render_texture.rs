use ash::vk;
use bevy::{app::Plugin, asset::AssetApp, render::texture::ImageLoader};
use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};

use crate::{
    render_buffer::BufferProvider,
    render_device::RenderDevice,
    vk_init, vk_utils,
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};

pub struct RenderTexturePlugin;

impl Plugin for RenderTexturePlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.init_asset::<bevy::prelude::Image>();
        app.init_asset_loader::<ImageLoader>();
        app.init_vulkan_asset::<bevy::prelude::Image>();
    }
}

pub struct RenderTexture {
    pub image: vk::Image,
    pub image_view: vk::ImageView,
}

impl VulkanAsset for bevy::prelude::Image {
    type ExtractedAsset = bevy::prelude::Image;
    type ExtractParam = ();
    type PreparedAsset = RenderTexture;

    fn extract_asset(
        &self,
        _param: &mut bevy::ecs::system::SystemParamItem<Self::ExtractParam>,
    ) -> Option<Self::ExtractedAsset> {
        Some(self.clone())
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        render_device: &RenderDevice,
    ) -> Self::PreparedAsset {
        let res = load_texture_from_bytes(
            render_device,
            vk::Format::R8G8B8A8_UNORM,
            asset.data.as_ref(),
            asset.texture_descriptor.size.width,
            asset.texture_descriptor.size.height,
        );

        render_device.register_bindless_texture(&res);

        res
    }

    fn destroy_asset(render_device: &RenderDevice, prepared_asset: &Self::PreparedAsset) {
        render_device
            .destroyer
            .destroy_image_view(prepared_asset.image_view);
        render_device.destroyer.destroy_image(prepared_asset.image);
    }
}

pub fn load_texture_from_bytes(
    device: &RenderDevice,
    format: vk::Format,
    bytes: &[u8],
    width: u32,
    height: u32,
) -> RenderTexture {
    let target_bytes_per_pixel = match format {
        vk::Format::R8G8B8A8_UNORM => 4,
        vk::Format::R32G32B32A32_SFLOAT => 16,
        _ => panic!("unsupported format"),
    };

    assert!(
        bytes.len() == (width * height) as usize * target_bytes_per_pixel,
        "expected {} bytes, got {}",
        (width * height) as usize * target_bytes_per_pixel,
        bytes.len()
    );
    let mut staging_buffer = device.create_host_buffer::<u8>(
        (width * height * target_bytes_per_pixel as u32) as u64,
        vk::BufferUsageFlags::TRANSFER_SRC,
    );
    {
        let mut staging_buffer = device.map_buffer(&mut staging_buffer);
        staging_buffer.as_slice_mut().copy_from_slice(bytes);
    }

    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let image_handle = unsafe { device.device.create_image(&image_info, None).unwrap() };

    let requirements = unsafe { device.device.get_image_memory_requirements(image_handle) };

    {
        let mut state = device.allocator_state.write().unwrap();

        let allocation = state
            .allocate(&AllocationCreateDesc {
                name: "render_texture",
                requirements,
                linear: false,
                location: gpu_allocator::MemoryLocation::GpuOnly,
                allocation_scheme: AllocationScheme::DedicatedImage(image_handle),
            })
            .unwrap();

        unsafe {
            device
                .device
                .bind_image_memory(image_handle, allocation.memory(), allocation.offset())
                .unwrap();
        }

        state.register_image_allocation(image_handle, allocation);
    }

    device.run_transfer_commands(|cmd_buffer| {
        vk_utils::transition_image_layout(
            &device,
            cmd_buffer,
            image_handle,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let copy_region = vk_init::buffer_image_copy(width, height);
        unsafe {
            device.device.cmd_copy_buffer_to_image(
                cmd_buffer,
                staging_buffer.handle,
                image_handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                std::slice::from_ref(&copy_region),
            );
        };
        vk_utils::transition_image_layout(
            &device,
            cmd_buffer,
            image_handle,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
    });

    device.destroyer.destroy_buffer(staging_buffer.handle);

    let view_info = vk_init::image_view_info(image_handle.clone(), format);
    let view = unsafe { device.device.create_image_view(&view_info, None).unwrap() };

    RenderTexture {
        image: image_handle,
        image_view: view,
    }
}