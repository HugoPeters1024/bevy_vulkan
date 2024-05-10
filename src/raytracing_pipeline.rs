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
use bytemuck::{Pod, Zeroable};

use crate::{
    ray_render_plugin::MainWorld,
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
    #[dependency]
    pub sphere_intersection_shader: Handle<Shader>,
    #[dependency]
    pub sphere_hit_shader: Handle<Shader>,
}

pub type RTGroupHandle = [u8; 32];

pub struct CompiledRaytracingPipeline {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_sets: [vk::DescriptorSet; 2],
    pub raygen_handle: RTGroupHandle,
    pub miss_handle: RTGroupHandle,
    pub hit_handle: RTGroupHandle,
    pub sphere_hit_handle: RTGroupHandle,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct RaytracingPushConstants {
    pub uniform_buffer: u64,
    pub material_buffer: u64,
    pub bluenoise_buffer: u64,
    pub unpacked_bluenoise_buffer: u64,
    pub focus_buffer: u64,
    pub sky_texture: u64,
}

impl VulkanAsset for RaytracingPipeline {
    type ExtractedAsset = (Shader, Shader, Shader, Shader, Shader);
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

        let Some(sphere_intersection_shader) = param
            .0
            .get_resource::<Assets<crate::shader::Shader>>()
            .unwrap()
            .get(&self.sphere_intersection_shader)
        else {
            log::warn!("Sphere intersection shader not ready yet");
            return None;
        };

        let Some(sphere_hit_shader) = param
            .0
            .get_resource::<Assets<crate::shader::Shader>>()
            .unwrap()
            .get(&self.sphere_hit_shader)
        else {
            log::warn!("Sphere hit shader not ready yet");
            return None;
        };

        Some((
            raygen_shader.clone(),
            miss_shader.clone(),
            hit_shader.clone(),
            sphere_intersection_shader.clone(),
            sphere_hit_shader.clone(),
        ))
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        render_device: &crate::render_device::RenderDevice,
    ) -> Self::PreparedAsset {
        let start = Instant::now();
        let (raygen_shader, miss_shader, hit_shader, sphere_intersection_shader, sphere_hit_shader) =
            asset;

        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR),
            vk::DescriptorSetLayoutBinding::default()
                .binding(100)
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
            .stage_flags(vk::ShaderStageFlags::ALL)
            .offset(0)
            .size(std::mem::size_of::<RaytracingPushConstants>() as u32);

        let set_layouts = [
            descriptor_set_layout,
            render_device.bindless_descriptor_set_layout,
        ];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&set_layouts)
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
            render_device.load_shader(
                &raygen_shader.spirv.unwrap(),
                vk::ShaderStageFlags::RAYGEN_KHR,
            ),
            render_device.load_shader(&miss_shader.spirv.unwrap(), vk::ShaderStageFlags::MISS_KHR),
            render_device.load_shader(
                &hit_shader.spirv.unwrap(),
                vk::ShaderStageFlags::CLOSEST_HIT_KHR,
            ),
            render_device.load_shader(
                &sphere_intersection_shader.spirv.unwrap(),
                vk::ShaderStageFlags::INTERSECTION_KHR,
            ),
            render_device.load_shader(
                &sphere_hit_shader.spirv.unwrap(),
                vk::ShaderStageFlags::CLOSEST_HIT_KHR,
            ),
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
            // Triangle hit shader
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::TRIANGLES_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(2)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(vk::SHADER_UNUSED_KHR),
            // Sphere shader
            vk::RayTracingShaderGroupCreateInfoKHR::default()
                .ty(vk::RayTracingShaderGroupTypeKHR::PROCEDURAL_HIT_GROUP)
                .general_shader(vk::SHADER_UNUSED_KHR)
                .closest_hit_shader(4)
                .any_hit_shader(vk::SHADER_UNUSED_KHR)
                .intersection_shader(3),
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

        let handle_count = 4;
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
        let sphere_hit_handle = handles[3];

        log::info!("Raytracing pipeline compiled in {:?}", start.elapsed());

        CompiledRaytracingPipeline {
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_sets,
            raygen_handle,
            miss_handle,
            hit_handle,
            sphere_hit_handle,
        }
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        render_device
            .destroyer
            .destroy_descriptor_set_layout(prepared_asset.descriptor_set_layout);
        render_device
            .destroyer
            .destroy_pipeline_layout(prepared_asset.pipeline_layout);
        render_device
            .destroyer
            .destroy_pipeline(prepared_asset.pipeline);
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
                        || filter.sphere_intersection_shader.id() == *id
                        || filter.sphere_hit_shader.id() == *id
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
