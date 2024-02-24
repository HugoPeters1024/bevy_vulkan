use ash::vk;
use bevy::{ecs::system::lifetimeless::SRes, prelude::*};

use crate::{
    ray_render_plugin::MainWorld,
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};

#[derive(Asset, TypePath, Debug, Clone)]
pub struct PostProcessFilter {
    #[dependency]
    pub vertex_shader: Handle<crate::shader::Shader>,
    #[dependency]
    pub fragment_shader: Handle<crate::shader::Shader>,
}

#[derive(Default)]
pub struct PostProcessFilterLoader;

pub struct CompiledPostProcessFilter {
    pub pipeline: vk::Pipeline,
}

impl VulkanAsset for PostProcessFilter {
    type ExtractedAsset = (crate::shader::Shader, crate::shader::Shader);
    type ExtractParam = SRes<MainWorld>;
    type PreparedAsset = CompiledPostProcessFilter;

    fn extract_asset(
        &self,
        param: &mut bevy::ecs::system::SystemParamItem<Self::ExtractParam>,
    ) -> Option<Self::ExtractedAsset> {
        let Some(vertex_shader) = param
            .0
            .get_resource::<Assets<crate::shader::Shader>>()
            .unwrap()
            .get(&self.vertex_shader)
        else {
            log::warn!("Vertex shader not ready yet");
            return None;
        };

        let Some(fragment_shader) = param
            .0
            .get_resource::<Assets<crate::shader::Shader>>()
            .unwrap()
            .get(&self.fragment_shader)
        else {
            log::warn!("Fragment shader not ready yet");
            return None;
        };

        Some((vertex_shader.clone(), fragment_shader.clone()))
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        render_device: &crate::render_device::RenderDevice,
    ) -> Self::PreparedAsset {
        let (vertex_shader, fragment_shader) = asset;
        let shader_stages = [
            render_device.load_shader(&vertex_shader.spirv, vk::ShaderStageFlags::VERTEX),
            render_device.load_shader(&fragment_shader.spirv, vk::ShaderStageFlags::FRAGMENT),
        ];

        let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
            .dynamic_states(&[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR]);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterization_state = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(vk::CullModeFlags::NONE);

        let multisample_state = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA);

        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&color_blend_attachment));

        let layout_info = vk::PipelineLayoutCreateInfo::default();
        let pipeline_layout = unsafe {
            render_device
                .create_pipeline_layout(&layout_info, None)
                .unwrap()
        };

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input_state)
            .input_assembly_state(&input_assembly_state)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization_state)
            .multisample_state(&multisample_state)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout);

        let pipeline = unsafe {
            render_device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                &[pipeline_info],
                None,
            )
        }
        .unwrap()[0];

        unsafe {
            render_device.destroy_shader_module(shader_stages[0].module, None);
            render_device.destroy_shader_module(shader_stages[1].module, None);
            render_device.destroy_pipeline_layout(pipeline_layout, None);
        }

        CompiledPostProcessFilter { pipeline }
    }
    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        unsafe {
            render_device.destroy_pipeline(prepared_asset.pipeline, None);
        }
    }
}

pub struct PostProcessFilterPlugin;

fn propagate_modified(
    filters: Res<Assets<PostProcessFilter>>,
    mut shader_events: EventReader<AssetEvent<crate::shader::Shader>>,
    mut parent_events: EventWriter<AssetEvent<PostProcessFilter>>,
) {
    for event in shader_events.read() {
        match event {
            AssetEvent::Modified { id } => {
                for (parent_id, filter) in filters.iter() {
                    if filter.vertex_shader.id() == *id || filter.fragment_shader.id() == *id {
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

impl Plugin for PostProcessFilterPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<PostProcessFilter>();
        app.init_vulkan_asset::<PostProcessFilter>();
        app.add_systems(Update, propagate_modified);
    }
}
