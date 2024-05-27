use bevy::{
    prelude::*,
    render::{mesh::Indices, RenderApp},
};

use crate::{
    blas::{build_blas_from_buffers, GeometryDescr, Vertex, BLAS},
    extract::Extract,
    render_buffer::BufferProvider,
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};
use ash::vk;

impl VulkanAsset for Mesh {
    type ExtractedAsset = Mesh;
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
        let vertex_count = asset.count_vertices();
        assert!(matches!(asset.indices(), Some(Indices::U32(_))));
        let index_count = match asset.indices() {
            Some(Indices::U32(indices)) => indices.len(),
            Some(Indices::U16(indices)) => indices.len(),
            None => panic!("Mesh has no indices"),
        };

        let attributes = asset.attributes().map(|(id, _)| id).collect::<Vec<_>>();
        assert!(attributes.len() == 3);

        let vertex_data = asset.get_vertex_buffer_data();
        let index_data = asset.get_index_buffer_bytes().unwrap();

        let mut vertex_buffer_host = render_device.create_host_buffer::<Vertex>(
            vertex_count as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        let mut index_buffer_host = render_device.create_host_buffer::<u32>(
            index_count as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        );

        let mut vertex_view = render_device.map_buffer(&mut vertex_buffer_host);
        vertex_view.copy_from_slice(bytemuck::cast_slice(&vertex_data));
        let mut index_view = render_device.map_buffer(&mut index_buffer_host);
        index_view.copy_from_slice(bytemuck::cast_slice(&index_data));

        build_blas_from_buffers(
            render_device,
            vertex_count,
            index_count,
            vertex_buffer_host,
            index_buffer_host,
            &[GeometryDescr {
                first_vertex: 0,
                vertex_count,
                first_index: 0,
                index_count,
            }],
        )
    }

    fn destroy_asset(
        render_device: &crate::render_device::RenderDevice,
        prepared_asset: &Self::PreparedAsset,
    ) {
        prepared_asset.destroy(render_device);
    }
}

pub struct VulkanMeshPlugin;

fn extract_meshes(
    mut commands: Commands,
    meshes: Extract<
        Query<(
            &Handle<Mesh>,
            &Handle<StandardMaterial>,
            &Transform,
            &GlobalTransform,
        )>,
    >,
) {
    for (mesh, mat, t, gt) in meshes.iter() {
        commands.spawn((mesh.clone(), mat.clone(), t.clone(), gt.clone()));
    }
}

impl Plugin for VulkanMeshPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Mesh>();
        app.init_vulkan_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_vulkan_asset::<StandardMaterial>();

        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        render_app.add_systems(ExtractSchedule, extract_meshes);
    }
}
