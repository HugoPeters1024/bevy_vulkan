use bevy::{
    app::{App, Plugin},
    asset::{Asset, AssetEvent, AssetId, Assets},
    ecs::{
        event::EventReader,
        schedule::IntoSystemConfigs,
        system::{Res, ResMut, Resource},
        world::{Mut, World},
    },
    render::{ExtractSchedule, RenderApp},
    utils::HashMap,
};
use crossbeam::channel::{Receiver, Sender};

use crate::{
    extract::Extract,
    ray_render_plugin::{Render, RenderSet, TeardownSchedule},
    render_device::RenderDevice,
};

pub trait VulkanAsset: Asset + Clone + Send + Sync + 'static {
    type PreparedAsset: Send + Sync + 'static;

    fn prepare_asset(self, render_device: &RenderDevice) -> Self::PreparedAsset;
    fn destroy_asset(render_device: &RenderDevice, prepared_asset: &Self::PreparedAsset);
}

#[derive(Resource)]
struct VulkanAssetComms<A: VulkanAsset> {
    send_work: Sender<(AssetId<A>, A)>,
    recv_result: Receiver<(AssetId<A>, A::PreparedAsset)>,
}

impl<A: VulkanAsset> VulkanAssetComms<A> {
    fn new(render_device: RenderDevice) -> Self {
        let (send_work, recv_work) = crossbeam::channel::unbounded::<(AssetId<A>, A)>();
        let (send_result, recv_result) = crossbeam::channel::unbounded();

        let ret = Self {
            send_work,
            recv_result,
        };

        std::thread::spawn(move || {
            while let Ok((id, asset)) = recv_work.recv() {
                if let Err(_) = send_result.send((id, asset.prepare_asset(&render_device))) {
                    break;
                }
            }
        });

        ret
    }
}

#[derive(Resource)]
pub struct VulkanAssets<A: VulkanAsset>(HashMap<AssetId<A>, A::PreparedAsset>);

impl<A: VulkanAsset> Default for VulkanAssets<A> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}

pub struct VulkanAssetPlugin<A: VulkanAsset> {
    _marker: std::marker::PhantomData<A>,
}

impl<A: VulkanAsset> Default for VulkanAssetPlugin<A> {
    fn default() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<A: VulkanAsset> Plugin for VulkanAssetPlugin<A> {
    fn build(&self, app: &mut App) {
        let render_app = app.get_sub_app_mut(RenderApp).unwrap();
        let render_device = render_app
            .world
            .get_resource::<RenderDevice>()
            .unwrap()
            .clone();
        render_app.insert_resource(VulkanAssetComms::<A>::new(render_device));
        render_app.init_resource::<VulkanAssets<A>>();
        render_app.add_systems(ExtractSchedule, extract_vulkan_asset::<A>);
        render_app.add_systems(Render, poll_for_asset::<A>.in_set(RenderSet::Prepare));
        render_app.add_systems(TeardownSchedule, on_shutdown::<A>);
    }
}

fn extract_vulkan_asset<A: VulkanAsset>(
    mut asset_events: Extract<EventReader<AssetEvent<A>>>,
    assets: Extract<Res<Assets<A>>>,
    comms: Res<VulkanAssetComms<A>>,
) {
    for event in asset_events.read() {
        match event {
            AssetEvent::Added { id } => {
                log::debug!("VulkanAsset received AssetEvent::Added for asset with id: {:?}", id);
                if let Some(asset) = assets.get(*id) {
                    comms.send_work.send((*id, asset.clone())).unwrap();
                } else {
                    log::warn!("VulkanAsset could not find asset with id: {:?}", id);
                }
            }
            AssetEvent::Modified { id } => {
                log::debug!("VulkanAsset received AssetEvent::Modified for asset with id: {:?}", id);
                if let Some(asset) = assets.get(*id) {
                    comms.send_work.send((*id, asset.clone())).unwrap();
                } else {
                    log::warn!("VulkanAsset could not find asset with id: {:?}", id);
                }
            }
            AssetEvent::Removed { id } => {
                log::debug!("VulkanAsset does not support AssetEvent::Removed for asset with id: {:?}", id);
            }
            AssetEvent::LoadedWithDependencies { id } => {
                log::debug!("VulkanAsset does not support AssetEvent::LoadedWithDependencies for asset with id: {:?}", id);
            }
            AssetEvent::Unused { id } => {
                log::debug!("VulkanAsset does not support AssetEvent::Unused for asset with id: {:?}", id);
            }
        }
    }
}

fn poll_for_asset<A: VulkanAsset>(
    comms: Res<VulkanAssetComms<A>>,
    mut assets: ResMut<VulkanAssets<A>>,
) {
    while let Ok((id, prep)) = comms.recv_result.try_recv() {
        log::info!("VulkanAsset received prepared asset for id: {:?}", id);
        assets.0.insert(id, prep);
    }
}

fn on_shutdown<A: VulkanAsset>(world: &mut World) {
    world.remove_resource::<VulkanAssetComms<A>>();
    world.resource_scope(|world, mut assets: Mut<VulkanAssets<A>>| {
        let render_device = world.get_resource::<RenderDevice>().unwrap();
        for (_, prep) in assets.0.drain() {
            A::destroy_asset(&render_device, &prep);
        }
    });
}
