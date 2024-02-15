use ash::vk;
use bevy::{asset::AssetLoader, prelude::*};
use thiserror::Error;

use crate::vulkan_asset::VulkanAsset;

#[derive(Asset, TypePath, Debug, Clone)]
struct PostProcessFilter {
    pub vertex_shader: Handle<crate::shader::Shader>,
    pub fragment_shader: Handle<crate::shader::Shader>,
}

struct PostProcessFilterLoader;

struct CompiledPostProcessFilter {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum PostProcessFilterError {}

impl AssetLoader for PostProcessFilterLoader {
    type Asset = PostProcessFilter;

    type Settings = ();

    type Error = PostProcessFilterError;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        todo!()
    }

    fn extensions(&self) -> &[&str] {
        todo!()
    }
}

impl VulkanAsset for PostProcessFilter {
    type PreparedAsset = CompiledPostProcessFilter;

    fn prepare_asset(
        self,
        render_device: &crate::render_device::RenderDevice,
    ) -> Self::PreparedAsset {
        todo!()
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        todo!()
    }
}
