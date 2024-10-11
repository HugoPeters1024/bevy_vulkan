use bevy::prelude::*;
use bevy_vulkan::{
    debug_camera::{DebugCamera, DebugCameraPlugin},
    dev_shaders::DevShaderPlugin,
    fps_reporter::print_fps,
    ray_default_plugins::RayDefaultPlugins,
    ray_render_plugin::RenderConfig,
    sphere::Sphere,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_plugins(DevShaderPlugin);
    app.add_plugins(DebugCameraPlugin);
    app.add_systems(Startup, setup);
    app.add_systems(Update, print_fps);
    app.run();
}

fn setup(
    mut commands: Commands,
    mut windows: Query<&mut Window>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut render_config: ResMut<RenderConfig>,
) {
    let mut window = windows.single_mut();
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);

    render_config.skydome = None;
    render_config.sky_color = 0.3 * Vec4::new(0.529, 0.808, 0.922, 0.0);

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
        material: materials.add(Color::srgb(0.1, 0.1, 0.1)),
        ..default()
    },));

    commands.spawn((
        TransformBundle::from_transform(Transform::from_translation(Vec3::new(0.0, 0.5, 0.0))),
        Sphere,
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.8),
            ..default()
        }),
    ));

    commands.spawn((
        TransformBundle::from_transform(Transform::from_translation(Vec3::new(1.2, 0.5, 0.0))),
        Sphere,
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 1.0),
            perceptual_roughness: 0.001,
            ior: 1.1,
            specular_transmission: 1.0,
            ..default()
        }),
    ));

    commands.spawn((
        TransformBundle::from_transform(Transform::from_translation(Vec3::new(2.4, 0.5, 0.0))),
        Sphere,
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.2, 0.2),
            perceptual_roughness: 0.001,
            metallic: 0.0,
            ior: 1.1,
            specular_transmission: 0.0,
            ..default()
        }),
    ));

    commands.spawn((
        TransformBundle::from_transform(Transform::from_translation(Vec3::new(1.6, 0.5, 2.9))),
        Sphere,
        materials.add(StandardMaterial {
            emissive: LinearRgba::new(2.5, 2.5, 2.9, 0.0),
            ..default()
        }),
    ));
}
