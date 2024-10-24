use bevy::prelude::*;
use bevy_vulkan::{
    debug_camera::{DebugCamera, DebugCameraPlugin},
    dev_shaders::DevShaderPlugin,
    dev_ui::DevUIPlugin,
    gltf_mesh::{GltfModel, GltfModelHandle},
    ray_default_plugins::RayDefaultPlugins,
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

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut windows: Query<&mut Window>) {
    let mut window = windows.single_mut();
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);

    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.4, 1.8, 4.0).looking_at(Vec3::new(0.0, 1.8, 0.0), Vec3::Y),
        DebugCamera::default(),
    ));

    commands.spawn((
        GltfModelHandle(asset_server.load::<GltfModel>("models/rungholt.glb")),
        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.15)),
    ));
}
