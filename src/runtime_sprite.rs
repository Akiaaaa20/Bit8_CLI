//! Per-Node playback; definitions and tile rendering remain shared.
use crate::sprite_definitions::SpriteRegistry;

const TICK_HZ: u8 = crate::runtime_timing::FIXED_HZ as u8;

#[derive(Default)]
pub(crate) struct NodeRuntimeSpriteState {
    sprite: Option<String>,
    animation: Option<String>,
    frame: usize,
    accumulator: u8,
    finished: bool,
    // Detect a selection made during update, so its first frame is presented.
    pub revision: u64,
}

impl NodeRuntimeSpriteState {
    pub fn bind(&mut self, registry: &SpriteRegistry, name: &str) -> Result<(), String> {
        if registry.get_sprite(name).is_none() {
            return Err(format!("Unknown sprite {name:?}"));
        }
        if self.sprite.as_deref() != Some(name) {
            self.sprite = Some(name.to_owned());
            self.animation = None;
            self.reset();
        }
        Ok(())
    }

    pub fn play(
        &mut self,
        registry: &SpriteRegistry,
        name: &str,
        node: &str,
    ) -> Result<(), String> {
        let sprite = self
            .sprite
            .as_deref()
            .ok_or_else(|| format!("Node {node:?} has no sprite"))?;
        if registry.get_animation(sprite, name).is_none() {
            return Err(format!("Sprite {sprite:?} has no animation {name:?}"));
        }
        if self.animation.as_deref() != Some(name) {
            self.animation = Some(name.to_owned());
            self.reset();
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.frame = 0;
        self.accumulator = 0;
        self.finished = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn advance(&mut self, registry: &SpriteRegistry) {
        let Some(animation) = self
            .sprite
            .as_deref()
            .zip(self.animation.as_deref())
            .and_then(|(sprite, animation)| registry.get_animation(sprite, animation))
        else {
            return;
        };
        if self.finished {
            return;
        }
        self.accumulator += animation.fps;
        while self.accumulator >= TICK_HZ {
            self.accumulator -= TICK_HZ;
            if self.frame + 1 < animation.frames.len() {
                self.frame += 1;
            } else if animation.looped {
                self.frame = 0;
            } else {
                self.finished = true;
                self.accumulator = 0;
                break;
            }
        }
    }

    pub fn tile<'a>(&self, registry: &'a SpriteRegistry) -> Option<&'a str> {
        let sprite = self.sprite.as_deref()?;
        if let Some(animation) = self.animation.as_deref() {
            registry
                .get_animation(sprite, animation)?
                .frames
                .get(self.frame)
                .map(String::as_str)
        } else {
            registry.effective_preview(sprite)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(fps: u8, looped: bool) -> SpriteRegistry {
        SpriteRegistry::parse(&format!(
            "version=1\n[sprite.Player]\npreview='A4'\n[sprite.Player.animation.walk]\nframes=['A1','A2','A3']\nfps={fps}\nloop={looped}\n[sprite.Player.animation.idle]\nframes=['A4']\nfps=2\n[sprite.Cat]\npreview='B1'\n[sprite.Empty]\n"
        )).unwrap()
    }

    fn playing(registry: &SpriteRegistry) -> NodeRuntimeSpriteState {
        let mut state = NodeRuntimeSpriteState::default();
        state.bind(registry, "Player").unwrap();
        state.play(registry, "walk", "One").unwrap();
        assert_eq!(state.frame, 0);
        assert_eq!(state.accumulator, 0);
        assert!(!state.finished);
        state
    }

    #[test]
    fn binding_and_preview_errors_and_empty_states() {
        let registry = registry(8, true);
        let mut state = NodeRuntimeSpriteState::default();
        assert_eq!(state.tile(&registry), None);
        assert!(
            state
                .play(&registry, "walk", "One")
                .unwrap_err()
                .contains("Node \"One\" has no sprite")
        );
        assert!(
            state
                .bind(&registry, "Missing")
                .unwrap_err()
                .contains("Unknown sprite \"Missing\"")
        );
        state.bind(&registry, "Player").unwrap();
        assert_eq!(state.tile(&registry), Some("A4"));
        assert!(state.animation.is_none());
        assert!(
            state
                .play(&registry, "fly", "One")
                .unwrap_err()
                .contains("Sprite \"Player\" has no animation \"fly\"")
        );
        state.bind(&registry, "Empty").unwrap();
        assert_eq!(state.tile(&registry), None);
    }

    #[test]
    fn same_sprite_and_animation_are_idempotent_and_different_sprite_resets() {
        let registry = registry(8, true);
        let mut state = playing(&registry);
        for _ in 0..4 {
            state.advance(&registry);
        }
        assert_eq!((state.frame, state.accumulator), (1, 2));
        let revision = state.revision;
        state.bind(&registry, "Player").unwrap();
        state.play(&registry, "walk", "One").unwrap();
        assert_eq!(
            (state.frame, state.accumulator, state.revision),
            (1, 2, revision)
        );
        state.bind(&registry, "Cat").unwrap();
        assert_eq!(
            (state.frame, state.accumulator, state.finished),
            (0, 0, false)
        );
        assert!(state.animation.is_none());
        assert_eq!(state.tile(&registry), Some("B1"));
    }

    #[test]
    fn switching_animation_resets() {
        let registry = registry(8, true);
        let mut state = playing(&registry);
        for _ in 0..4 {
            state.advance(&registry);
        }
        state.play(&registry, "idle", "One").unwrap();
        assert_eq!((state.frame, state.accumulator), (0, 0));
        assert_eq!(state.tile(&registry), Some("A4"));
        state.play(&registry, "walk", "One").unwrap();
        assert_eq!(state.tile(&registry), Some("A1"));
    }

    fn assert_timing(fps: u8) {
        let registry = registry(fps, true);
        let mut state = playing(&registry);
        for tick in 1..=300 {
            state.advance(&registry);
            assert_eq!(state.frame, (tick * usize::from(fps) / 30) % 3);
            assert_eq!(usize::from(state.accumulator), tick * usize::from(fps) % 30);
            assert!(!state.finished);
        }
    }
    #[test]
    fn one_fps_integer_timing() {
        assert_timing(1);
    }
    #[test]
    fn eight_fps_integer_remainders() {
        assert_timing(8);
    }
    #[test]
    fn thirty_fps_advances_each_tick_and_wraps() {
        assert_timing(30);
    }

    #[test]
    fn nonloop_holds_finished_and_only_away_back_restarts() {
        let registry = registry(30, false);
        let mut state = playing(&registry);
        state.advance(&registry);
        state.advance(&registry);
        assert_eq!(state.tile(&registry), Some("A3"));
        assert!(!state.finished); // Final frame receives its own display interval.
        state.advance(&registry);
        assert!(state.finished);
        for _ in 0..100 {
            state.play(&registry, "walk", "One").unwrap();
            state.advance(&registry);
            assert_eq!(state.tile(&registry), Some("A3"));
            assert!(state.finished);
        }
        state.play(&registry, "idle", "One").unwrap();
        state.play(&registry, "walk", "One").unwrap();
        assert_eq!(
            (state.frame, state.accumulator, state.finished),
            (0, 0, false)
        );
    }

    #[test]
    fn shared_definitions_do_not_share_playback_or_draw_time() {
        let registry = registry(30, true);
        let mut one = playing(&registry);
        let two = playing(&registry);
        for _ in 0..20 {
            assert_eq!(one.tile(&registry), Some("A1"));
        }
        assert_eq!((one.frame, one.accumulator), (0, 0));
        one.advance(&registry);
        assert_eq!(one.tile(&registry), Some("A2"));
        assert_eq!(two.tile(&registry), Some("A1"));
    }
}
