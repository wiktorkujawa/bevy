//! First-party `.bsn` asset loading (experimental).
//!
//! Full BSN-on-disk parity with the `bsn!` macro is not implemented yet.
//! This module ships:
//! - [`BsnSource`] — raw UTF-8 `.bsn` text as an [`Asset`]
//! - [`BsnAssetLoader`] — loads `.bsn` into [`BsnSource`]
//! - [`parse_minimal_bsn`] — parses a small `#Name` + `Transform::from_xyz` subset
//! - [`spawn_minimal_bsn_names`] — spawns parsed entities with [`Name`]
//!
//! The architecture for loading `.bsn` as [`ScenePatch`] already exists (see
//! crate docs / FakeSceneLoader tests). This module is the first shipped step
//! toward that path.

use alloc::{string::String, vec::Vec};
use bevy_asset::{io::Reader, Asset, AssetApp, AssetLoader, LoadContext};
use bevy_ecs::{name::Name, world::World};
use bevy_reflect::TypePath;
use thiserror::Error;

/// Raw `.bsn` file contents.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct BsnSource {
    /// UTF-8 source text.
    pub source: String,
}

/// A single entity parsed from the minimal BSN subset.
#[derive(Debug, Clone, PartialEq)]
pub struct MinimalBsnEntity {
    /// Entity display name (`#Name` syntax).
    pub name: String,
    /// Translation, if a `Transform::from_xyz` line was present.
    pub translation: Option<[f32; 3]>,
}

/// Errors while reading or parsing `.bsn` assets.
#[derive(Error, Debug)]
pub enum BsnAssetError {
    /// Underlying I/O / asset reader failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Source was not valid UTF-8.
    #[error("BSN source is not valid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    /// Minimal parser failure.
    #[error("minimal BSN parse error: {0}")]
    Parse(String),
}

/// Loads `.bsn` files into [`BsnSource`].
#[derive(Default, TypePath)]
pub struct BsnAssetLoader;

impl AssetLoader for BsnAssetLoader {
    type Asset = BsnSource;
    type Settings = ();
    type Error = BsnAssetError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = String::from_utf8(bytes)?;
        Ok(BsnSource { source })
    }

    fn extensions(&self) -> &[&str] {
        &["bsn"]
    }
}

/// Parses a minimal BSN-like document:
///
/// ```text
/// #LeftCube
/// Transform::from_xyz(-1.5, 0.5, 0.0)
/// --
/// #RightCube
/// Transform::from_xyz(1.5, 0.5, 0.0)
/// ```
///
/// Comments starting with `//` and a wrapping `bsn_list! { ... }` are ignored.
pub fn parse_minimal_bsn(source: &str) -> Result<Vec<MinimalBsnEntity>, BsnAssetError> {
    let mut entities = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_translation: Option<[f32; 3]> = None;

    let flush = |entities: &mut Vec<MinimalBsnEntity>,
                 name: &mut Option<String>,
                 translation: &mut Option<[f32; 3]>| {
        if let Some(name) = name.take() {
            entities.push(MinimalBsnEntity {
                name,
                translation: translation.take(),
            });
        } else {
            *translation = None;
        }
    };

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with("//")
            || line.starts_with("bsn_list!")
            || line == "{"
            || line == "}"
        {
            continue;
        }
        if line == "--" {
            flush(&mut entities, &mut current_name, &mut current_translation);
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            flush(&mut entities, &mut current_name, &mut current_translation);
            let name = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| BsnAssetError::Parse("empty #Name".into()))?
                .to_string();
            current_name = Some(name);
            continue;
        }
        if let Some(coords) = parse_from_xyz(line) {
            current_translation = Some(coords);
            continue;
        }
        // Ignore richer Transform { ... } blocks and unknown lines for now.
        if line.starts_with("Transform") {
            continue;
        }
    }
    flush(&mut entities, &mut current_name, &mut current_translation);
    Ok(entities)
}

fn parse_from_xyz(line: &str) -> Option<[f32; 3]> {
    let start = line.find("Transform::from_xyz(")?;
    let after = &line[start + "Transform::from_xyz(".len()..];
    let end = after.find(')')?;
    let inner = &after[..end];
    let mut parts = inner.split(',').map(|p| p.trim().parse::<f32>().ok());
    let x = parts.next()??;
    let y = parts.next()??;
    let z = parts.next()??;
    Some([x, y, z])
}

/// Spawns each parsed minimal entity into `world` with a custom spawn callback
/// (so callers can attach `Transform` without adding a `bevy_transform` dependency here).
pub fn spawn_minimal_bsn_with<F>(world: &mut World, entities: &[MinimalBsnEntity], mut spawn_one: F)
where
    F: FnMut(&mut World, &MinimalBsnEntity),
{
    for entity in entities {
        spawn_one(world, entity);
    }
}

/// Convenience spawn using only [`Name`].
pub fn spawn_minimal_bsn_names(world: &mut World, entities: &[MinimalBsnEntity]) {
    spawn_minimal_bsn_with(world, entities, |world, entity| {
        world.spawn(Name::new(entity.name.clone()));
    });
}

/// Registers [`BsnSource`] and [`BsnAssetLoader`].
pub fn register_bsn_asset(app: &mut bevy_app::App) {
    app.init_asset::<BsnSource>()
        .register_asset_loader(BsnAssetLoader);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_two_named_entities() {
        let src = r#"
// comment
bsn_list! {
    #LeftCube
    Transform::from_xyz(-1.5, 0.5, 0.0)
    --
    #RightCube
    Transform::from_xyz(1.5, 0.5, 0.0)
}
"#;
        let entities = parse_minimal_bsn(src).unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].name, "LeftCube");
        assert_eq!(entities[0].translation, Some([-1.5, 0.5, 0.0]));
        assert_eq!(entities[1].name, "RightCube");
        assert_eq!(entities[1].translation, Some([1.5, 0.5, 0.0]));
    }
}
