use bevy::{
    asset::{AssetLoader, AsyncReadExt},
    prelude::*,
    render::RenderApp,
};
use thiserror::Error;

use crate::{
    blas::{build_blas_from_buffers, GeometryDescr, Vertex, BLAS},
    extract::Extract,
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};

pub struct GltfPlugin;

impl Plugin for GltfPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Gltf>();
        app.init_asset_loader::<GltfLoader>();
        app.init_vulkan_asset::<Gltf>();

        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(ExtractSchedule, extract_gltfs);
    }
}

#[derive(Asset, TypePath, Debug, Clone)]
pub struct Gltf {
    pub document: gltf::Document,
    pub buffers: Vec<gltf::buffer::Data>,
    pub images: Vec<gltf::image::Data>,
}

impl Gltf {
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
    type Asset = Gltf;
    type Settings = ();
    type Error = GltfLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy::asset::io::Reader,
        _settings: &'a Self::Settings,
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::utils::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            let (document, buffers, images) = gltf::import_slice(bytes)?;

            let asset = Gltf {
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

impl VulkanAsset for Gltf {
    type ExtractedAsset = Gltf;
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

        let mut vertex_buffer = vec![Vertex::default(); vertex_count];
        let mut index_buffer = vec![0; index_count];

        let geometries = extract_mesh_data(&asset, &mut vertex_buffer, &mut index_buffer);

        build_blas_from_buffers(
            render_device,
            vertex_count,
            index_count,
            bytemuck::cast_slice(&vertex_buffer),
            bytemuck::cast_slice(&index_buffer),
            &geometries,
        )
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
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
    gltf: &Gltf,
    vertex_buffer: &mut [Vertex],
    index_buffer: &mut [u32],
) -> Vec<GeometryDescr> {
    let mesh = gltf.single_mesh();
    let mut geometries = Vec::new();
    let mut vertex_buffer_head = 0;
    let mut index_buffer_head = 0;
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
        geometries.push(geometry);
    }

    geometries
}

fn extract_gltfs(
    mut commands: Commands,
    meshes: Extract<Query<(&Handle<Gltf>, &Transform, &GlobalTransform)>>,
) {
    for (mesh, t, gt) in meshes.iter() {
        commands.spawn((mesh.clone(), t.clone(), gt.clone()));
    }
}
