use ash::vk;

pub fn image_info<'a>(
    width: u32,
    height: u32,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> vk::ImageCreateInfo<'a> {
    vk::ImageCreateInfo::default()
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
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
}

pub fn image_view_info<'a>(image: vk::Image, format: vk::Format) -> vk::ImageViewCreateInfo<'a> {
    vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1),
        )
}

pub fn layout_transition2<'a>(
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
) -> vk::ImageMemoryBarrier2<'a> {
    vk::ImageMemoryBarrier2::default()
        .image(image.clone())
        .old_layout(from)
        .new_layout(to)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
}

pub fn buffer_image_copy(width: u32, height: u32) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
}
