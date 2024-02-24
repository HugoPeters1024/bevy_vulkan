use bevy::{app::PluginGroupBuilder, prelude::*};

pub struct RayDefaultPlugins;

impl PluginGroup for RayDefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();
        group = group
            .add(bevy::log::LogPlugin::default())
            .add(bevy::core::TaskPoolPlugin::default())
            .add(bevy::core::TypeRegistrationPlugin)
            .add(bevy::core::FrameCountPlugin)
            .add(bevy::time::TimePlugin)
            .add(bevy::transform::TransformPlugin)
            .add(bevy::hierarchy::HierarchyPlugin)
            .add(bevy::diagnostic::DiagnosticsPlugin)
            .add(bevy::input::InputPlugin)
            .add(bevy::window::WindowPlugin {
                close_when_requested: false,
                ..default()
            })
            .add(bevy::a11y::AccessibilityPlugin);

        group = group.add(bevy::asset::AssetPlugin::default());
        group = group.add(bevy::scene::ScenePlugin);
        group = group.add(bevy::winit::WinitPlugin::default());
        group = group.add(bevy::audio::AudioPlugin::default());

        group = group.add(bevy::render::pipelined_rendering::PipelinedRenderingPlugin);

        group = group.add(crate::ray_render_plugin::RayRenderPlugin);
        group = group.add(crate::post_process_filter::PostProcessFilterPlugin);
        group = group.add(crate::shader::ShaderPlugin);
        group
    }
}
