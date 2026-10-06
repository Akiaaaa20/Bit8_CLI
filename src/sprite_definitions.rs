//! Optional immutable project-level Sprite definitions.
use crate::project_assets::{
    load_registered_tilesheets, parse_asset_reference, resolve_asset_reference,
};
use crate::tilesheet::Tilesheet;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

pub const SPRITES_FILE: &str = "bit8.sprites.toml";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AnimationDefinition {
    pub name: String,
    /// Stable tile symbols, not legacy numeric indices or copied pixels.
    pub frames: Vec<String>,
    pub fps: u8,
    #[serde(rename = "loop")]
    pub looped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteDefinition {
    pub name: String,
    pub preview: Option<String>,
    /// Source declaration order, including for preview fallback.
    pub animations: Vec<AnimationDefinition>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpriteRegistry {
    pub sprites: Vec<SpriteDefinition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileData {
    version: u32,
    #[serde(default)]
    sprite: BTreeMap<String, toml::Spanned<SpriteData>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpriteData {
    preview: Option<String>,
    #[serde(default)]
    animation: BTreeMap<String, toml::Spanned<AnimationData>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnimationData {
    frames: Vec<String>,
    fps: i64,
    #[serde(rename = "loop", default = "default_loop")]
    looped: bool,
}

fn default_loop() -> bool {
    true
}

fn invalid(message: impl Into<String>) -> IoError {
    IoError::new(
        ErrorKind::InvalidData,
        format!("{SPRITES_FILE}: {}", message.into()),
    )
}

impl SpriteRegistry {
    /// Read-only frontend DTO, preserving core preview rules and declaration order.
    pub fn inspection(&self) -> Vec<SpriteInspection> {
        self.sprites
            .iter()
            .map(|sprite| SpriteInspection {
                name: sprite.name.clone(),
                preview: self.effective_preview(&sprite.name).map(str::to_owned),
                animations: sprite.animations.clone(),
            })
            .collect()
    }
    /// Parse structure independently from project asset-reference validation.
    pub fn parse(source: &str) -> Result<Self, Box<dyn Error>> {
        let file: FileData = toml::from_str(source).map_err(|error| invalid(format!("{error}")))?;
        if file.version != 1 {
            return Err(
                invalid(format!("unsupported version {}; expected 1", file.version)).into(),
            );
        }
        let mut sprites: Vec<_> = file.sprite.into_iter().collect();
        sprites.sort_by_key(|(_, data)| data.span().start);
        let mut definitions = Vec::new();
        for (name, data) in sprites {
            if name.trim().is_empty() {
                return Err(invalid("Sprite name must not be empty").into());
            }
            let data = data.into_inner();
            let mut animations: Vec<_> = data.animation.into_iter().collect();
            animations.sort_by_key(|(_, data)| data.span().start);
            let mut animation_definitions = Vec::new();
            for (animation, data) in animations {
                let data = data.into_inner();
                let context = format!("Sprite {name:?} animation {animation:?}");
                if animation.trim().is_empty() {
                    return Err(invalid(format!("{context} name must not be empty")).into());
                }
                if data.frames.is_empty() {
                    return Err(invalid(format!("{context} has no frames")).into());
                }
                if !(1..=30).contains(&data.fps) {
                    return Err(invalid(format!(
                        "{context} fps {} must be between 1 and 30",
                        data.fps
                    ))
                    .into());
                }
                animation_definitions.push(AnimationDefinition {
                    name: animation,
                    frames: data.frames,
                    fps: data.fps as u8,
                    looped: data.looped,
                });
            }
            definitions.push(SpriteDefinition {
                name,
                preview: data.preview,
                animations: animation_definitions,
            });
        }
        Ok(Self {
            sprites: definitions,
        })
    }

    pub fn get_sprite(&self, name: &str) -> Option<&SpriteDefinition> {
        self.sprites.iter().find(|sprite| sprite.name == name)
    }

    pub fn get_animation(&self, sprite: &str, animation: &str) -> Option<&AnimationDefinition> {
        self.get_sprite(sprite)?
            .animations
            .iter()
            .find(|item| item.name == animation)
    }

    pub fn effective_preview(&self, sprite: &str) -> Option<&str> {
        let sprite = self.get_sprite(sprite)?;
        sprite.preview.as_deref().or_else(|| {
            sprite
                .animations
                .iter()
                .find(|animation| animation.name == "idle")
                .or_else(|| sprite.animations.first())?
                .frames
                .first()
                .map(String::as_str)
        })
    }

    /// Reuse exactly the existing registered group/cell resolution rules.
    pub fn validate_assets(
        &self,
        sheets: &HashMap<String, Tilesheet>,
    ) -> Result<(), Box<dyn Error>> {
        let validate = |symbol: &str, context: String| -> Result<(), Box<dyn Error>> {
            let reference = parse_asset_reference(symbol)
                .ok_or_else(|| invalid(format!("{context} uses invalid tile ID {symbol:?}")))?;
            resolve_asset_reference(&reference, sheets).map_err(|error| {
                invalid(format!(
                    "{context} uses unknown or invalid tile ID {symbol:?}: {error}"
                ))
            })?;
            Ok(())
        };
        for sprite in &self.sprites {
            if let Some(preview) = &sprite.preview {
                validate(preview, format!("Sprite {:?} preview", sprite.name))?;
            }
            for animation in &sprite.animations {
                for frame in &animation.frames {
                    validate(
                        frame,
                        format!("Sprite {:?} animation {:?}", sprite.name, animation.name),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Absence returns None without loading assets or creating any file.
    pub fn load_project(project: &Path) -> Result<Option<Self>, Box<dyn Error>> {
        let path = project.join(SPRITES_FILE);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(
                    IoError::new(error.kind(), format!("{}: {error}", path.display())).into(),
                );
            }
        };
        let definitions = Self::parse(&source)?;
        definitions.validate_assets(&load_registered_tilesheets(project)?)?;
        Ok(Some(definitions))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SpriteInspection {
    pub name: String,
    pub preview: Option<String>,
    pub animations: Vec<AnimationDefinition>,
}
