use bevy::prelude::*;

#[derive(Component)]
pub struct DebugCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub move_acceleration: f32,
    move_speed: Vec3,
    yaw_speed: f32,
    pitch_speed: f32,
}

impl Default for DebugCamera {
    fn default() -> Self {
        DebugCamera {
            yaw: 0.0,
            pitch: 0.0,
            move_acceleration: 1.0,
            move_speed: Vec3::ZERO,
            yaw_speed: 0.0,
            pitch_speed: 0.0,
        }
    }
}

pub struct DebugCameraPlugin;

impl Plugin for DebugCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, controls);
    }
}

fn controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(Entity, &mut DebugCamera)>,
    mut transform: Query<&mut Transform>,
) {
    let (camera_entity, mut camera) = camera.single_mut();
    let mut transform = transform.get_mut(camera_entity).unwrap();

    let forward: Vec3 = transform.local_z().into();
    let side: Vec3 = transform.local_x().into();
    let move_acceleration = 0.5
        * time.delta_secs()
        * if keyboard.pressed(KeyCode::ShiftLeft) {
            3.4 * camera.move_acceleration
        } else {
            camera.move_acceleration
        };
    let rot_acceleration = 0.2 * time.delta_secs();
    let max_rot_speed = time.delta_secs();
    if keyboard.pressed(KeyCode::KeyW) {
        camera.move_speed += -forward * move_acceleration;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        camera.move_speed += forward * move_acceleration;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        camera.move_speed -= side * move_acceleration;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        camera.move_speed += side * move_acceleration;
    }
    if keyboard.pressed(KeyCode::KeyQ) {
        camera.move_speed -= Vec3::Y * move_acceleration;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        camera.move_speed += Vec3::Y * move_acceleration;
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
    camera.yaw_speed *= 0.80;
    camera.pitch_speed *= 0.80;
    camera.move_speed *= 0.80;

    if camera.yaw_speed.abs() < 0.001 {
        camera.yaw_speed = 0.0;
    }

    if camera.pitch_speed.abs() < 0.001 {
        camera.pitch_speed = 0.0;
    }

    if camera.move_speed.length() < 0.001 {
        camera.move_speed = Vec3::ZERO;
    }

    transform.translation += camera.move_speed;
    transform.rotation = Quat::from_rotation_y(camera.yaw) * Quat::from_rotation_x(camera.pitch);
}
