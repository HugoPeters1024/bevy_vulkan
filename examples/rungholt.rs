use bevy::prelude::*;
use bevy_vulkan::{
    debug_camera::{DebugCamera, DebugCameraPlugin},
    gltf_mesh::GltfModel,
    post_process_filter::PostProcessFilter,
    ray_default_plugins::RayDefaultPlugins,
    ray_render_plugin::RenderConfig,
    raytracing_pipeline::RaytracingPipeline,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_plugins(DebugCameraPlugin);
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
    commands.spawn((PbrBundle {
        mesh: meshes.add(Plane3d::default().mesh().size(100.0, 100.0)),
        material: materials.add(Color::srgb(0.3, 0.5, 0.3)),
        transform: Transform::from_translation(Vec3::new(0.0, -0.5, 0.0)),
        ..default()
    },));

    commands.spawn((
        asset_server.load::<GltfModel>("models/rungholt.glb"),
        TransformBundle::from_transform(
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(0.15)),
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
        skydome: asset_server.load("textures/sky.hdr"),
        accumulate: false,
        pull_focus: None,
    });
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
