use std::collections::HashMap;

use ash::vk;
use bevy::{
    asset::AssetLoader,
    prelude::*,
    render::RenderApp, tasks::ConditionalSendFuture,
};
use thiserror::Error;

use crate::{
    blas::{build_blas_from_buffers, GeometryDescr, RTXMaterial, Vertex, BLAS},
    extract::Extract,
    render_buffer::{Buffer, BufferProvider},
    render_device::RenderDevice,
    render_env::{DEFAULT_NORMAL_TEXTURE_IDX, WHITE_TEXTURE_IDX},
    render_texture::{load_texture_from_bytes, padd_pixel_bytes_rgba_unorm, RenderTexture},
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};

pub struct GltfPlugin;

impl Plugin for GltfPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<GltfModel>();
        app.init_asset_loader::<GltfLoader>();
        app.init_vulkan_asset::<GltfModel>();

        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(ExtractSchedule, extract_gltfs);
    }
}

#[derive(Asset, TypePath, Debug, Clone)]
pub struct GltfModel {
    pub document: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    pub images: Vec<gltf::image::Data>,
}

#[derive(Component, Deref, Clone)]
pub struct GltfModelHandle(pub Handle<GltfModel>);

impl GltfModel {
    pub fn single_mesh(&self) -> gltf::Mesh {
        let scene = self.document.default_scene().unwrap();
        let mut node = scene.nodes().next().unwrap();
        while node.mesh().is_none() {
            node = node.children().next().unwrap();
        }

        return node.mesh().unwrap();
    }
}

#[derive(Default)]
pub struct GltfLoader;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum GltfLoaderError {
    #[error("Could not load gltf: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not load gltf: {0}")]
    GltfLoadError(#[from] gltf::Error),
    #[error("Could not parse gltf: {0}")]
    Parse(#[from] std::string::FromUtf8Error),
}

impl AssetLoader for GltfLoader {
    type Asset = GltfModel;
    type Settings = ();
    type Error = GltfLoaderError;

    fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext,
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            let (document, buffers, images) = gltf::import_slice(bytes)?;

            let asset = GltfModel {
                document,
                buffers,
                images,
            };

            log::info!(
                "gltf {} has {} chunks of buffer data",
                load_context.path().display(),
                asset.buffers.len()
            );
            log::info!(
                "gltf {} has {} chunks of image data",
                load_context.path().display(),
                asset.images.len()
            );

            Ok(asset)
        })
    }
}

impl VulkanAsset for GltfModel {
    type ExtractedAsset = GltfModel;
    type ExtractParam = ();
    type PreparedAsset = BLAS;

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
        let mesh = asset.single_mesh();
        let (vertex_count, index_count) = extract_mesh_sizes(&mesh);

        let mut textures = Vec::new();

        let mut vertex_buffer_host: Buffer<Vertex> = render_device.create_host_buffer(
            vertex_count as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        let mut index_buffer_host: Buffer<u32> = render_device.create_host_buffer(
            index_count as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        let mut vertex_buffer_mapped = render_device.map_buffer(&mut vertex_buffer_host);
        let mut index_buffer_mapped = render_device.map_buffer(&mut index_buffer_host);

        let geometries_and_materials = extract_mesh_data(
            render_device,
            &asset,
            vertex_buffer_mapped.as_slice_mut(),
            index_buffer_mapped.as_slice_mut(),
            &mut textures,
        );

        let (geometries, materials): (Vec<_>, Vec<_>) =
            geometries_and_materials.into_iter().unzip();

        assert!(geometries.len() == materials.len());

        let mut blas = build_blas_from_buffers(
            render_device,
            vertex_count,
            index_count,
            vertex_buffer_host,
            index_buffer_host,
            &geometries,
        );

        blas.gltf_materials = Some(materials);
        blas.gltf_textures = Some(textures);
        blas
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        if let Some(gltf_textures) = &prepared_asset.gltf_textures {
            for texture in gltf_textures {
                bevy::prelude::Image::destroy_asset(render_device, texture);
            }
        }

        prepared_asset.destroy(render_device);
    }
}

fn extract_mesh_sizes(mesh: &gltf::Mesh) -> (usize, usize) {
    let mut vertex_count = 0;
    let mut index_count = 0;
    for primitive in mesh.primitives() {
        let positions = primitive
            .attributes()
            .find_map(|(s, a)| {
                if s == gltf::Semantic::Positions {
                    Some(a)
                } else {
                    None
                }
            })
            .unwrap();
        vertex_count += positions.count();

        index_count += primitive.indices().unwrap().count();
    }
    (vertex_count, index_count)
}

fn extract_mesh_data(
    render_device: &RenderDevice,
    gltf: &GltfModel,
    vertex_buffer: &mut [Vertex],
    index_buffer: &mut [u32],
    textures: &mut Vec<RenderTexture>,
) -> Vec<(GeometryDescr, RTXMaterial)> {
    let mesh = gltf.single_mesh();
    let mut geometries = Vec::new();
    let mut vertex_buffer_head = 0;
    let mut index_buffer_head = 0;
    let mut loaded_textures: HashMap<usize, RenderTexture> = HashMap::new();

    let mut load_cached_texture = |image_idx: usize| {
        if let Some(res) = loaded_textures.get(&image_idx) {
            return render_device.get_bindless_texture_index(&res).unwrap();
        }

        let Some(image) = load_gltf_texture(&render_device, gltf, image_idx) else {
            return WHITE_TEXTURE_IDX;
        };

        render_device.register_bindless_texture(&image);
        textures.push(image.clone());
        loaded_textures.insert(image_idx, image);
        return render_device
            .get_bindless_texture_index(loaded_textures.get(&image_idx).unwrap())
            .expect("Impossible");
    };

    for primitive in mesh.primitives() {
        let positions = primitive
            .attributes()
            .find_map(|(s, a)| {
                if s == gltf::Semantic::Positions {
                    Some(a)
                } else {
                    None
                }
            })
            .unwrap();
        let indices = primitive.indices().unwrap();

        let geometry = GeometryDescr {
            first_vertex: vertex_buffer_head,
            vertex_count: positions.count(),
            first_index: index_buffer_head,
            index_count: indices.count(),
        };

        let mut emissive_factor = [0.0; 4];
        emissive_factor[0] = primitive.material().emissive_factor()[0];
        emissive_factor[1] = primitive.material().emissive_factor()[1];
        emissive_factor[2] = primitive.material().emissive_factor()[2];

        let specular_transmission_factor = primitive
            .material()
            .transmission()
            .map_or(0.0, |t| t.transmission_factor());

        let base_color_texture = primitive
            .material()
            .pbr_metallic_roughness()
            .base_color_texture()
            .map(|texture| load_cached_texture(texture.texture().source().index()))
            .unwrap_or(WHITE_TEXTURE_IDX);

        let base_emissive_texture = primitive
            .material()
            .emissive_texture()
            .map(|texture| load_cached_texture(texture.texture().source().index()))
            .unwrap_or(WHITE_TEXTURE_IDX);

        let normal_texture = primitive
            .material()
            .normal_texture()
            .map(|texture| load_cached_texture(texture.texture().source().index()))
            .unwrap_or(DEFAULT_NORMAL_TEXTURE_IDX);

        let specular_transmission_texture =
            primitive
                .material()
                .transmission()
                .map_or(WHITE_TEXTURE_IDX, |t| {
                    t.transmission_texture()
                        .map(|texture| load_cached_texture(texture.texture().source().index()))
                        .unwrap_or(WHITE_TEXTURE_IDX)
                });

        let metallic_roughness_texture = primitive
            .material()
            .pbr_metallic_roughness()
            .metallic_roughness_texture()
            .map(|texture| load_cached_texture(texture.texture().source().index()))
            .unwrap_or(WHITE_TEXTURE_IDX);

        let material = RTXMaterial {
            base_color_factor: primitive
                .material()
                .pbr_metallic_roughness()
                .base_color_factor(),
            base_emissive_factor: emissive_factor,
            base_color_texture,
            base_emissive_texture,
            normal_texture,
            specular_transmission_texture,
            metallic_roughness_texture,
            specular_transmission_factor,
            roughness_factor: primitive
                .material()
                .pbr_metallic_roughness()
                .roughness_factor(),
            metallic_factor: primitive
                .material()
                .pbr_metallic_roughness()
                .metallic_factor(),
            refract_index: primitive.material().ior().unwrap_or(1.0),
            __padding: [0; 12],
        };

        let reader = primitive.reader(|buffer| Some(&gltf.buffers[buffer.index()]));
        let pos_reader = reader.read_positions().unwrap();

        assert!(pos_reader.len() == geometry.vertex_count);

        for (i, pos) in pos_reader.enumerate() {
            vertex_buffer[geometry.first_vertex + i].position[0] = pos[0];
            vertex_buffer[geometry.first_vertex + i].position[1] = pos[1];
            vertex_buffer[geometry.first_vertex + i].position[2] = pos[2];
        }

        let normal_reader = reader.read_normals().unwrap();
        assert!(normal_reader.len() == geometry.vertex_count);

        for (i, normal) in normal_reader.enumerate() {
            if normal[0].is_nan() || normal[1].is_nan() || normal[2].is_nan() {
                vertex_buffer[geometry.first_vertex + i].normal[0] = 0.0;
                vertex_buffer[geometry.first_vertex + i].normal[1] = 0.0;
                vertex_buffer[geometry.first_vertex + i].normal[2] = 0.0;
                continue;
            }

            if (1.0
                - (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt())
            .abs()
                > 0.01
            {
                vertex_buffer[geometry.first_vertex + i].normal[0] = 1.0;
                vertex_buffer[geometry.first_vertex + i].normal[1] = 0.0;
                vertex_buffer[geometry.first_vertex + i].normal[2] = 0.0;
                continue;
            }

            vertex_buffer[geometry.first_vertex + i].normal[0] = normal[0];
            vertex_buffer[geometry.first_vertex + i].normal[1] = normal[1];
            vertex_buffer[geometry.first_vertex + i].normal[2] = normal[2];
        }

        if let Some(uv_reader) = reader.read_tex_coords(0).map(|r| r.into_f32()) {
            for (i, uv) in uv_reader.enumerate() {
                vertex_buffer[geometry.first_vertex + i].uv[0] = uv[0];
                vertex_buffer[geometry.first_vertex + i].uv[1] = uv[1];
            }
        }

        let index_reader = reader.read_indices().unwrap().into_u32();
        assert!(index_reader.len() == geometry.index_count);
        assert!(geometry.index_count % 3 == 0);

        for (i, index) in index_reader.enumerate() {
            index_buffer[geometry.first_index + i] = index + vertex_buffer_head as u32;
        }

        vertex_buffer_head += geometry.vertex_count;
        index_buffer_head += geometry.index_count;
        geometries.push((geometry, material));
    }

    geometries
}

fn load_gltf_texture(
    device: &RenderDevice,
    asset: &GltfModel,
    image_idx: usize,
) -> Option<RenderTexture> {
    let image = &asset.images[image_idx];
    let (bytes, format) = match image.format {
        gltf::image::Format::R8G8B8A8 => (image.pixels.clone(), vk::Format::R8G8B8A8_UNORM),
        gltf::image::Format::R8G8B8 => (
            padd_pixel_bytes_rgba_unorm(
                &image.pixels,
                3,
                image.width as usize,
                image.height as usize,
            ),
            vk::Format::R8G8B8A8_UNORM,
        ),
        gltf::image::Format::R8 => (
            padd_pixel_bytes_rgba_unorm(
                &image.pixels,
                1,
                image.width as usize,
                image.height as usize,
            ),
            vk::Format::R8G8B8A8_UNORM,
        ),
        _ => {
            log::warn!(
                "WARNING: Unsupported texture format {:?}, ignoring...",
                image.format
            );
            return None;
        }
    };

    Some(load_texture_from_bytes(
        device,
        format,
        vk::ImageUsageFlags::SAMPLED,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        &bytes,
        image.width,
        image.height,
    ))
}

fn extract_gltfs(
    mut commands: Commands,
    meshes: Extract<Query<(&GltfModelHandle, &Transform, &GlobalTransform)>>,
) {
    for (mesh, t, gt) in meshes.iter() {
        commands.spawn((mesh.clone(), t.clone(), gt.clone()));
    }
}
