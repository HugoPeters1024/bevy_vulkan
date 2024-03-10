mod blas;
mod extract;
mod gltf_mesh;
mod post_process_filter;
mod ray_default_plugins;
mod ray_render_plugin;
mod raytracing_pipeline;
mod render_buffer;
mod render_device;
mod render_texture;
mod sbt;
mod shader;
mod sphere;
mod swapchain;
mod tlas_builder;
mod vk_init;
mod vk_utils;
mod vulkan_asset;
mod vulkan_mesh;

use bevy::prelude::*;
use gltf_mesh::Gltf;
use post_process_filter::PostProcessFilter;
use ray_render_plugin::RenderConfig;
use raytracing_pipeline::RaytracingPipeline;

use crate::ray_default_plugins::*;

#[derive(Component)]
struct Cube;

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_systems(Startup, setup);
    app.add_systems(Update, (animate_cube, move_camera));
    app.run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut windows: Query<&mut Window>,
) {
    let mut window = windows.single_mut();
    window.resolution.set_scale_factor_override(Some(1.0));

    // camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.4, 1.8, 4.0).looking_at(Vec3::new(0.0, 1.8, 0.0), Vec3::Y),
        ..default()
    });

    commands.spawn(PbrBundle {
        mesh: meshes.add(Circle::new(4.0)),
        material: materials.add(Color::rgb(0.8, 0.2, 0.2)),
        transform: Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(5.0)),
        ..default()
    });

    for _ in 0..40 {
        let x = rand::random::<f32>() * 30.0 - 15.0;
        let z = rand::random::<f32>() * 30.0 - 15.0;
        let scale = rand::random::<f32>() * 1.5 + 0.5;
        let y = scale / 2.0;
        let material = materials.add(StandardMaterial {
            base_color: Color::rgb(
                rand::random::<f32>(),
                rand::random::<f32>(),
                rand::random::<f32>(),
            ),
            specular_transmission: rand::random::<f32>(),
            perceptual_roughness: rand::random::<f32>(),
            ..default()
        });
        commands.spawn((
            crate::sphere::Sphere,
            TransformBundle::from_transform(
                Transform::from_xyz(x, y, z).with_scale(Vec3::splat(scale)),
            ),
            material,
        ));
    }

    commands.spawn((
        asset_server.load::<Gltf>("models/sibenik.glb"),
        TransformBundle::from_transform(Transform::from_rotation(Quat::from_rotation_x(
            std::f32::consts::FRAC_PI_2*0.0,
        )).with_scale(Vec3::splat(0.4))),
    ));

    // commands.spawn((
    //     crate::sphere::Sphere,
    //     TransformBundle::from_transform(
    //         Transform::from_xyz(0.35, 0.85, 0.35).with_scale(Vec3::splat(0.5)),
    //     ),
    // ));

    // cube
    //commands.spawn((
    //    PbrBundle {
    //        mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
    //        material: materials.add(Color::rgb_u8(124, 144, 255)),
    //        transform: Transform::from_xyz(0.0, 1.2, 0.0),
    //        ..default()
    //    },
    //    Cube,
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
        accumulate: false,
    });
}

fn animate_cube(time: Res<Time>, mut query: Query<(&Cube, &mut Transform)>) {
    for (_, mut transform) in query.iter_mut() {
        transform.rotate(Quat::from_rotation_x(time.delta_seconds()));
    }
}

fn move_camera(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Transform, With<Camera3d>>,
) {
    for mut transform in query.iter_mut() {
        let forward: Vec3 = transform.local_z().into();
        let side: Vec3 = transform.local_x().into();
        let mut translation = Vec3::ZERO;
        let speed = 0.5 * time.delta_seconds();
        let rot_speed = time.delta_seconds();
        if keyboard.pressed(KeyCode::KeyW) {
            translation += -forward * speed;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            translation += forward * speed;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            translation -= side * speed;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            translation += side * speed;
        }
        if keyboard.pressed(KeyCode::KeyQ) {
            translation -= Vec3::Y * speed;
        }
        if keyboard.pressed(KeyCode::KeyE) {
            translation += Vec3::Y * speed;
        }

        let mut rotation = Quat::IDENTITY;

        if keyboard.pressed(KeyCode::ArrowLeft) {
            rotation *= Quat::from_rotation_y(rot_speed);
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            rotation *= Quat::from_rotation_y(-rot_speed);
        }

        transform.translation += translation;
        transform.rotate(rotation);
    }
}
