use bevy::{
    app::App,
    asset::{Asset, AssetEvent, AssetId, Assets, Handle},
    ecs::{
        event::EventReader,
        schedule::IntoSystemConfigs,
        system::{Res, ResMut, Resource, StaticSystemParam, SystemParam, SystemParamItem},
        world::{Mut, World},
    },
    prelude::{Deref, DerefMut},
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
    type ExtractedAsset: Send + Sync + 'static;
    type ExtractParam: SystemParam;
    type PreparedAsset: Send + Sync + 'static;

    fn extract_asset(
        &self,
        param: &mut SystemParamItem<Self::ExtractParam>,
    ) -> Option<Self::ExtractedAsset>;

    fn prepare_asset(
        asset: Self::ExtractedAsset,
        render_device: &RenderDevice,
    ) -> Self::PreparedAsset;
    fn destroy_asset(render_device: &RenderDevice, prepared_asset: &Self::PreparedAsset);
}

#[derive(Resource)]
pub struct VulkanAssetComms<A: VulkanAsset> {
    send_work: Sender<(AssetId<A>, A::ExtractedAsset)>,
    recv_result: Receiver<(AssetId<A>, A::PreparedAsset)>,
}

impl<A: VulkanAsset> VulkanAssetComms<A> {
    fn new(render_device: RenderDevice) -> Self {
        let (send_work, recv_work) =
            crossbeam::channel::unbounded::<(AssetId<A>, A::ExtractedAsset)>();
        let (send_result, recv_result) = crossbeam::channel::unbounded();

        let ret = Self {
            send_work,
            recv_result,
        };

        std::thread::spawn(move || {
            while let Ok((id, asset)) = recv_work.recv() {
                if let Err(_) = send_result.send((id, A::prepare_asset(asset, &render_device))) {
                    break;
                }
            }
        });

        ret
    }
}

pub enum VulkanAssetLoadingState<A: VulkanAsset> {
    Loading,
    Loaded(A::PreparedAsset),
}

#[derive(Resource, Deref, DerefMut)]
pub struct VulkanAssets<A: VulkanAsset>(HashMap<AssetId<A>, VulkanAssetLoadingState<A>>);

impl<A: VulkanAsset> VulkanAssets<A> {
    pub fn get(&self, handle: &Handle<A>) -> Option<&A::PreparedAsset> {
        self.0.get(&handle.id()).map_or(None, |state| match state {
            VulkanAssetLoadingState::Loading => None,
            VulkanAssetLoadingState::Loaded(asset) => Some(asset),
        })
    }
}

impl<A: VulkanAsset> Default for VulkanAssets<A> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}

fn extract_vulkan_asset<A: VulkanAsset>(
    mut asset_events: Extract<EventReader<AssetEvent<A>>>,
    assets: Extract<Res<Assets<A>>>,
    mut render_assets: ResMut<VulkanAssets<A>>,
    comms: Res<VulkanAssetComms<A>>,
    param: StaticSystemParam<A::ExtractParam>,
) {
    let mut param = param.into_inner();
    for event in asset_events.read() {
        match event {
            AssetEvent::Added { id } => {
                log::debug!(
                    "VulkanAsset received AssetEvent::Added for asset with id: {:?}",
                    id
                );
                if let Some(asset) = assets.get(*id) {
                    if let Some(extracted) = asset.extract_asset(&mut param) {
                        if render_assets
                            .insert(*id, VulkanAssetLoadingState::Loading)
                            .is_none()
                        {
                            comms.send_work.send((*id, extracted)).unwrap();
                        }
                    }
                } else {
                    log::warn!("VulkanAsset could not find asset with id: {:?}", id);
                }
            }
            AssetEvent::Modified { id } => {
                log::debug!(
                    "VulkanAsset received AssetEvent::Modified for asset with id: {:?}",
                    id
                );
                if let Some(asset) = assets.get(*id) {
                    if let Some(extracted) = asset.extract_asset(&mut param) {
                        comms.send_work.send((*id, extracted)).unwrap();
                    }
                } else {
                    log::warn!("VulkanAsset could not find asset with id: {:?}", id);
                }
            }
            AssetEvent::Removed { id } => {
                log::debug!(
                    "VulkanAsset does not support AssetEvent::Removed for asset with id: {:?}",
                    id
                );
            }
            AssetEvent::LoadedWithDependencies { id } => {
                log::debug!(
                    "VulkanAsset received AssetEvent::LoadedWithDependencies for asset with id: {:?}",
                    id
                );
                if let Some(asset) = assets.get(*id) {
                    if let Some(extracted) = asset.extract_asset(&mut param) {
                        if render_assets
                            .insert(*id, VulkanAssetLoadingState::Loading)
                            .is_none()
                        {
                            comms.send_work.send((*id, extracted)).unwrap();
                        }
                    }
                } else {
                    log::warn!("VulkanAsset could not find asset with id: {:?}", id);
                }
            }
            AssetEvent::Unused { id } => {
                log::debug!(
                    "VulkanAsset does not support AssetEvent::Unused for asset with id: {:?}",
                    id
                );
            }
        }
    }
}

pub fn poll_for_asset<A: VulkanAsset>(
    render_device: Res<RenderDevice>,
    comms: Res<VulkanAssetComms<A>>,
    mut assets: ResMut<VulkanAssets<A>>,
) {
    while let Ok((id, prep)) = comms.recv_result.try_recv() {
        log::debug!("VulkanAsset received prepared asset for id: {:?}", id);
        if let Some(old) = assets.0.insert(id, VulkanAssetLoadingState::Loaded(prep)) {
            match old {
                VulkanAssetLoadingState::Loading => {}
                VulkanAssetLoadingState::Loaded(old) => A::destroy_asset(&render_device, &old),
            }
        }
    }
}

fn on_shutdown<A: VulkanAsset>(world: &mut World) {
    world.remove_resource::<VulkanAssetComms<A>>();
    world.resource_scope(|world, mut assets: Mut<VulkanAssets<A>>| {
        let render_device = world.get_resource::<RenderDevice>().unwrap();
        for (_, prep) in assets.0.drain() {
            match prep {
                VulkanAssetLoadingState::Loading => {
                    log::warn!("VulkanAsset was still loading when shutting down");
                }
                VulkanAssetLoadingState::Loaded(prep) => A::destroy_asset(&render_device, &prep),
            }
        }
    });
}

pub trait VulkanAssetExt {
    fn init_vulkan_asset<A: VulkanAsset>(&mut self);
}

impl VulkanAssetExt for App {
    fn init_vulkan_asset<A: VulkanAsset>(&mut self) {
        let render_app = self.get_sub_app_mut(RenderApp).unwrap();
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
