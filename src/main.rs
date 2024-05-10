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
use bevy_rapier3d::prelude::*;
use gltf_mesh::Gltf;
use post_process_filter::PostProcessFilter;
use ray_render_plugin::RenderConfig;
use raytracing_pipeline::RaytracingPipeline;

use crate::ray_default_plugins::*;

#[derive(Component, Default)]
struct DebugCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub yaw_speed: f32,
    pub pitch_speed: f32,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());
    app.add_systems(Startup, setup);
    app.add_systems(Update, (controls, print_fps));

    app.run();
}

#[derive(Resource)]
struct GameAssets {
    cube: Handle<Mesh>,
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

    commands.insert_resource(GameAssets {
        cube: meshes.add(Cuboid::default()),
    });

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
            material: materials.add(Color::rgb(0.3, 0.5, 0.3)),
            transform: Transform::from_translation(Vec3::new(0.0, -0.5, 0.0)),
            ..default()
        },
        RigidBody::Fixed,
        Collider::cuboid(50.0, 0.01, 50.0),
    ));

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
        asset_server.load::<Gltf>("models/rungholt.glb"),
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
        skydome: asset_server.load("textures/sky.hdr"),
        accumulate: false,
        pull_focus: None,
    });
}

fn controls(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(Entity, &mut DebugCamera)>,
    sphere: Query<Entity, With<crate::sphere::Sphere>>,
    mut transform: Query<&mut Transform>,
) {
    let (camera_entity, mut camera) = camera.single_mut();
    let mut transform = transform.get_mut(camera_entity).unwrap();

    if keyboard.just_pressed(KeyCode::Tab) {
        commands.spawn((
            asset_server.load::<Gltf>("models/DamagedHelmet.glb"),
            TransformBundle::from_transform(
                Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_translation(transform.translation)
                    .with_scale(Vec3::splat(0.8)),
            ),
        ));
    }

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
    let rot_acceleration = 0.1 * time.delta_seconds();
    let max_rot_speed = time.delta_seconds();
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
        camera.yaw_speed = (camera.yaw_speed + rot_acceleration).min(max_rot_speed);
    }
    if keyboard.pressed(KeyCode::ArrowRight) {
        camera.yaw_speed = (camera.yaw_speed - rot_acceleration).max(-max_rot_speed);
    }

    if keyboard.pressed(KeyCode::ArrowUp) {
        camera.pitch_speed = (camera.pitch_speed + rot_acceleration).min(max_rot_speed);
    }

    if keyboard.pressed(KeyCode::ArrowDown) {
        camera.pitch_speed = (camera.pitch_speed - rot_acceleration).max(-max_rot_speed);
    }

    camera.yaw += camera.yaw_speed;
    camera.pitch += camera.pitch_speed;
    camera.yaw_speed *= 0.90;
    camera.pitch_speed *= 0.90;

    if camera.yaw_speed.abs() < 0.001 {
        camera.yaw_speed = 0.0;
    }

    if camera.pitch_speed.abs() < 0.001 {
        camera.pitch_speed = 0.0;
    }

    transform.translation += translation;
    transform.rotation = Quat::from_rotation_y(camera.yaw) * Quat::from_rotation_x(camera.pitch);
}

fn print_fps(time: Res<Time>, mut tick: Local<u64>, mut last_time: Local<u128>) {
    *tick += 1;
    if *tick % 60 == 0 {
        let current = time.elapsed().as_millis();
        let elapsed = current - *last_time;
        *last_time = current;
        println!("FPS: {}", (1000.0 / elapsed as f32) * 60.0);
    }
}

fn spawn_cubes(
    mut commands: Commands,
    game_assets: Res<GameAssets>,
    mut tick: Local<u64>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    *tick += 1;
    if *tick % 60 == 0 {
        let mut material: StandardMaterial = Color::rgb(0.5, 0.5, 0.5).into();
        if *tick % 360 == 0 {
            material.emissive = Color::rgb(rand::random(), rand::random(), rand::random()) * 1.0;
        }
        let density = rand::random::<f32>() * 100.0;
        commands.spawn((
            game_assets.cube.clone(),
            Transform::from_xyz(0.0, 10.0, 0.0),
            GlobalTransform::default(),
            RigidBody::Dynamic,
            Collider::cuboid(0.5, 0.5, 0.5),
            ColliderMassProperties::Density(density),
            materials.add(material),
        ));
    }
}
