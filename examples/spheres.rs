use bevy::prelude::*;
use bevy_vulkan::{
    debug_camera::{DebugCamera, DebugCameraPlugin},
    fps_reporter::print_fps,
    gltf_mesh::GltfModel,
    post_process_filter::PostProcessFilter,
    ray_default_plugins::RayDefaultPlugins,
    ray_render_plugin::RenderConfig,
    raytracing_pipeline::RaytracingPipeline,
    sphere::Sphere,
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
    mut windows: Query<&mut Window>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut window = windows.single_mut();
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);

    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(4.0, 1.8, 0.0)
                .looking_at(Vec3::new(4.0, 1.8, 0.0), Vec3::Y),
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
        material: materials.add(Color::srgb(0.2, 0.2, 0.3)),
        transform: Transform::from_translation(Vec3::new(0.0, -0.5, 0.0)),
        ..default()
    },));

    commands.spawn((
        TransformBundle::from_transform(Transform::from_translation(Vec3::new(0.0, 1.5, 0.0))),
        Sphere,
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.8),
            ..default()
        }),
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
