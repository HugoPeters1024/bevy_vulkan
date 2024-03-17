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

#[derive(Component, Default)]
struct DebugCamera {
    pub yaw: f32,
    pub pitch: f32,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_systems(Startup, setup);
    app.add_systems(Update, (animate_cube, move_camera, print_fps));
    app.run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut windows: Query<&mut Window>,
) {
    let mut window = windows.single_mut();
    window.resolution.set_scale_factor_override(Some(1.0));

    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.4, 1.8, 4.0)
                .looking_at(Vec3::new(0.0, 1.8, 0.0), Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::FRAC_PI_3 * 1.2,
                near: 0.1,
                far: 100.0,
                aspect_ratio: window.width() / window.height(),
            }),
            ..default()
        },
        DebugCamera::default(),
    ));

    commands.spawn((
        crate::sphere::Sphere,
        TransformBundle::from_transform(Transform::from_xyz(0.0, 0.51, 0.0)),
        materials.add(StandardMaterial {
            base_color: Color::WHITE,
            specular_transmission: 1.0,
            perceptual_roughness: 0.01,
            emissive: Color::BLACK,
            ..default()
        }),
    ));

    //commands.spawn((
    //    asset_server.load::<Gltf>("models/sponza.glb"),
    //    TransformBundle::from_transform(
    //        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.0))
    //            .with_scale(Vec3::splat(0.008)),
    //    ),
    //));

    commands.spawn((
        asset_server.load::<Gltf>("models/rungholt.glb"),
        TransformBundle::from_transform(
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(0.3)),
        ),
    ));

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
        pull_focus: None,
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
    mut query: Query<(&mut Transform, &mut DebugCamera)>,
) {
    for (mut transform, mut camera) in query.iter_mut() {
        let forward: Vec3 = transform.local_z().into();
        let side: Vec3 = transform.local_x().into();
        let mut translation = Vec3::ZERO;
        let speed = 0.5
            * time.delta_seconds()
            * if keyboard.pressed(KeyCode::ShiftLeft) {
                3.4
            } else {
                1.0
            };
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

        if keyboard.pressed(KeyCode::ArrowLeft) {
            camera.yaw += rot_speed;
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            camera.yaw -= rot_speed;
        }

        if keyboard.pressed(KeyCode::ArrowUp) {
            camera.pitch += rot_speed;
        }

        if keyboard.pressed(KeyCode::ArrowDown) {
            camera.pitch -= rot_speed;
        }

        transform.translation += translation;
        transform.rotation =
            Quat::from_rotation_y(camera.yaw) * Quat::from_rotation_x(camera.pitch);
    }
}

fn print_fps(time: Res<Time>, mut tick: Local<u64>) {
    *tick += 1;
    if *tick % 60 == 0 {
        println!("FPS: {}", 1.0 / time.delta_seconds());
    }
}
