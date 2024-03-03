use ash::vk;
use bevy::{
    prelude::*,
    render::{mesh::Indices, RenderApp},
};

use crate::{
    blas::{build_blas_from_buffers, GeometryDescr, BLAS},
    extract::Extract,
    vulkan_asset::{VulkanAsset, VulkanAssetExt},
};

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

        build_blas_from_buffers(
            render_device,
            vertex_count,
            index_count,
            &vertex_data,
            &index_data,
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
    meshes: Extract<Query<(&Handle<Mesh>, &Transform, &GlobalTransform)>>,
) {
    for (mesh, t, gt) in meshes.iter() {
        commands.spawn((mesh.clone(), t.clone(), gt.clone()));
    }
}

impl Plugin for VulkanMeshPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();

        app.init_vulkan_asset::<Mesh>();

        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        render_app.add_systems(ExtractSchedule, extract_meshes);
    }
}
