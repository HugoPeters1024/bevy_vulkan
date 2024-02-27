use std::time::Instant;

use ash::vk;
use bevy::{
    app::{Plugin, Update},
    asset::{Asset, AssetApp, AssetEvent, Assets, Handle},
    ecs::{
        event::{EventReader, EventWriter},
        system::{lifetimeless::SRes, Res},
    },
    reflect::TypePath,
};

use crate::{
    ray_render_plugin::MainWorld,
    render_buffer::{Buffer, BufferProvider},
    shader::Shader,
    vk_utils,
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};

#[derive(Asset, TypePath, Debug, Clone)]
pub struct RaytracingPipeline {
    #[dependency]
    pub raygen_shader: Handle<Shader>,
    #[dependency]
    pub miss_shader: Handle<Shader>,
    #[dependency]
    pub hit_shader: Handle<Shader>,
}

pub type RTGroupHandle = [u8; 32];

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SBTRegionRaygen {
    pub handle: RTGroupHandle,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct SBTRegionMiss {
    pub handle: RTGroupHandle,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub enum SBTRegionHitEntry {
    Triangle(SBTRegionHitTriangle),
    Sphere(SBTRegionHitSphere),
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SBTRegionHitTriangle {
    pub handle: RTGroupHandle,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SBTRegionHitSphere {
    pub handle: RTGroupHandle,
}

#[derive(Default, Debug)]
pub struct SBT {
    pub raygen_region: vk::StridedDeviceAddressRegionKHR,
    pub miss_region: vk::StridedDeviceAddressRegionKHR,
    pub hit_region: vk::StridedDeviceAddressRegionKHR,
    pub data: Buffer<u8>,
}

pub struct CompiledRaytracingPipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_sets: [vk::DescriptorSet; 2],
    pub shader_binding_table: SBT,
}

impl VulkanAsset for RaytracingPipeline {
    type ExtractedAsset = (Shader, Shader, Shader);
    type ExtractParam = SRes<MainWorld>;
    type PreparedAsset = CompiledRaytracingPipeline;

    fn extract_asset(
        &self,
        param: &mut bevy::ecs::system::SystemParamItem<Self::ExtractParam>,
    ) -> Option<Self::ExtractedAsset> {
        let Some(raygen_shader) = param
            .0
            .get_resource::<Assets<crate::shader::Shader>>()
            .unwrap()
            .get(&self.raygen_shader)
        else {
            log::warn!("Raygen shader not ready yet");
            return None;
        };

        let Some(miss_shader) = param
            .0
            .get_resource::<Assets<crate::shader::Shader>>()
            .unwrap()
            .get(&self.miss_shader)
        else {
            log::warn!("Miss shader not ready yet");
            return None;
        };

        let Some(hit_shader) = param
            .0
            .get_resource::<Assets<crate::shader::Shader>>()
            .unwrap()
            .get(&self.hit_shader)
        else {
            log::warn!("Hit shader not ready yet");
            return None;
        };

        Some((
            raygen_shader.clone(),
            miss_shader.clone(),
            hit_shader.clone(),
        ))
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        render_device: &crate::render_device::RenderDevice,
    ) -> Self::PreparedAsset {
        let start = Instant::now();
        let (raygen_shader, miss_shader, hit_shader) = asset;

        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR),
        ];

        let descriptor_set_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

        let descriptor_set_layout = unsafe {
            render_device
                .create_descriptor_set_layout(&descriptor_set_layout_info, None)
                .unwrap()
        };

        let push_constant_info = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)
            .offset(0)
            .size(std::mem::size_of::<u64>() as u32);

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout))
            .push_constant_ranges(std::slice::from_ref(&push_constant_info));

        let pipeline_layout = unsafe {
            render_device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .unwrap()
        };

        let descriptor_sets = {
            let descriptor_pool = render_device.descriptor_pool.lock().unwrap();
            let layouts = [descriptor_set_layout, descriptor_set_layout];
            let alloc_info = vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(*descriptor_pool)
                .set_layouts(&layouts);
            unsafe {
                render_device
                    .allocate_descriptor_sets(&alloc_info)
                    .unwrap()
                    .try_into()
                    .unwrap()
            }
        };

        let shader_stages = [
            render_device.load_shader(&raygen_shader.spirv, vk::ShaderStageFlags::RAYGEN_KHR),
            render_device.load_shader(&miss_shader.spirv, vk::ShaderStageFlags::MISS_KHR),
            render_device.load_shader(&hit_shader.spirv, vk::ShaderStageFlags::CLOSEST_HIT_KHR),
        ];

        let shader_group = [
            // Raygen shader
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(0)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // Miss shader
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::GENERAL)
                .general_shader(1)
                .closest_hit_shader(vk::SHADER_UNUSED_KHR)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // Hit shader
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(2)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
        ];

        let pipeline_info = vk::RayTracingPipelineCreateInfoKHR::default()
            .stages(&shader_stages)
            .groups(&shader_group)
            .max_pipeline_ray_recursion_depth(1)
            .layout(pipeline_layout);

        let pipeline = unsafe {
            render_device
                .ext_rtx_pipeline
                .create_ray_tracing_pipelines(
                    vk::DeferredOperationKHR::null(),
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&pipeline_info),
                    None,
                )
                .unwrap()[0]
        };

        unsafe {
            for shader in shader_stages {
                render_device.destroy_shader_module(shader.module, None);
            }
        }

        let rtprops = vk_utils::get_raytracing_properties(&render_device);
        let handle_size = rtprops.shader_group_handle_size;
        assert!(
            handle_size as usize == std::mem::size_of::<RTGroupHandle>(),
            "at the time we only support 128-bit handles (at time of writing all devices have this)"
        );

        let handle_count = 3;
        let handle_data_size = handle_count * handle_size;
        let handles: Vec<RTGroupHandle> = unsafe {
            render_device
                .ext_rtx_pipeline
                .get_ray_tracing_shader_group_handles(
                    pipeline,
                    0,
                    handle_count,
                    handle_data_size as usize,
                )
                .unwrap()
                .chunks(handle_size as usize)
                .map(|chunk| {
                    let mut handle = RTGroupHandle::default();
                    handle.copy_from_slice(chunk);
                    handle
                })
                .collect()
        };

        let raygen_handle = handles[0];
        let miss_handle = handles[1];
        let hit_handle = handles[2];

        let handle_size_aligned = vk_utils::aligned_size(
            std::mem::size_of::<RTGroupHandle>() as u64,
            rtprops.shader_group_handle_alignment as u64,
        );

        let mut shader_binding_table = SBT::default();
        shader_binding_table.raygen_region.stride = vk_utils::aligned_size(
            handle_size_aligned,
            rtprops.shader_group_base_alignment as u64,
        );
        shader_binding_table.raygen_region.size = shader_binding_table.raygen_region.stride;

        shader_binding_table.miss_region.stride = handle_size_aligned as u64;
        shader_binding_table.miss_region.size = vk_utils::aligned_size(
            shader_binding_table.miss_region.stride,
            rtprops.shader_group_base_alignment as u64,
        );

        shader_binding_table.hit_region.stride = handle_size_aligned as u64;
        shader_binding_table.hit_region.size = vk_utils::aligned_size(
            shader_binding_table.hit_region.stride,
            rtprops.shader_group_base_alignment as u64,
        );

        let total_size = shader_binding_table.raygen_region.size
            + shader_binding_table.miss_region.size
            + shader_binding_table.hit_region.size;

        shader_binding_table.data = render_device
            .create_host_buffer(total_size, vk::BufferUsageFlags::SHADER_BINDING_TABLE_KHR);

        {
            let mut data = render_device.map_buffer(&mut shader_binding_table.data);
            unsafe {
                let mut dst: *mut u8 = data.as_ptr_mut();

                // raygen region (only a handle)
                (dst as *mut SBTRegionRaygen).write(SBTRegionRaygen {
                    handle: raygen_handle,
                });
                dst = dst.add(shader_binding_table.raygen_region.size as usize);

                // miss region (comes after the raygen region)
                (dst as *mut SBTRegionMiss).write(SBTRegionMiss {
                    handle: miss_handle,
                });
                dst = dst.add(shader_binding_table.miss_region.size as usize);

                // hit region (comes after the miss region)
                (dst as *mut SBTRegionHitTriangle)
                    .write(SBTRegionHitTriangle { handle: hit_handle });
            }
        }

        shader_binding_table.raygen_region.device_address = shader_binding_table.data.address;
        shader_binding_table.miss_region.device_address =
            shader_binding_table.data.address + shader_binding_table.raygen_region.size;
        shader_binding_table.hit_region.device_address = shader_binding_table.data.address
            + shader_binding_table.raygen_region.size
            + shader_binding_table.miss_region.size;

        log::info!("Raytracing pipeline compiled in {:?}", start.elapsed());

        CompiledRaytracingPipeline {
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_sets,
            shader_binding_table,
        }
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        render_device
            .destroyer
            .destroy_buffer(prepared_asset.shader_binding_table.data.handle);
        render_device
            .destroyer
            .destroy_pipeline_layout(prepared_asset.pipeline_layout);
        render_device
            .destroyer
            .destroy_pipeline(prepared_asset.pipeline);
        render_device
            .destroyer
            .destroy_descriptor_set_layout(prepared_asset.descriptor_set_layout);
    }
}

fn propagate_modified(
    filters: Res<Assets<RaytracingPipeline>>,
    mut shader_events: EventReader<AssetEvent<Shader>>,
    mut parent_events: EventWriter<AssetEvent<RaytracingPipeline>>,
) {
    for event in shader_events.read() {
        match event {
            AssetEvent::Modified { id } => {
                for (parent_id, filter) in filters.iter() {
                    if filter.raygen_shader.id() == *id
                        || filter.miss_shader.id() == *id
                        || filter.hit_shader.id() == *id
                    {
                        parent_events.send(AssetEvent::Modified {
                            id: parent_id.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

pub struct RaytracingPipelinePlugin;

impl Plugin for RaytracingPipelinePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_asset::<RaytracingPipeline>();
        app.init_vulkan_asset::<RaytracingPipeline>();
        app.add_systems(Update, propagate_modified);
    }
}
