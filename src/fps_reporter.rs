use bevy::prelude::*;

pub fn print_fps(time: Res<Time>, mut tick: Local<u64>, mut last_time: Local<u128>) {
    *tick += 1;
    if *tick % 60 == 0 {
        let current = time.elapsed().as_millis();
        let elapsed = current - *last_time;
        *last_time = current;
        println!("FPS: {}", (1000.0 / elapsed as f32) * 60.0);
    }
}
