use ash::vk;

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
