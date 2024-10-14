#![feature(iter_array_chunks)]
pub mod blas;
pub mod debug_camera;
pub mod dev_shaders;
pub mod dev_ui;
pub mod extract;
pub mod fps_reporter;
pub mod gltf_mesh;
pub mod post_process_filter;
pub mod ray_default_plugins;
pub mod ray_render_plugin;
pub mod raytracing_pipeline;
pub mod render_buffer;
pub mod render_device;
pub mod render_texture;
pub mod sbt;
pub mod shader;
pub mod sphere;
pub mod swapchain;
pub mod tlas_builder;
pub mod vk_init;
pub mod vk_utils;
pub mod vulkan_asset;
pub mod vulkan_mesh;
pub mod bluenoise_plugin;

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use debug_camera::{DebugCamera, DebugCameraPlugin};
use fps_reporter::print_fps;
use gltf_mesh::GltfModel;
use post_process_filter::PostProcessFilter;
use ray_render_plugin::RenderConfig;
use raytracing_pipeline::RaytracingPipeline;

use crate::ray_default_plugins::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_plugins(DebugCameraPlugin);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());
    app.add_systems(Startup, setup);
    app.add_systems(Update, print_fps);
    app.run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut windows: Query<&mut Window>,
) {
    let mut window = windows.single_mut();
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);

    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.4, 1.8, 4.0)
                .looking_at(Vec3::new(0.0, 1.8, 0.0), Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::FRAC_PI_3 * 1.0,
                near: 0.00001,
                far: 1000.0,
                aspect_ratio: window.width() / window.height(),
            }),
            ..default()
        },
        DebugCamera::default(),
    ));

    // plane
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Plane3d::default().mesh().size(100.0, 100.0)),
            material: materials.add(Color::srgb(0.3, 0.5, 0.3)),
            transform: Transform::from_translation(Vec3::new(0.0, -0.5, 0.0)),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cuboid(50.0, 0.01, 50.0),
    ));

    //commands.spawn(PbrBundle {
    //    mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
    //    material: materials.add(Color::srgb_u8(124, 144, 255)),
    //    transform: Transform::from_xyz(0.0, 0.5, 0.0),
    //    ..default()
    //});

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/cornell_box.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * -1.0))
    //            .with_translation(Vec3::new(0.0, 0.0, -4.0))
    //            .with_scale(Vec3::splat(1.0)),
    //    ),
    //));

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/sponza.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.0))
    //            .with_scale(Vec3::splat(0.012)),
    //    ),
    //));

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/sibenik.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.0))
    //            .with_scale(Vec3::splat(0.8)),
    //    ),
    //));

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/san_miquel.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
    //            .with_scale(Vec3::splat(0.8)),
    //    ),
    //));

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/bistro_interior.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.0))
    //            .with_scale(Vec3::splat(0.0028)),
    //    ),
    //));

    commands.spawn((
        asset_server.load::<GltfModel>("models/rungholt.glb"),
        TransformBundle::from_transform(
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(0.15)),
        ),
    ));

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/living_room.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
    //            .with_scale(Vec3::splat(1.0)),
    //    ),
    //));

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/fireplace.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
    //            .with_scale(Vec3::splat(1.0)),
    //    ),
    //));

    let filter = PostProcessFilter {
        vertex_shader: asset_server.load("shaders/quad.vert"),
        fragment_shader: asset_server.load("shaders/quad.frag"),
    };

    let rtx_pipeline = RaytracingPipeline {
        raygen_shader: asset_server.load("shaders/raygen.rgen"),
        miss_shader: asset_server.load("shaders/miss.rmiss"),
        hit_shader: asset_server.load("shaders/closest_hit.rchit"),
        sphere_intersection_shader: asset_server.load("shaders/sphere_intersection.rint"),
        sphere_hit_shader: asset_server.load("shaders/sphere_hit.rchit"),
    };

    commands.insert_resource(RenderConfig {
        rtx_pipeline: asset_server.add(rtx_pipeline),
        postprocess_pipeline: asset_server.add(filter),
        skydome: Some(asset_server.load("textures/sky.hdr")),
        sky_color: Vec4::ZERO,
        accumulate: false,
        pull_focus: None,
    });
}
