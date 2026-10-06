use bit8::project_assets::load_registered_tilesheets;
use bit8::runtime_session::RuntimeSession;
use bit8::sprite_definitions::{SPRITES_FILE, SpriteRegistry};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const IDLE: &str = "version = 1\n[sprite.Player.animation.idle]\nframes = [\"A4\"]\nfps = 2\n";

#[test]
fn inspection_preserves_core_previews_and_ordered_animation_metadata() {
    let registry = SpriteRegistry::parse(
        "version=1\n[sprite.Explicit]\npreview='A4'\n[sprite.Explicit.animation.walk]\nframes=['A7','A5','A6']\nfps=8\nloop=false\n[sprite.Explicit.animation.idle]\nframes=['A1']\nfps=2\nloop=true\n[sprite.Idle.animation.walk]\nframes=['A2']\nfps=8\n[sprite.Idle.animation.idle]\nframes=['A3']\nfps=2\n[sprite.First.animation.zed]\nframes=['B1','B2']\nfps=3\n[sprite.First.animation.aaa]\nframes=['A1']\nfps=4\n[sprite.Empty]\n",
    ).unwrap();
    let dto = serde_json::to_value(registry.inspection()).unwrap();
    let get = |name: &str| {
        dto.as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == name)
            .unwrap()
    };
    assert_eq!(get("Explicit")["preview"], "A4");
    assert_eq!(get("Idle")["preview"], "A3");
    assert_eq!(get("First")["preview"], "B1");
    assert!(get("Empty")["preview"].is_null());
    assert_eq!(get("Empty")["animations"], serde_json::json!([]));
    assert_eq!(
        get("Explicit")["animations"],
        serde_json::json!([
            {"name":"walk","frames":["A7","A5","A6"],"fps":8,"loop":false},
            {"name":"idle","frames":["A1"],"fps":2,"loop":true}
        ])
    );
    assert_eq!(get("First")["animations"][0]["name"], "zed");
}

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bit8-sprite-data-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn assets(&self) {
        for (group, width) in [("A", 32), ("B", 16)] {
            image::RgbaImage::from_pixel(width, 8, image::Rgba([255, 0, 77, 255]))
                .save(self.0.join(format!("{group}.png")))
                .unwrap();
        }
        fs::write(self.0.join("bit8.assets.toml"), "version = 1\n[[tilesheets]]\ngroup = \"B\"\nfile = \"B.png\"\n[[tilesheets]]\ngroup = \"A\"\nfile = \"A.png\"\n").unwrap();
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn one_animation_and_queries() {
    let registry = SpriteRegistry::parse(IDLE).unwrap();
    let idle = registry.get_animation("Player", "idle").unwrap();
    assert_eq!(idle.frames, ["A4"]);
    assert_eq!(idle.fps, 2);
    assert!(registry.get_sprite("missing").is_none());
    assert!(registry.get_animation("Player", "missing").is_none());
}

#[test]
fn multiple_animations_preserve_source_order() {
    let registry = SpriteRegistry::parse(&format!("{IDLE}\n[sprite.Player.animation.walk]\nframes=[\"A1\",\"A2\"]\nfps=8\n[sprite.Player.animation.jump]\nframes=[\"A3\"]\nfps=6\nloop=false\n")).unwrap();
    assert_eq!(
        registry
            .get_sprite("Player")
            .unwrap()
            .animations
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["idle", "walk", "jump"]
    );
}

#[test]
fn multiple_sprites() {
    let registry = SpriteRegistry::parse(&format!("{IDLE}\n[sprite.Slime]\npreview=\"B1\"\n[sprite.Slime.animation.idle]\nframes=[\"B1\",\"B2\"]\nfps=4\n")).unwrap();
    assert_eq!(registry.sprites.len(), 2);
    assert_eq!(registry.effective_preview("Slime"), Some("B1"));
}

#[test]
fn explicit_preview_wins_and_remains_explicit() {
    let registry = SpriteRegistry::parse("version=1\n[sprite.Player]\npreview=\"A1\"\n[sprite.Player.animation.idle]\nframes=[\"A4\"]\nfps=2").unwrap();
    assert_eq!(registry.effective_preview("Player"), Some("A1"));
    assert_eq!(
        registry.get_sprite("Player").unwrap().preview.as_deref(),
        Some("A1")
    );
}

#[test]
fn idle_preview_is_derived_and_takes_priority_over_first_animation() {
    let registry = SpriteRegistry::parse("version=1\n[sprite.Player.animation.walk]\nframes=[\"A1\"]\nfps=8\n[sprite.Player.animation.idle]\nframes=[\"A4\",\"A3\"]\nfps=2").unwrap();
    assert_eq!(registry.effective_preview("Player"), Some("A4"));
    assert!(registry.get_sprite("Player").unwrap().preview.is_none());
}

#[test]
fn no_idle_uses_first_declared_not_alphabetically_first_animation() {
    let source = "version=1\n[sprite.Player.animation.z_walk]\nframes=[\"A4\"]\nfps=8\n[sprite.Player.animation.a_jump]\nframes=[\"A1\"]\nfps=6";
    for _ in 0..10 {
        let registry = SpriteRegistry::parse(source).unwrap();
        assert_eq!(registry.effective_preview("Player"), Some("A4"));
    }
}

#[test]
fn inline_animation_declaration_order_is_also_preserved() {
    let registry = SpriteRegistry::parse("version=1\n[sprite.Player]\nanimation = { z_walk = { frames = [\"A4\"], fps = 8 }, a_jump = { frames = [\"A1\"], fps = 6 } }").unwrap();
    assert_eq!(registry.effective_preview("Player"), Some("A4"));
}

#[test]
fn sprite_without_animations_has_no_preview() {
    let registry = SpriteRegistry::parse("version=1\n[sprite.Player]").unwrap();
    assert_eq!(registry.effective_preview("Player"), None);
    assert_eq!(registry.effective_preview("missing"), None);
}

#[test]
fn omitted_loop_defaults_to_true() {
    assert!(
        SpriteRegistry::parse(IDLE)
            .unwrap()
            .get_animation("Player", "idle")
            .unwrap()
            .looped
    );
}

#[test]
fn explicit_loop_false() {
    assert!(
        !SpriteRegistry::parse(&format!("{IDLE}loop=false\n"))
            .unwrap()
            .get_animation("Player", "idle")
            .unwrap()
            .looped
    );
}

#[test]
fn fps_one_is_valid() {
    SpriteRegistry::parse(&IDLE.replace("fps = 2", "fps = 1")).unwrap();
}

#[test]
fn fps_thirty_is_valid() {
    SpriteRegistry::parse(&IDLE.replace("fps = 2", "fps = 30")).unwrap();
}

#[test]
fn fps_zero_is_invalid() {
    assert!(
        SpriteRegistry::parse(&IDLE.replace("fps = 2", "fps = 0"))
            .unwrap_err()
            .to_string()
            .contains("Sprite \"Player\" animation \"idle\" fps 0")
    );
}

#[test]
fn fps_thirty_one_is_invalid() {
    assert!(
        SpriteRegistry::parse(&IDLE.replace("fps = 2", "fps = 31"))
            .unwrap_err()
            .to_string()
            .contains("between 1 and 30")
    );
}

#[test]
fn empty_frames_are_invalid() {
    assert!(
        SpriteRegistry::parse(&IDLE.replace("[\"A4\"]", "[]"))
            .unwrap_err()
            .to_string()
            .contains("Sprite \"Player\" animation \"idle\" has no frames")
    );
}

#[test]
fn stable_group_references_use_existing_registry() {
    let project = Project::new();
    project.assets();
    let registry =
        SpriteRegistry::parse(&format!("{IDLE}\n[sprite.Slime]\npreview=\"B2\"\n")).unwrap();
    registry
        .validate_assets(&load_registered_tilesheets(&project.0).unwrap())
        .unwrap();
}

#[test]
fn unknown_preview_is_separate_from_structure_parsing() {
    let registry = SpriteRegistry::parse("version=1\n[sprite.Player]\npreview=\"Z999\"").unwrap();
    let error = registry
        .validate_assets(&Default::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("Sprite \"Player\" preview"));
    assert!(error.contains("Z999"));
}

#[test]
fn invalid_and_out_of_range_frames_are_not_removed_or_substituted() {
    let project = Project::new();
    project.assets();
    let sheets = load_registered_tilesheets(&project.0).unwrap();
    for symbol in ["INVALID", "A99", "A0", "Z1", "3", "A.png"] {
        let registry = SpriteRegistry::parse(&IDLE.replace("A4", symbol)).unwrap();
        let error = registry.validate_assets(&sheets).unwrap_err().to_string();
        assert!(error.contains("Player") && error.contains("idle") && error.contains(symbol));
        assert_eq!(
            registry.get_animation("Player", "idle").unwrap().frames,
            [symbol]
        );
    }
}

#[test]
fn optional_missing_file_does_not_create_files_or_require_registry() {
    let project = Project::new();
    assert!(SpriteRegistry::load_project(&project.0).unwrap().is_none());
    assert_eq!(fs::read_dir(&project.0).unwrap().count(), 0);
}

#[test]
fn project_loading_validates_without_rewriting_inferred_preview() {
    let project = Project::new();
    project.assets();
    fs::write(project.0.join(SPRITES_FILE), IDLE).unwrap();
    let registry = SpriteRegistry::load_project(&project.0).unwrap().unwrap();
    assert_eq!(registry.effective_preview("Player"), Some("A4"));
    assert!(registry.get_sprite("Player").unwrap().preview.is_none());
    assert_eq!(
        fs::read_to_string(project.0.join(SPRITES_FILE)).unwrap(),
        IDLE
    );
}

#[test]
fn legacy_runtime_and_direct_a4_drawing_are_unchanged() {
    let project = Project::new();
    project.assets();
    fs::write(
        project.0.join("main.b8"),
        "func draw() cls(0) spr(A4,0,0) sprite(A4,8,0) end",
    )
    .unwrap();
    let mut session = RuntimeSession::start(&project.0).unwrap();
    let expected = session.step(0).unwrap().pixels().to_vec();
    assert_eq!(expected[0], 0xff004d);
    assert_eq!(expected[8], 0xff004d);
    assert!(!project.0.join(SPRITES_FILE).exists());
    // Valid optional definitions do not reinterpret the direct tile APIs.
    fs::write(project.0.join(SPRITES_FILE), IDLE).unwrap();
    let mut session = RuntimeSession::start(&project.0).unwrap();
    assert_eq!(session.step(0).unwrap().pixels(), expected.as_slice());
}

#[test]
fn malformed_toml_types_versions_and_duplicate_names_fail_safely() {
    for source in [
        "version = [",
        "version=2",
        "version=1\n[sprite.Player]\n[sprite.Player]",
        "version=1\n[sprite.\"\"]",
        "version=1\n[sprite.\"  \"]",
        "version=1\n[sprite.P.animation.idle]\nframes=[\"A1\"]\nfps=2.5",
        "version=1\n[sprite.P.animation.idle]\nframes=[1]\nfps=2",
        "version=1\n[sprite.P.animation.idle]\nframes=[\"A1\"]\nfps=2\nloop=\"true\"",
        "version=1\n[sprite.P.animation.idle]\nframes=[\"A1\"]\nfps=2\n[sprite.P.animation.idle]",
    ] {
        assert!(
            SpriteRegistry::parse(source)
                .unwrap_err()
                .to_string()
                .contains(SPRITES_FILE),
            "{source}"
        );
    }
}
