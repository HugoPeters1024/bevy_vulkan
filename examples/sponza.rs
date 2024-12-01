use bevy::prelude::*;
use bevy_vulkan::{
    debug_camera::{DebugCamera, DebugCameraPlugin},
    dev_shaders::DevShaderPlugin,
    dev_ui::DevUIPlugin,
    gltf_mesh::{GltfModel, GltfModelHandle},
    ray_default_plugins::RayDefaultPlugins,
    ray_render_plugin::RenderConfig,
    sphere::Sphere,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(RayDefaultPlugins);
    app.add_plugins(DevShaderPlugin);
    app.add_plugins(DevUIPlugin);
    app.add_plugins(DebugCameraPlugin);
    app.add_systems(Startup, setup);
    app.run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut render_config: ResMut<RenderConfig>,
) {
    //render_config.skydome = None;
    render_config.sky_color = Vec4::splat(1.0);

    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(4.0, 1.8, 0.0).looking_at(Vec3::new(4.0, 1.8, 0.0), Vec3::Y),
        DebugCamera::default(),
    ));

    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 0.0)),
        Sphere,
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.0, 0.0),
            emissive: LinearRgba::new(10.0, 7.0, 5.0, 1.0),
            ..default()
        })),
    ));

    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 6.1, 5.5)).with_scale(Vec3::splat(2.0)),
        Sphere,
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 1.0),
            perceptual_roughness: 0.0,
            ior: 1.02,
            specular_transmission: 1.0,
            ..default()
        })),
    ));

    commands.spawn((
        GltfModelHandle(asset_server.load::<GltfModel>("models/sponza.glb")),
        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.0))
            .with_scale(Vec3::splat(0.012)),
    ));
}
