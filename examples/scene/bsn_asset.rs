//! Loads an experimental `.bsn` file via [`BsnSource`] / [`BsnAssetLoader`],
//! parses the minimal `#Name` + `Transform::from_xyz` subset, and spawns cubes.
//!
//! ```bash
//! cargo run -p bevy --example bsn_asset
//! ```

use bevy::{
    prelude::*,
    scene::{parse_minimal_bsn, spawn_minimal_bsn_with, BsnSource},
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, (setup_camera_light, load_bsn))
        .add_systems(Update, spawn_when_ready)
        .run();
}

#[derive(Resource)]
struct PendingBsn(Handle<BsnSource>);

#[derive(Resource, Default)]
struct BsnSpawned(bool);

fn setup_camera_light(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-4.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn load_bsn(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(PendingBsn(asset_server.load("scenes/demo_minimal.bsn")));
    commands.init_resource::<BsnSpawned>();
}

fn spawn_when_ready(
    mut commands: Commands,
    pending: Option<Res<PendingBsn>>,
    sources: Res<Assets<BsnSource>>,
    mut spawned: ResMut<BsnSpawned>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if spawned.0 {
        return;
    }
    let Some(pending) = pending else {
        return;
    };
    let Some(source) = sources.get(&pending.0) else {
        return;
    };
    let Ok(entities) = parse_minimal_bsn(&source.source) else {
        error!("failed to parse demo_minimal.bsn");
        spawned.0 = true;
        return;
    };

    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let material = materials.add(Color::srgb(0.3, 0.7, 0.4));

    let count = entities.len();
    commands.queue(move |world: &mut World| {
        spawn_minimal_bsn_with(world, &entities, |world, entity| {
            let mut e = world.spawn(Name::new(entity.name.clone()));
            if let Some([x, y, z]) = entity.translation {
                e.insert((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_xyz(x, y, z),
                ));
            }
        });
    });
    info!("spawned {count} entities from .bsn");
    spawned.0 = true;
}
