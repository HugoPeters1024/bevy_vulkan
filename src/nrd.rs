use ash::vk;
use bevy::{prelude::*, render::RenderApp};

use crate::{
    ray_render_plugin::{Render, RenderSet},
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
    vk_utils,
};
pub struct NrdPlugin;

impl Plugin for NrdPlugin {
    fn build(&self, app: &mut App) {
        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        render_app.add_systems(Render, test.in_set(RenderSet::Prepare));
    }
}

const WIDTH: u16 = 1920;
const HEIGHT: u16 = 1080;

#[derive(Resource)]
pub struct NrdResources {
    pipelines: Vec<(
        vk::PipelineLayout,
        vk::Pipeline,
        vk::DescriptorSetLayout,
        Vec<vk::DescriptorSet>,
    )>,
    transient_pool: Vec<(vk::Image, vk::ImageView)>,
    permanent_pool: Vec<(vk::Image, vk::ImageView)>,
    samplers: Vec<vk::Sampler>,
    pub out_diff_radiance_hit_dist: (vk::Image, vk::ImageView),
    in_mv: (vk::Image, vk::ImageView),
    // all the same maximum size
    constant_buffers: Vec<Buffer<u8>>,
    constant_buffer_max_size: u32,
    instance: nrd_sys::Instance,
    sampler_offset: u32,
    texture_offset: u32,
    constant_buffer_offset: u32,
    storage_texture_and_buffer_offset: u32,
}

fn test(mut commands: Commands, render_device: Res<RenderDevice>, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;

    let lib_desc = nrd_sys::Instance::library_desc();
    let id1 = nrd_sys::Identifier(0);
    let instance = nrd_sys::Instance::new(&[nrd_sys::DenoiserDesc {
        identifier: id1,
        denoiser: nrd_sys::Denoiser::ReblurDiffuse,
        render_width: WIDTH,
        render_height: HEIGHT,
    }])
    .unwrap();
    let res = unsafe { make_vk_resources(render_device, &lib_desc, instance) };
    commands.insert_resource(res);
}

unsafe fn make_vk_resources(
    render_device: Res<RenderDevice>,
    lib: &nrd_sys::ffi::LibraryDesc,
    mut instance: nrd_sys::Instance,
) -> NrdResources {
    let id1 = nrd_sys::Identifier(0);

    instance
        .set_common_settings(&nrd_sys::CommonSettings::default())
        .unwrap();

    instance
        .set_denoiser_settings(id1, &nrd_sys::ReferenceSettings::default())
        .unwrap();

    let instance_desc = instance.desc();

    let mut samplers = Vec::new();
    for sampler in instance_desc.samplers() {
        let sampler_info = match sampler {
            nrd_sys::Sampler::NearestClamp => vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            nrd_sys::Sampler::NearestMirroredRepeat => vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::MIRRORED_REPEAT)
                .address_mode_v(vk::SamplerAddressMode::MIRRORED_REPEAT)
                .address_mode_w(vk::SamplerAddressMode::MIRRORED_REPEAT),
            nrd_sys::Sampler::LinearClamp => vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            nrd_sys::Sampler::LinearMirroredRepeat => vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::MIRRORED_REPEAT)
                .address_mode_v(vk::SamplerAddressMode::MIRRORED_REPEAT)
                .address_mode_w(vk::SamplerAddressMode::MIRRORED_REPEAT),
        };

        let sampler = render_device.create_sampler(&sampler_info, None).unwrap();
        samplers.push(sampler);
    }

    let mut pipelines = Vec::new();
    for pipeline_desc in instance_desc.pipelines() {
        let shader_stage = render_device.load_shader(
            &*pipeline_desc.compute_shader_spirv,
            vk::ShaderStageFlags::COMPUTE,
        );

        let mut bindings = Vec::new();

        for (si, _) in samplers.iter().enumerate() {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(lib.spirv_binding_offsets.sampler_offset + si as u32)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            );
        }

        if pipeline_desc.has_constant_data {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(lib.spirv_binding_offsets.constant_buffer_offset)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            );
        }

        for resource_range in pipeline_desc.resource_ranges() {
            let descriptor_index = resource_range.base_register_index
                + match resource_range.descriptor_type {
                    nrd_sys::DescriptorType::Texture => lib.spirv_binding_offsets.texture_offset,
                    nrd_sys::DescriptorType::StorageTexture => {
                        lib.spirv_binding_offsets.storage_texture_and_buffer_offset
                    }
                };

            for i in 0..resource_range.descriptors_num {
                bindings.push(
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(descriptor_index + i)
                        .descriptor_count(1)
                        .descriptor_type(match resource_range.descriptor_type {
                            nrd_sys::ffi::DescriptorType::Texture => {
                                vk::DescriptorType::SAMPLED_IMAGE
                            }
                            nrd_sys::ffi::DescriptorType::StorageTexture => {
                                vk::DescriptorType::STORAGE_IMAGE
                            }
                        })
                        .stage_flags(vk::ShaderStageFlags::COMPUTE),
                );
            }
        }

        let descriptor_set_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

        let descriptor_set_layout = render_device
            .create_descriptor_set_layout(&descriptor_set_layout_info, None)
            .unwrap();

        let layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));

        let pipeline_layout = render_device
            .create_pipeline_layout(&layout_info, None)
            .unwrap();

        let pipeline_info = vk::ComputePipelineCreateInfo::default()
            .stage(shader_stage)
            .layout(pipeline_layout);

        let pipeline = render_device
            .create_compute_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
            .unwrap()[0];

        pipelines.push((pipeline_layout, pipeline, descriptor_set_layout, Vec::new()));
    }

    let mut transient_pool = Vec::new();
    let mut permanent_pool = Vec::new();

    for texture_descr in instance_desc.transient_pool() {
        transient_pool.push(make_gpu_image(
            &render_device,
            // TODO: specialize when possible
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            texture_descr,
        ));
    }

    for texture_descr in instance_desc.permanent_pool() {
        permanent_pool.push(make_gpu_image(
            &render_device,
            // TODO: specialize when possible
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
            texture_descr,
        ));
    }

    let constant_buffer_max_size = instance_desc.constant_buffer_max_data_size;

    // create the input and output images
    let out_diff_radiance_hit_dist = make_gpu_image(
        &render_device,
        vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
        &nrd_sys::TextureDesc {
            format: nrd_sys::Format::RGBA16_SFLOAT,
            width: WIDTH,
            height: HEIGHT,
            mip_num: 1,
        },
    );

    let in_mv = make_gpu_image(
        &render_device,
        vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
        &nrd_sys::TextureDesc {
            format: nrd_sys::Format::RGBA16_SFLOAT,
            width: WIDTH,
            height: HEIGHT,
            mip_num: 1,
        },
    );

    NrdResources {
        pipelines,
        transient_pool,
        permanent_pool,
        samplers,
        out_diff_radiance_hit_dist,
        in_mv,
        instance,
        constant_buffers: Vec::new(),
        constant_buffer_max_size,
        sampler_offset: lib.spirv_binding_offsets.sampler_offset,
        texture_offset: lib.spirv_binding_offsets.texture_offset,
        constant_buffer_offset: lib.spirv_binding_offsets.constant_buffer_offset,
        storage_texture_and_buffer_offset: lib
            .spirv_binding_offsets
            .storage_texture_and_buffer_offset,
    }
}

pub unsafe fn make_gpu_image(
    render_device: &RenderDevice,
    usage: vk::ImageUsageFlags,
    texture_descr: &nrd_sys::TextureDesc,
) -> (vk::Image, vk::ImageView) {
    let format = match texture_descr.format {
        nrd_sys::Format::RG8_UNORM => vk::Format::R8G8_UNORM,
        nrd_sys::Format::RGBA8_UNORM => vk::Format::R8G8B8A8_UNORM,
        nrd_sys::Format::R8_UINT => vk::Format::R8_UINT,
        nrd_sys::Format::R16_UINT => vk::Format::R16_UINT,
        nrd_sys::Format::R8_UNORM => vk::Format::R8_UNORM,
        nrd_sys::Format::RGBA16_SFLOAT => vk::Format::R16G16B16A16_SFLOAT,
        nrd_sys::Format::R16_SFLOAT => vk::Format::R16_SFLOAT,
        nrd_sys::Format::R32_SFLOAT => vk::Format::R32_SFLOAT,
        _ => todo!("{:?}", texture_descr.format),
    };

    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width: texture_descr.width as u32,
            height: texture_descr.height as u32,
            depth: 1,
        })
        .mip_levels(texture_descr.mip_num as u32)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let image = render_device.create_gpu_image(&image_info);

    render_device.run_transfer_commands(|cmd_buffer| {
        vk_utils::transition_image_layout(
            &render_device,
            cmd_buffer,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::GENERAL,
        );
    });

    let image_view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .components(vk::ComponentMapping::default())
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    let image_view = render_device
        .create_image_view(&image_view_info, None)
        .unwrap();

    return (image, image_view);
}

pub unsafe fn record_commands(
    render_device: &RenderDevice,
    cmd_buffer: vk::CommandBuffer,
    nrd: &mut NrdResources,
    in_viewz: (vk::Image, vk::ImageView),
    in_normal_roughness: (vk::Image, vk::ImageView),
    in_diff_radiance_hitdist: (vk::Image, vk::ImageView),
    frame_index: u32,
    projection_matrix: &Mat4,
    projection_matrix_prev: &Mat4,
    view_matrix: &Mat4,
    view_matrix_prev: &Mat4,
) {
    let mut settings = nrd_sys::CommonSettings::default();
    settings.frame_index = frame_index;
    settings.view_to_clip_matrix = projection_matrix.to_cols_array();
    settings.view_to_clip_matrix_prev = projection_matrix_prev.to_cols_array();
    settings.world_to_view_matrix = view_matrix.to_cols_array();
    settings.world_to_view_matrix_prev = view_matrix_prev.to_cols_array();
    nrd.instance.set_common_settings(&settings).unwrap();

    if let Ok(queue) = render_device.queue.lock() {
        render_device.queue_wait_idle(*queue).unwrap();
    }
    let id1 = nrd_sys::Identifier(0);
    let dispatches = nrd
        .instance
        .get_compute_dispatches(&[id1])
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>();

    // keep track of the descriptor sets per pipeline used (allocated lazily)
    let mut per_pipeline_descriptor_set_idx = vec![0; nrd.pipelines.len()];

    while nrd.constant_buffers.len() < dispatches.len() {
        nrd.constant_buffers.push(render_device.create_host_buffer(
            nrd.constant_buffer_max_size as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        ));
    }

    for (di, dispatch) in dispatches.iter().enumerate() {
        if per_pipeline_descriptor_set_idx[dispatch.pipeline_index as usize] >= nrd.pipelines[dispatch.pipeline_index as usize].3.len() {
            let descriptor_pool = render_device.descriptor_pool.lock().unwrap();
            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(*descriptor_pool)
                .set_layouts(std::slice::from_ref(&nrd.pipelines[dispatch.pipeline_index as usize].2));

            let descriptor_set = render_device.allocate_descriptor_sets(&alloc_info).unwrap()[0];

            nrd.pipelines[dispatch.pipeline_index as usize].3.push(descriptor_set);
        }

        let descriptor_set =
            nrd.pipelines[dispatch.pipeline_index as usize].3[per_pipeline_descriptor_set_idx[dispatch.pipeline_index as usize]];
        per_pipeline_descriptor_set_idx[dispatch.pipeline_index as usize] += 1;

        let (pipeline_layout, pipeline, descriptor_set_layout, _) =
            nrd.pipelines[dispatch.pipeline_index as usize];


        // Set the constant buffer in the descriptor set
        if !dispatch.constant_buffer().is_empty() {
            let constant_buffer = &mut nrd.constant_buffers[di];

            {
                let mut constant_buffer_data = render_device.map_buffer(constant_buffer);
                for (i, byte) in dispatch.constant_buffer().iter().enumerate() {
                    constant_buffer_data[i] = *byte;
                }
            }

            let descriptor_index = nrd.constant_buffer_offset;

            let buffer_info = vk::DescriptorBufferInfo::default()
                .buffer(constant_buffer.handle)
                .offset(0)
                .range(vk::WHOLE_SIZE);

            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(descriptor_index)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .buffer_info(std::slice::from_ref(&buffer_info));

            render_device.update_descriptor_sets(&[descriptor_write], &[]);
        }

        // set the samplers in the descriptor set
        for (si, sampler) in nrd.samplers.iter().enumerate() {
            let descriptor_index = nrd.sampler_offset + si as u32;

            let image_info = vk::DescriptorImageInfo::default()
                .sampler(*sampler)
                .image_view(vk::ImageView::null())
                .image_layout(vk::ImageLayout::GENERAL);

            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(descriptor_index)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(std::slice::from_ref(&image_info));

            render_device.update_descriptor_sets(&[descriptor_write], &[]);
        }

        // set the other resources in the descriptor set
        let mut storage_texture_next_slot = nrd.storage_texture_and_buffer_offset;
        let mut texture_next_slot = nrd.texture_offset;
        for resource in dispatch.resources() {
            let descriptor_index = match resource.state_needed {
                nrd_sys::DescriptorType::StorageTexture => {
                    storage_texture_next_slot += 1;
                    storage_texture_next_slot - 1
                }
                nrd_sys::DescriptorType::Texture => {
                    texture_next_slot += 1;
                    texture_next_slot - 1
                }
            };

            let descriptor_type = match resource.state_needed {
                nrd_sys::DescriptorType::StorageTexture => vk::DescriptorType::STORAGE_IMAGE,
                nrd_sys::DescriptorType::Texture => vk::DescriptorType::SAMPLED_IMAGE,
            };

            let image_view = resource_desc_to_image(
                nrd,
                resource,
                in_viewz,
                in_normal_roughness,
                in_diff_radiance_hitdist,
            )
            .1;

            let image_info = vk::DescriptorImageInfo::default()
                .image_view(image_view)
                .image_layout(vk::ImageLayout::GENERAL);

            let descriptor_write = vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(descriptor_index)
                .descriptor_type(descriptor_type)
                .image_info(std::slice::from_ref(&image_info));

            render_device.update_descriptor_sets(&[descriptor_write], &[]);
        }

        render_device.cmd_bind_descriptor_sets(
            cmd_buffer,
            vk::PipelineBindPoint::COMPUTE,
            pipeline_layout,
            0,
            std::slice::from_ref(&descriptor_set),
            &[],
        );

        render_device.cmd_bind_pipeline(cmd_buffer, vk::PipelineBindPoint::COMPUTE, pipeline);

        // TODO: how to derive these?
        if di >= 14 {
            let mut image_barriers = Vec::new();
            for resource in dispatch.resources() {
                let image = resource_desc_to_image(
                    nrd,
                    resource,
                    in_viewz,
                    in_normal_roughness,
                    in_diff_radiance_hitdist,
                )
                .0;

                image_barriers.push(
                    vk::ImageMemoryBarrier2::default()
                        .image(image)
                        .src_stage_mask(vk::PipelineStageFlags2KHR::COMPUTE_SHADER)
                        .dst_stage_mask(vk::PipelineStageFlags2KHR::COMPUTE_SHADER)
                        .src_access_mask(vk::AccessFlags2KHR::SHADER_STORAGE_WRITE)
                        .dst_access_mask(vk::AccessFlags2KHR::SHADER_STORAGE_READ)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(1)
                                .base_array_layer(0)
                                .layer_count(1),
                        ),
                );
            }

            render_device.cmd_pipeline_barrier2(
                cmd_buffer,
                &vk::DependencyInfoKHR::default().image_memory_barriers(&image_barriers),
            );
        }

        render_device.cmd_dispatch(
            cmd_buffer,
            dispatch.grid_width as u32,
            dispatch.grid_height as u32,
            1,
        );
    }
}

fn resource_desc_to_image(
    nrd: &NrdResources,
    resource: &nrd_sys::ResourceDesc,
    in_viewz: (vk::Image, vk::ImageView),
    in_normal_roughness: (vk::Image, vk::ImageView),
    in_diff_radiance_hitdist: (vk::Image, vk::ImageView),
) -> (vk::Image, vk::ImageView) {
    match resource.ty {
        nrd_sys::ResourceType::TRANSIENT_POOL => {
            nrd.transient_pool[resource.index_in_pool as usize]
        }

        nrd_sys::ResourceType::PERMANENT_POOL => {
            nrd.permanent_pool[resource.index_in_pool as usize]
        }

        nrd_sys::ResourceType::OUT_DIFF_RADIANCE_HITDIST => nrd.out_diff_radiance_hit_dist,
        nrd_sys::ResourceType::IN_MV => nrd.in_mv,
        nrd_sys::ResourceType::IN_VIEWZ => in_viewz,
        nrd_sys::ResourceType::IN_NORMAL_ROUGHNESS => in_normal_roughness,
        nrd_sys::ResourceType::IN_DIFF_RADIANCE_HITDIST => in_diff_radiance_hitdist,

        _ => todo!("{:?}", resource.ty),
    }
}
