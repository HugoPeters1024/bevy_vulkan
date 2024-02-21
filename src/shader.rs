use ash::{util::read_spv, vk};
use std::{borrow::Cow, fs::read_to_string, io::Cursor};
use thiserror::Error;

use bevy::{
    asset::{AssetLoader, AsyncReadExt},
    ecs::system::lifetimeless::SRes,
    prelude::*,
};

use crate::vulkan_asset::*;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ShaderLoaderError {
    #[error("Could not load shader: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not parse shader: {0}")]
    Parse(#[from] std::string::FromUtf8Error),
    #[error("Could not compile shader: {0}")]
    Compile(#[from] shaderc::Error),
}

pub struct ShaderLoader {
    compiler: shaderc::Compiler,
}

impl Default for ShaderLoader {
    fn default() -> Self {
        Self {
            compiler: shaderc::Compiler::new().unwrap(),
        }
    }
}

#[derive(Asset, TypePath, Debug, Clone)]
pub struct Shader {
    pub path: String,
    pub spirv: Cow<'static, [u8]>,
    pub ty: shaderc::ShaderKind,
}

impl AssetLoader for ShaderLoader {
    type Asset = Shader;
    type Settings = ();
    type Error = ShaderLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let ext = load_context.path().extension().unwrap().to_str().unwrap();
            let path = load_context.asset_path().to_string();
            // On windows, the path will inconsistently use \ or /.
            // TODO: remove this once AssetPath forces cross-platform "slash" consistency. See #10511
            let path = path.replace(std::path::MAIN_SEPARATOR, "/");
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            let kind = match ext {
                "vert" => shaderc::ShaderKind::Vertex,
                "frag" => shaderc::ShaderKind::Fragment,
                "comp" => shaderc::ShaderKind::Compute,
                "rgen" => shaderc::ShaderKind::RayGeneration,
                "rint" => shaderc::ShaderKind::Intersection,
                "rchit" => shaderc::ShaderKind::ClosestHit,
                "rmiss" => shaderc::ShaderKind::Miss,
                _ => panic!("Unsupported shader extension: {}", ext),
            };

            let mut options = shaderc::CompileOptions::new().unwrap();
            options.set_target_env(shaderc::TargetEnv::Vulkan, vk::make_api_version(0, 1, 3, 0));
            options.set_target_spirv(shaderc::SpirvVersion::V1_6);
            options.set_optimization_level(shaderc::OptimizationLevel::Performance);

            options.set_include_callback(|fname, _type, _, _depth| {
                let full_path = format!("./assets/shaders/{}", fname);
                let Ok(contents) = read_to_string(full_path.clone()) else {
                    return Err(format!("Failed to read shader include: {}", fname));
                };

                Ok(shaderc::ResolvedInclude {
                    resolved_name: fname.to_string(),
                    content: contents,
                })
            });

            let binary_result = self.compiler.compile_into_spirv(
                std::str::from_utf8(&bytes).unwrap(),
                kind,
                path.as_str(),
                "main",
                Some(&options),
            );

            let Ok(binary) = binary_result else {
                let e = binary_result.err().unwrap();
                return Err(ShaderLoaderError::Compile(e));
            };

            let shader = Shader {
                path: load_context.path().to_str().unwrap().to_string(),
                spirv: Vec::from(binary.as_binary_u8()).into(),
                ty: kind,
            };

            log::info!("Loaded shader: {:?}", shader.path);
            Ok(shader)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["vert", "frag"]
    }
}

impl VulkanAsset for Shader {
    type ExtractedAsset = Shader;
    type ExtractParam = ();
    type PreparedAsset = vk::ShaderModule;

    fn extract_asset(
        &self,
        _param: &mut bevy::ecs::system::SystemParamItem<Self::ExtractParam>,
    ) -> Option<Self::ExtractedAsset> {
        Some(self.clone())
    }

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        render_device: &crate::render_device::RenderDevice,
    ) -> Self::PreparedAsset {
        let code = read_spv(&mut Cursor::new(&asset.spirv)).unwrap();
        unsafe {
            render_device
                .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)
                .unwrap()
        }
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        unsafe {
            render_device.destroy_shader_module(*prepared_asset, None);
        }
    }
}
