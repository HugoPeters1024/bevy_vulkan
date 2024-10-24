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
    app.add_systems(FixedUpdate, print_cam_pos);
    app.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut windows: Query<&mut Window>) {
    let mut window = windows.single_mut();
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);

    // camera
    commands.spawn((
        Transform::from_xyz(4.98, 5.83, 1.3)
            .with_rotation(Quat::from_xyzw(-0.0941, -0.701, -0.094, 0.700).normalize()),
        Camera3d::default(),
        DebugCamera::default(),
    ));

    commands.spawn((
        GltfModelHandle(asset_server.load::<GltfModel>("models/san_miquel.glb")),
        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.8)),
    ));
}

fn print_cam_pos(q: Query<&Transform, With<Camera>>, keyboard: Res<ButtonInput<KeyCode>>) {
    if keyboard.just_pressed(KeyCode::Space) {
        for t in q.iter() {
            dbg!(t);
        }
    }
}
