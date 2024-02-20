use ash::vk;
use bevy::{asset::{AssetLoader, AsyncReadExt}, prelude::*};
use serde::Deserialize;
use thiserror::Error;

use crate::vulkan_asset::VulkanAsset;

#[derive(Debug, Deserialize)]
struct PostProcessFilterRaw {
    pub vertex_shader: String,
    pub fragment_shader: String,
}

#[derive(Asset, TypePath, Debug, Clone)]
pub struct PostProcessFilter {
    pub vertex_shader: Handle<crate::shader::Shader>,
    pub fragment_shader: Handle<crate::shader::Shader>,
}

#[derive(Default)]
pub struct PostProcessFilterLoader;

pub struct CompiledPostProcessFilter {
    pub pipeline: vk::Pipeline,
    pub pipeline_layout: vk::PipelineLayout,
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum PostProcessFilterError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ron error: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

impl AssetLoader for PostProcessFilterLoader {
    type Asset = PostProcessFilter;

    type Settings = ();

    type Error = PostProcessFilterError;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            let raw: PostProcessFilterRaw = ron::de::from_bytes(&bytes)
                .map_err(|e| PostProcessFilterError::from(e))?;

            let vertex_shader = load_context.load(raw.vertex_shader);
            let fragment_shader = load_context.load(raw.fragment_shader);

            Ok(PostProcessFilter {
                vertex_shader,
                fragment_shader,
            })
        })
    }

    fn extensions(&self) -> &[&str] {
        &["pipeline"]
    }
}

impl VulkanAsset for PostProcessFilter {
    type PreparedAsset = ();

    fn prepare_asset(
        self,
        render_device: &crate::render_device::RenderDevice,
    ) -> Self::PreparedAsset {
        ()
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
    }
}

struct PostProcessFilterPlugin;

impl Plugin for PostProcessFilterPlugin {
    fn build(&self, app: &mut App) {
         app.init_asset_loader::<PostProcessFilterLoader>();
    }
}
