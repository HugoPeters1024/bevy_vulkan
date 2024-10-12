use bevy::prelude::*;
use bevy_vulkan::{
    debug_camera::{DebugCamera, DebugCameraPlugin},
    dev_shaders::DevShaderPlugin,
    fps_reporter::print_fps,
    ray_default_plugins::RayDefaultPlugins,
    ray_render_plugin::RenderConfig,
    sphere::Sphere,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

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

    //render_config.skydome = None;
    render_config.sky_color = 0.3 * Vec4::new(0.529, 0.808, 0.922, 0.0);

    // camera
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 1.0, 7.0)
                .looking_at(Vec3::new(2.0, 1.0, 0.0), Vec3::Y),
            projection: Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::FRAC_PI_3 * 0.7,
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
        material: materials.add(Color::srgb(0.1, 0.2, 0.1)),
        ..default()
    },));

    commands.spawn((
        TransformBundle::from_transform(
            Transform::from_translation(Vec3::new(0.0, 1.5, 0.0)).with_scale(Vec3::splat(3.0)),
        ),
        Sphere,
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.8),
            ..default()
        }),
    ));

    commands.spawn((
        TransformBundle::from_transform(
            Transform::from_translation(Vec3::new(3.8, 1.5, 0.0)).with_scale(Vec3::splat(3.0)),
        ),
        Sphere,
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 1.0),
            perceptual_roughness: 0.00,
            ior: 1.05,
            specular_transmission: 1.0,
            ..default()
        }),
    ));

    commands.spawn((
        TransformBundle::from_transform(
            Transform::from_translation(Vec3::new(-3.8, 1.5, 0.0)).with_scale(Vec3::splat(3.0)),
        ),
        Sphere,
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.2, 0.2),
            perceptual_roughness: 0.001,
            metallic: 0.5,
            ..default()
        }),
    ));

    let mut rng = ChaCha8Rng::seed_from_u64(42);

    for x in -11..11 {
        for y in -11..11 {
            let dx = rng.gen_range(-0.5..0.5);
            let dy = rng.gen_range(-0.5..0.5);

            let xf = x as f32 + dx;
            let yf = y as f32 + dy;

            if xf * xf + yf * yf < 4.0 * 4.0 {
                continue;
            }

            let choose_mat: f64 = rng.gen();
            let mut material = StandardMaterial::default();

            if choose_mat < 0.8 {
                // lambertian
                material.base_color = Color::linear_rgb(rng.gen(), rng.gen(), rng.gen());
            } else if choose_mat < 0.95 {
                // mirror
                material.base_color = Color::WHITE;
                material.perceptual_roughness = 0.01;
                material.metallic = 1.0;
            } else {
                // glass
                material.base_color = Color::WHITE;
                material.perceptual_roughness = 0.0;
                material.ior = 1.01 + 0.15 * rng.gen::<f32>();
                material.specular_transmission = 1.0;
            }
            commands.spawn((
                TransformBundle::from_transform(
                    Transform::from_translation(Vec3::new(xf, 0.25, yf))
                        .with_scale(Vec3::splat(0.5)),
                ),
                Sphere,
                materials.add(material),
            ));
        }
    }

    //commands.spawn((
    //    TransformBundle::from_transform(Transform::from_translation(Vec3::new(1.6, 0.5, 2.9))),
    //    Sphere,
    //    materials.add(StandardMaterial {
    //        emissive: LinearRgba::new(2.5, 2.5, 2.9, 0.0),
    //        ..default()
    //    }),
    //));
}
