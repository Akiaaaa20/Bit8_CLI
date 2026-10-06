use bit8::framebuffer::Framebuffer;
use bit8::project_assets::load_registered_tilesheets;
use bit8::runtime_session::RuntimeSession;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const COLORS: [u32; 4] = [0x1d2b53, 0x7e2553, 0x008751, 0xab5236];
const DEFINITIONS: &str = "version=1\n[sprite.Player]\npreview='A4'\n[sprite.Player.animation.walk]\nframes=['A1','A2','A3','A4']\nfps=8\nloop=true\n[sprite.Player.animation.fast]\nframes=['A1','A2','A3','A4']\nfps=30\nloop=true\n[sprite.Player.animation.attack]\nframes=['A1','A2']\nfps=30\nloop=false\n[sprite.Empty]\n[sprite.Preview]\npreview='A4'\n[sprite.Fallback.animation.idle]\nframes=['A3']\nfps=1\n";

struct Project(PathBuf);
impl Project {
    fn new(script: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bit8-animation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let project = Self(path);
        project.write("main.b8", "func draw() cls(0) end\n");
        project.write("player.b8", script);
        project.write("bit8.sprites.toml", DEFINITIONS);
        project.write(
            "bit8.assets.toml",
            "version=1\n[[tilesheets]]\ngroup='A'\nfile='tilesheet.png'\nsolid=[4]\n",
        );
        let image = image::RgbaImage::from_fn(32, 8, |x, _| {
            let color = COLORS[(x / 8) as usize];
            image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255])
        });
        image.save(project.0.join("tilesheet.png")).unwrap();
        project.nodes("", "");
        project
    }
    fn write(&self, file: &str, source: &str) {
        fs::write(self.0.join(file), source).unwrap();
    }
    fn nodes(&self, player_extra: &str, more_nodes: &str) {
        self.write("world.b8map", &format!("version=1\nwidth=8\nheight=8\n{}\n[node_state]\nnext_id=4\n[[node_state.nodes]]\nid='N1'\nname='Player'\nx=0\ny=0\nenabled=true\nscript='player.b8'\n{player_extra}\n{more_nodes}",["-- -- -- -- -- -- -- --";8].join("\n")));
    }
    fn start(&self) -> RuntimeSession {
        RuntimeSession::start(&self.0).unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn persisted_map_sprite_round_trips_and_clearing_preserves_legacy_data() {
    use bit8::map::Bit8Map;
    let project = Project::new("");
    project.nodes("sprite='Player'\nvisual='A3'\n[node_state.nodes.collider]\nenabled=true\noffset_x=0\noffset_y=0\nwidth=8\nheight=8", "");
    let path = project.0.join("world.b8map");
    let map = Bit8Map::load(&path, &project.0).unwrap();
    assert_eq!(map.nodes()[0].sprite.as_deref(), Some("Player"));
    let serialized = map.serialize().unwrap();
    assert!(serialized.contains("sprite = \"Player\""));
    project.write("world.b8map", &serialized);
    assert_eq!(Bit8Map::load(&path, &project.0).unwrap(), map);
    project.write(
        "world.b8map",
        &serialized.replace("sprite = \"Player\"\n", ""),
    );
    let cleared = Bit8Map::load(&path, &project.0).unwrap();
    assert!(cleared.nodes()[0].sprite.is_none());
    assert_eq!(cleared.nodes()[0].visual.as_deref(), Some("A3"));
    assert_eq!(cleared.nodes()[0].collider, map.nodes()[0].collider);
    assert_eq!(cleared.cells(), map.cells());
    assert_eq!(cleared.nodes()[0].script, map.nodes()[0].script);
}

#[test]
fn persisted_binding_precedes_init_play_and_uses_same_preview_and_animation() {
    let project = Project::new(
        "func init() self:play('walk') self:spr() end\nfunc update() if btn(RIGHT) then self:move(1,0) end self:play('walk') end\nfunc draw() self:spr() end",
    );
    project.nodes("sprite='Player'", "");
    let mut session = project.start();
    assert_eq!(session.framebuffer().pixels()[0], COLORS[0]);
    for tick in 1..=4 {
        let frame = session.step(bit8::input::Button::Right.mask()).unwrap();
        assert_eq!(frame.pixels()[tick], COLORS[tick * 8 / 30]);
    }
    assert!(
        !fs::read_to_string(project.0.join("player.b8"))
            .unwrap()
            .contains("self:sprite")
    );
}

#[test]
fn persisted_player_idle_walk_fixture_works_without_explicit_script_binding() {
    let project = Project::new(
        "func init() self:play('idle') end\nfunc update() if btn(RIGHT) then self:move(1,0) self:play('walk') else self:play('idle') end end\nfunc draw() self:spr() end",
    );
    let colors = [
        0x1d2b53_u32,
        0x7e2553,
        0x008751,
        0xab5236,
        0xff004d,
        0xffa300,
        0xffec27,
    ];
    let image = image::RgbaImage::from_fn(56, 8, |x, _| {
        let c = colors[(x / 8) as usize];
        image::Rgba([(c >> 16) as u8, (c >> 8) as u8, c as u8, 255])
    });
    image.save(project.0.join("tilesheet.png")).unwrap();
    project.write("bit8.sprites.toml","version=1\n[sprite.Player]\npreview='A4'\n[sprite.Player.animation.idle]\nframes=['A4']\nfps=2\n[sprite.Player.animation.walk]\nframes=['A4','A5','A6','A7']\nfps=8\nloop=true\n");
    project.nodes("sprite='Player'", "");
    let mut session = project.start();
    for tick in 1..=10 {
        let frame = session.step(bit8::input::Button::Right.mask()).unwrap();
        assert_eq!(frame.pixels()[tick], colors[3 + ((tick - 1) * 8 / 30) % 4]);
    }
    assert_eq!(session.step(0).unwrap().pixels()[10], colors[3]);
    assert!(
        !fs::read_to_string(project.0.join("player.b8"))
            .unwrap()
            .contains("self:sprite")
    );
}

#[test]
fn persisted_unknown_sprite_fails_map_and_runtime_even_without_script() {
    use bit8::map::Bit8Map;
    for scripted in [true, false] {
        let project = Project::new("");
        project.nodes("sprite='Missing'", "");
        if !scripted {
            let source = fs::read_to_string(project.0.join("world.b8map"))
                .unwrap()
                .replace("script='player.b8'\n", "");
            project.write("world.b8map", &source);
        }
        let map_error = Bit8Map::load(&project.0.join("world.b8map"), &project.0)
            .unwrap_err()
            .to_string();
        let runtime_error = RuntimeSession::start(&project.0).err().unwrap().to_string();
        for error in [map_error, runtime_error] {
            for context in ["world.b8map", "N1", "Player", "Missing", "unknown sprite"] {
                assert!(error.contains(context), "{error}");
            }
        }
    }
}

#[test]
fn runtime_override_is_not_persisted_and_sprite_does_not_implicitly_draw() {
    let project =
        Project::new("func init() self:sprite('Fallback') end\nfunc draw() self:spr() end");
    project.nodes("sprite='Player'\nvisual='A4'", "");
    let original = fs::read_to_string(project.0.join("world.b8map")).unwrap();
    assert_eq!(project.start().step(0).unwrap().pixels()[0], COLORS[2]);
    assert_eq!(
        fs::read_to_string(project.0.join("world.b8map")).unwrap(),
        original
    );
    project.write("player.b8", "func init() self:play('walk') end");
    assert!(
        project
            .start()
            .step(0)
            .unwrap()
            .pixels()
            .iter()
            .all(|pixel| *pixel == 0)
    );
}

#[test]
fn two_persisted_nodes_share_definitions_not_playback_state() {
    let project = Project::new(
        "func init() self:play(self.name=='Player' and 'fast' or 'walk') end\nfunc draw() self:spr() end",
    );
    project.nodes("sprite='Player'", "[[node_state.nodes]]\nid='N2'\nname='Other'\nx=8\ny=0\nenabled=true\nscript='player.b8'\nsprite='Player'");
    let mut session = project.start();
    for tick in 1..=8 {
        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[0], COLORS[tick % 4]);
        assert_eq!(frame.pixels()[8], COLORS[(tick * 8 / 30) % 4]);
    }
}

#[test]
fn cli_sprite_summary_uses_core_effective_preview_rules() {
    let project = Project::new("");
    let output = Command::new(env!("CARGO_BIN_EXE_bit8"))
        .args(["inspect", "tilesheets"])
        .arg(&project.0)
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let sprites = value["sprites"].as_array().unwrap();
    let animations = &sprites
        .iter()
        .find(|item| item["name"] == "Player")
        .unwrap()["animations"];
    assert_eq!(
        animations[0],
        serde_json::json!({"name":"walk","frames":["A1","A2","A3","A4"],"fps":8,"loop":true})
    );
    assert_eq!(animations[2]["loop"], false);
    assert_eq!(
        sprites
            .iter()
            .find(|item| item["name"] == "Player")
            .unwrap()["preview"],
        "A4"
    );
    assert_eq!(
        sprites
            .iter()
            .find(|item| item["name"] == "Fallback")
            .unwrap()["preview"],
        "A3"
    );
    assert!(sprites.iter().find(|item| item["name"] == "Empty").unwrap()["preview"].is_null());
}

#[test]
fn init_draws_frame_zero_without_consuming_a_tick_then_eight_fps_progresses() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('walk') self:spr() end\nfunc draw() self:spr() end",
    );
    let mut session = project.start();
    assert_eq!(session.framebuffer().pixels()[0], COLORS[0]);
    for tick in 1..=120 {
        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[0], COLORS[(tick * 8 / 30) % 4]);
    }
}

#[test]
fn repeated_sprite_and_play_in_update_do_not_restart_playback() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('walk') end\nfunc update() self:sprite('Player') self:play('walk') end\nfunc draw() self:spr() end",
    );
    let mut session = project.start();
    for tick in 1..=30 {
        assert_eq!(
            session.step(0).unwrap().pixels()[0],
            COLORS[(tick * 8 / 30) % 4]
        );
    }
}

#[test]
fn update_selection_presents_frame_zero_then_thirty_fps_advances_after_update() {
    let project = Project::new(
        "ticks=0\nfunc init() self:sprite('Player') self:play('walk') end\nfunc update() ticks=ticks+1 if ticks==1 then self:play('fast') end end\nfunc draw() self:spr() end",
    );
    let mut session = project.start();
    for index in 0..8 {
        assert_eq!(session.step(0).unwrap().pixels()[0], COLORS[index % 4]);
    }
}

#[test]
fn update_observes_previous_frame_and_draw_observes_advanced_frame() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('fast') end\nfunc update() self.x=16 self:spr() self.x=0 end\nfunc draw() self:spr() end",
    );
    project.write("main.b8", "func draw() end");
    let mut session = project.start();
    for tick in 1..=8 {
        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[16], COLORS[(tick - 1) % 4]);
        assert_eq!(frame.pixels()[0], COLORS[tick % 4]);
    }
}

#[test]
fn thirty_fps_init_selection_advances_on_every_normal_tick() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('fast') end\nfunc draw() self:spr() end",
    );
    let mut session = project.start();
    for tick in 1..=8 {
        assert_eq!(session.step(0).unwrap().pixels()[0], COLORS[tick % 4]);
    }
}

#[test]
fn elapsed_time_drives_eight_fifteen_and_thirty_fps_at_all_frontend_cadences() {
    use bit8::runtime_timing::FIXED_DT;
    use std::time::Duration;
    for fps in [8, 15, 30] {
        for cadence in [30_u64, 60, 120] {
            let project = Project::new(
                "func init() self:sprite('Player') self:play('timed') end\nfunc update() self:sprite('Player') self:play('timed') end\nfunc draw() self:spr() end",
            );
            project.write("bit8.sprites.toml",&format!("{DEFINITIONS}\n[sprite.Player.animation.timed]\nframes=['A1','A2','A3','A4']\nfps={fps}\n"));
            let mut session = project.start();
            let mut total = Duration::ZERO;
            for frame in 1..=cadence * 10 {
                let elapsed = Duration::from_nanos(
                    frame * 1_000_000_000 / cadence - (frame - 1) * 1_000_000_000 / cadence,
                );
                total += elapsed;
                let ticks = total.as_nanos() / FIXED_DT.as_nanos();
                let expected = COLORS[((ticks * fps / 30) % 4) as usize];
                assert_eq!(session.advance(elapsed).unwrap().pixels()[0], expected);
                for _ in 0..3 {
                    assert_eq!(session.draw().unwrap().pixels()[0], expected);
                }
            }
        }
    }
}

#[test]
fn elapsed_catchup_preserves_independent_nodes_and_finished_nonloop_state() {
    use bit8::runtime_timing::FIXED_DT;
    let project = Project::new(
        "func init() self:sprite('Player') if self.name=='Player' then self:play('attack') else self:play('walk') end end\nfunc update() if self.name=='Player' then self:play('attack') else self:play('walk') end end\nfunc draw() self:spr() end",
    );
    project.nodes(
        "",
        "[[node_state.nodes]]\nid='N2'\nname='Other'\nx=8\ny=0\nenabled=true\nscript='player.b8'",
    );
    let mut session = project.start();
    for batch in 1..=6 {
        let frame = session.advance(FIXED_DT * 5).unwrap();
        assert_eq!(frame.pixels()[0], COLORS[1]);
        assert_eq!(frame.pixels()[8], COLORS[((batch * 5 * 8) / 30) % 4]);
    }
}

#[test]
fn drawing_multiple_times_never_advances_and_uses_current_node_coordinates() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('walk') end\nfunc draw() for i=0,3 do self.x=i*8 self:spr() end self.x=0 end",
    );
    let mut session = project.start();
    for tick in 1..=20 {
        let frame = session.step(0).unwrap();
        for x in [0, 8, 16, 24] {
            assert_eq!(frame.pixels()[x], COLORS[(tick * 8 / 30) % 4]);
        }
    }
}

#[test]
fn preview_and_empty_sprite_and_no_sprite_are_safe() {
    for (selection, color) in [
        ("self:sprite('Preview')", COLORS[3]),
        ("self:sprite('Fallback')", COLORS[2]),
        ("self:sprite('Empty')", 0),
        ("", 0),
    ] {
        let project = Project::new(&format!(
            "func init() {selection} end\nfunc draw() self:spr() end"
        ));
        assert_eq!(project.start().step(0).unwrap().pixels()[0], color);
    }
}

#[test]
fn unknown_sprite_animation_and_missing_sprite_errors_keep_node_script_context() {
    for (selection, message) in [
        ("self:sprite('Missing')", "Unknown sprite \"Missing\""),
        ("self:play('walk')", "Node \"Player\" has no sprite"),
        (
            "self:sprite('Player') self:play('fly')",
            "Sprite \"Player\" has no animation \"fly\"",
        ),
    ] {
        let project = Project::new(&format!("func update() {selection} end"));
        let mut session = project.start();
        let error = session.step(0).err().unwrap().to_string();
        assert!(error.contains(message), "{error}");
        assert!(error.contains("player.b8"), "{error}");
        assert!(error.contains("Player"), "{error}");
    }
}

#[test]
fn invalid_definitions_and_asset_references_fail_before_game_init() {
    for source in [
        "invalid sprite data",
        "version=1\n[sprite.Player]\npreview='A99'",
        "version=1\n[sprite.Player.animation.walk]\nframes=['Z1']\nfps=8",
    ] {
        let project = Project::new("func init() error('should not execute init') end");
        project.write("bit8.sprites.toml", source);
        let error = RuntimeSession::start(&project.0).err().unwrap().to_string();
        assert!(error.contains("bit8.sprites.toml"), "{error}");
        assert!(!error.contains("should not execute init"));
    }
}

#[test]
fn independent_nodes_advance_even_offscreen_and_disabled_nodes_do_not_tick() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('fast') end\nfunc update() if self.name=='Player' then self.x=100 else self.x=8 end end\nfunc draw() self:spr() end",
    );
    project.nodes(
        "",
        "[[node_state.nodes]]\nid='N2'\nname='Other'\nx=8\ny=0\nenabled=false\nscript='player.b8'",
    );
    project.write("main.b8", "t=0\nfunc update() t=t+1 if t==3 then node('Other').enabled=true end if t==4 then node('Player').x=0 end end\nfunc draw() cls(0) end");
    // Reveal the offscreen Player during draw; visibility must not freeze it.
    project.write("player.b8", "func init() self:sprite('Player') self:play('fast') end\nfunc update() if self.name=='Player' then self.x=100 end end\nfunc draw() if self.name=='Player' then self.x=0 end self:spr() end");
    let mut session = project.start();
    for tick in 1..=6 {
        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[0], COLORS[tick % 4]);
        assert_eq!(
            frame.pixels()[8],
            if tick < 3 { 0 } else { COLORS[(tick - 2) % 4] }
        );
    }
}

#[test]
fn nonloop_final_hold_and_same_animation_idempotency_and_away_back_restart() {
    let project = Project::new(
        "t=0\nfunc init() self:sprite('Player') self:play('attack') end\nfunc update() t=t+1 if t==5 then self:play('walk') elseif t==6 then self:play('attack') else self:play('attack') end end\nfunc draw() self:spr() end",
    );
    let mut session = project.start();
    for expected in [1, 1, 1, 1, 0, 0, 1, 1] {
        assert_eq!(session.step(0).unwrap().pixels()[0], COLORS[expected]);
    }
}

#[test]
fn binding_a_different_sprite_resets_to_preview_not_automatic_idle() {
    let project = Project::new(
        "t=0\nfunc init() self:sprite('Player') self:play('fast') end\nfunc update() t=t+1 if t==2 then self:sprite('Fallback') elseif t==3 then self:sprite('Player') end end\nfunc draw() self:spr() end",
    );
    let mut session = project.start();
    for expected in [1, 2, 3, 3] {
        assert_eq!(session.step(0).unwrap().pixels()[0], COLORS[expected]);
    }
}

#[test]
fn node_drawing_reuses_camera_transparency_clipping_and_direct_numeric_tile_renderer() {
    let project = Project::new(
        "func init() self:sprite('Preview') self.x=30 self.y=40 end\nfunc draw() self:spr() spr(A4,38,40) sprite(A4,46,40) spr(3,54,40) end",
    );
    project.nodes(
        "",
        "[[node_state.nodes]]\nid='N2'\ntype='Camera'\nname='Camera'\nx=64\ny=64\nenabled=true",
    );
    project.write("main.b8", "func draw() cls(10) end");
    let mut image = image::open(project.0.join("tilesheet.png"))
        .unwrap()
        .to_rgba8();
    image.put_pixel(26, 0, image::Rgba([255, 0, 255, 0]));
    image.save(project.0.join("tilesheet.png")).unwrap();
    let sheets = load_registered_tilesheets(&project.0).unwrap();
    let sprite = sheets["A"].cell(3).unwrap();
    let mut expected = Framebuffer::new();
    expected.clear(0xffec27);
    for x in [-2, 6, 14, 22] {
        expected.draw_sprite(sprite, x, 8);
    }
    let mut session = project.start();
    assert_eq!(session.step(0).unwrap().pixels(), expected.pixels());
    assert_eq!(expected.pixels()[8 * 64], 0xffec27);
}

#[test]
fn animation_sits_above_existing_move_collide_and_raw_position_rules_without_persistence() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('walk') end\nfunc update() assert(self:collide(1,0)) assert(self.x==0) self:move(10,0) assert(self.x==0) self:move(0,8) assert(self.y==8) self.x=8 assert(self.x==8) self:play('walk') end\nfunc draw() self:spr() end",
    );
    project.nodes(
        "[node_state.nodes.collider]\nenabled=true\noffset_x=0\noffset_y=0\nwidth=8\nheight=8",
        "",
    );
    let map = fs::read_to_string(project.0.join("world.b8map"))
        .unwrap()
        .replacen("-- -- -- -- -- -- -- --", "-- A4 -- -- -- -- -- --", 1);
    project.write("world.b8map", &map);
    assert_eq!(
        project.start().step(0).unwrap().pixels()[8 * 64 + 8],
        COLORS[0]
    );
    assert_eq!(
        fs::read_to_string(project.0.join("world.b8map")).unwrap(),
        map
    );
}

#[test]
fn actual_host_executes_node_animation_and_movement_using_the_same_session() {
    let project = Project::new(
        "func init() self:sprite('Player') self:play('walk') end\nfunc update() if btn(RIGHT) then self:move(1,0) end self:sprite('Player') self:play('walk') end\nfunc draw() self:spr() end",
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_bit8"))
        .arg("host")
        .arg(&project.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let commands = format!(
        "{{\"type\":\"step\",\"buttons\":[\"RIGHT\"],\"elapsed_ns\":{}}}\n",
        bit8::runtime_timing::FIXED_DT.as_nanos()
    )
    .repeat(4)
        + "{\"type\":\"stop\"}\n";
    child
        .stdin
        .take()
        .unwrap()
        .write_all(commands.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty());
    let messages: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(messages[0]["type"], "ready");
    for tick in 1..=4 {
        assert_eq!(messages[tick]["pixels"][tick], COLORS[tick * 8 / 30]);
    }
    assert_eq!(messages[5]["type"], "exit");
}
