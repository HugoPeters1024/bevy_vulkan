use bevy::{
    app::SubApp,
    ecs::{schedule::ScheduleLabel, system::SystemState},
    prelude::*,
    render::RenderApp,
    window::{PrimaryWindow, RawHandleWrapper, WindowCloseRequested},
};

pub fn close_when_requested(mut commands: Commands, mut closed: EventReader<WindowCloseRequested>) {
    if closed.len() > 0 {
        log::info!("Window close requested");
        for event in closed.read() {
            commands.entity(event.window).despawn();
        }
    }
}

#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone)]
pub struct Render;

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum RenderSet {
    ExtractCommands,
    Render,
    Cleanup,
}

impl Render {
    fn base_schedule() -> Schedule {
        let mut schedule = Schedule::new(Self);
        schedule.configure_sets(
            (
                RenderSet::ExtractCommands,
                RenderSet::Render,
                RenderSet::Cleanup,
            )
                .chain(),
        );
        schedule
    }
}

pub struct RayRenderPlugin;

impl Plugin for RayRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, close_when_requested);

        let mut render_app = App::empty();
        render_app.main_schedule_label = Render.intern();

        let mut system_state: SystemState<Query<&RawHandleWrapper, With<PrimaryWindow>>> =
            SystemState::new(&mut app.world);
        let primary_window = system_state.get(&app.world).get_single().ok().cloned().unwrap();

        render_app
            .insert_resource(unsafe { crate::render_device::RenderDevice::from_window(&primary_window) });

        app.init_resource::<ScratchMainWorld>();

        let mut extract_schedule = Schedule::new(ExtractSchedule);
        extract_schedule.set_apply_final_deferred(false);

        render_app.main_schedule_label = Render.intern();
        render_app.add_schedule(extract_schedule);
        render_app.add_schedule(Render::base_schedule());
        render_app.add_systems(Render, World::clear_entities.in_set(RenderSet::Cleanup));
        render_app.add_systems(
            Render,
            apply_extract_commands.in_set(RenderSet::ExtractCommands),
        );

        app.insert_sub_app(RenderApp, SubApp::new(render_app, move |main_world, render_app| {
            let total_count = main_world.entities().total_count();

            assert_eq!(
                render_app.world.entities().len(),
                0,
                "An entity was spawned after the entity list was cleared last frame and before the extract schedule began. This is not supported",
            );

            // SAFETY: This is safe given the clear_entities call in the past frame and the assert above
            unsafe {
                render_app
                    .world
                    .entities_mut()
                    .flush_and_reserve_invalid_assuming_no_entities(total_count);
            }

            extract(main_world, render_app);
        }));
    }
}

#[derive(Resource, Default)]
struct ScratchMainWorld(World);

/// The simulation [`World`] of the application, stored as a resource.
/// This resource is only available during [`ExtractSchedule`] and not
/// during command application of that schedule.
/// See [`Extract`] for more details.
#[derive(Resource, Default)]
pub struct MainWorld(World);

fn extract(main_world: &mut World, render_app: &mut App) {
    // temporarily add the app world to the render world as a resource
    let scratch_world = main_world.remove_resource::<ScratchMainWorld>().unwrap();
    let inserted_world = std::mem::replace(main_world, scratch_world.0);
    render_app.world.insert_resource(MainWorld(inserted_world));

    render_app.world.run_schedule(ExtractSchedule);

    // move the app world back, as if nothing happened.
    let inserted_world = render_app.world.remove_resource::<MainWorld>().unwrap();
    let scratch_world = std::mem::replace(main_world, inserted_world.0);
    main_world.insert_resource(ScratchMainWorld(scratch_world));
}

/// Applies the commands from the extract schedule. This happens during
/// the render schedule rather than during extraction to allow the commands to run in parallel with the
/// main app when pipelined rendering is enabled.
fn apply_extract_commands(render_world: &mut World) {
    render_world.resource_scope(|render_world, mut schedules: Mut<Schedules>| {
        schedules
            .get_mut(ExtractSchedule)
            .unwrap()
            .apply_deferred(render_world);
    });
}
