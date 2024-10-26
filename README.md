### Raytracing in Bevy (WIP 🔨)

This is a custom rendering backend for Bevy leverages hardware raytracing using vulkan.
You will need a GPU that supports `VK_KHR_ray_tracing`. A non exhaustive list of supported device
can be found on [gpuinfo.org](https://vulkan.gpuinfo.org/listdevicescoverage.php?extension=VK_KHR_ray_tracing&platform=all)

The models required to run some of the examples are not available yet because they are too big for git, @me if you want to get a copy.

### Required packages

Besides the rust toolchain, you will need to follow the [installation guide for Bevy itself](https://bevyengine.org/learn/quick-start/getting-started/setup/#installing-os-dependencies).
After that you should be able to run any of the examples given that you meet the GPU hardware and software requirements.

### Examples

run `cargo run --example` to get a list of available examples.

This rendering backend integrates seamlessly with Bevy, as a result, the code needed to run a simple scene is extremely simple:

```rust
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
) {
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(4.0, 1.8, 0.0).looking_at(Vec3::new(4.0, 1.8, 0.0), Vec3::Y),
        DebugCamera::default(),
    ));

    commands.spawn((
        GltfModelHandle(asset_server.load::<GltfModel>("models/sponza.glb")),
        Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.0))
            .with_scale(Vec3::splat(0.012)),
    ));

    // glowing sphere
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 0.0)),
        Sphere,
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.0, 0.0),
            emissive: LinearRgba::new(10.0, 7.0, 5.0, 1.0),
            ..default()
        })),
    ));
}
```
