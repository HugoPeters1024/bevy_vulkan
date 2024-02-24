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
    shader::Shader,
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

pub struct CompileRaytracingPipeline {
    pub pipeline: vk::Pipeline,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
}

impl VulkanAsset for RaytracingPipeline {
    type ExtractedAsset = (Shader, Shader, Shader);
    type ExtractParam = SRes<MainWorld>;
    type PreparedAsset = CompileRaytracingPipeline;

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

        let bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::RAYGEN_KHR)];

        let descriptor_set_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

        let descriptor_set_layout = unsafe {
            render_device
                .create_descriptor_set_layout(&descriptor_set_layout_info, None)
                .unwrap()
        };

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(std::slice::from_ref(&descriptor_set_layout));

        let pipeline_layout = unsafe {
            render_device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .unwrap()
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
            render_device.destroy_pipeline_layout(pipeline_layout, None);
        }

        log::info!("Raytracing pipeline compiled in {:?}", start.elapsed());

        CompileRaytracingPipeline {
            pipeline,
            descriptor_set_layout,
        }
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        unsafe {
            render_device.destroy_pipeline(prepared_asset.pipeline, None);
            render_device.destroy_descriptor_set_layout(prepared_asset.descriptor_set_layout, None);
        }
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
