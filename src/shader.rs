use ash::vk;
use std::{borrow::Cow, cell::RefCell, fs::read_to_string, rc::Rc};
use thiserror::Error;

use bevy::{asset::AssetLoader, prelude::*, utils::ConditionalSendFuture};

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
    pub spirv: Option<Cow<'static, [u8]>>,
    #[dependency]
    pub dependencies: Vec<Handle<Shader>>,
}

impl AssetLoader for ShaderLoader {
    type Asset = Shader;
    type Settings = ();
    type Error = ShaderLoaderError;

    fn extensions(&self) -> &[&str] {
        &["vert", "frag"]
    }

    fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext,
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let ext = load_context.path().extension().unwrap().to_str().unwrap();
            let path = load_context.asset_path().to_string();
            // On windows, the path will inconsistently use \ or /.
            // TODO: remove this once AssetPath forces cross-platform "slash" consistency. See #10511
            let path = path.replace(std::path::MAIN_SEPARATOR, "/");
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            if ext == "glsl" {
                return Ok(Shader {
                    path: load_context.path().to_str().unwrap().to_string(),
                    spirv: None,
                    dependencies: Vec::new(),
                });
            }

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
            options.set_generate_debug_info();
            options.set_optimization_level(shaderc::OptimizationLevel::Performance);

            let load_context = Rc::new(RefCell::new(load_context));
            let load_context_copy = load_context.clone();
            let dependencies = Rc::new(RefCell::new(Vec::new()));
            let dependencies_copy = dependencies.clone();

            options.set_include_callback(move |fname, _type, _, _depth| {
                let full_path = format!("./assets/shaders/{}", fname);
                let Ok(contents) = read_to_string(full_path.clone()) else {
                    return Err(format!("Failed to read shader include: {}", fname));
                };

                dependencies_copy.borrow_mut().push(
                    load_context_copy
                        .borrow_mut()
                        .load::<Shader>(format!("shaders/{}", fname)),
                );

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

            let dependencies = dependencies.borrow().clone();

            let shader = Shader {
                path: load_context.borrow().path().to_str().unwrap().to_string(),
                spirv: Some(Vec::from(binary.as_binary_u8()).into()),
                dependencies,
            };

            log::info!("Loaded shader: {:?}", shader.path);
            Ok(shader)
        })
    }
}

pub struct ShaderPlugin;

impl Plugin for ShaderPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<crate::shader::Shader>();
        app.init_asset_loader::<crate::shader::ShaderLoader>();

        app.add_systems(Update, reload_modified);
    }
}

fn reload_modified(
    shaders: Res<Assets<Shader>>,
    asset_server: Res<AssetServer>,
    mut shader_events: EventReader<AssetEvent<Shader>>,
) {
    for event in shader_events.read() {
        match event {
            AssetEvent::Modified { id } => {
                for (parent_id, shader) in shaders.iter() {
                    if shader.dependencies.iter().any(|dep| dep.id() == *id) {
                        asset_server.reload(asset_server.get_path(parent_id).unwrap());
                    }
                }
            }
            _ => {}
        }
    }
}
