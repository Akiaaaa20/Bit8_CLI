use bit8::framebuffer::Framebuffer;
use bit8::map::Bit8Map;
use bit8::project_assets::load_registered_tilesheets;
use bit8::runtime_session::RuntimeSession;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

fn temporary_project(name: &str, source: &str) -> PathBuf {
    let project = std::env::temp_dir().join(format!(
        "bit8-host-{name}-{}-{}",
        std::process::id(),
        NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&project).unwrap();
    fs::write(project.join("main.b8"), source).unwrap();
    project
}

fn spawn_host(project: &Path, input: &str) -> Output {
    // Fixtures assert logical-tick results, not pipe arrival speed.
    let input = input
        .lines()
        .map(|line| match serde_json::from_str::<Value>(line) {
            Ok(mut command) if command["type"] == "step" => {
                if command.get("elapsed_ns").is_none() {
                    command["elapsed_ns"] =
                        serde_json::json!(bit8::runtime_timing::FIXED_DT.as_nanos() as u64);
                }
                command.to_string()
            }
            _ => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if input.is_empty() { "" } else { "\n" };
    let mut child = Command::new(env!("CARGO_BIN_EXE_bit8"))
        .arg("host")
        .arg(project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn messages(output: &Output) -> Vec<Value> {
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn frame_pixels(message: &Value) -> &[Value] {
    message["pixels"].as_array().unwrap()
}

#[test]
fn spawned_host_steps_input_edges_and_stops_without_an_extra_frame() {
    let project = temporary_project(
        "input-flow",
        "x = 30\npresses = 0\nfunc update()\n    if btn(RIGHT) then x = x + 1 end\n    if btnp(A) then presses = presses + 1 end\nend\nfunc draw()\n    cls(0)\n    pix(x, 32, 1)\n    pix(presses, 0, 2)\nend\n",
    );
    let output = spawn_host(
        &project,
        concat!(
            "{\"type\":\"step\",\"buttons\":[]}\n",
            "{\"type\":\"step\",\"buttons\":[\"RIGHT\"]}\n",
            "{\"type\":\"step\",\"buttons\":[\"RIGHT\",\"A\"]}\n",
            "{\"type\":\"step\",\"buttons\":[\"RIGHT\",\"A\"]}\n",
            "{\"type\":\"step\",\"buttons\":[]}\n",
            "{\"type\":\"step\",\"buttons\":[\"A\"]}\n",
            "{\"type\":\"stop\"}\n",
            "{\"type\":\"step\",\"buttons\":[\"RIGHT\"]}\n"
        ),
    );

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let messages = messages(&output);
    assert_eq!(
        messages[0],
        serde_json::json!({"type":"ready", "width":64, "height":64})
    );
    assert_eq!(
        messages.last().unwrap(),
        &serde_json::json!({"type":"exit"})
    );
    let frames = messages
        .iter()
        .filter(|message| message["type"] == "frame")
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 6);

    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(frame["seq"], index as u64 + 1);
        assert_eq!(frame["width"], 64);
        assert_eq!(frame["height"], 64);
        assert_eq!(frame_pixels(frame).len(), 4096);
    }
    assert_eq!(frame_pixels(frames[0])[32 * 64 + 30], 0x1D2B53);
    assert_eq!(frame_pixels(frames[1])[32 * 64 + 31], 0x1D2B53);
    assert_eq!(frame_pixels(frames[2])[32 * 64 + 32], 0x1D2B53);
    assert_eq!(frame_pixels(frames[2])[1], 0x7E2553);
    assert_eq!(frame_pixels(frames[3])[32 * 64 + 33], 0x1D2B53);
    assert_eq!(frame_pixels(frames[3])[1], 0x7E2553);
    assert_eq!(frame_pixels(frames[4])[1], 0x7E2553);
    assert_eq!(frame_pixels(frames[5])[2], 0x7E2553);
    assert_eq!(messages.len(), 8, "stop must prevent the trailing step");

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn invalid_buttons_malformed_json_and_unknown_types_are_recoverable() {
    let project = temporary_project("invalid-input", "func draw() cls(0) end\n");
    let output = spawn_host(
        &project,
        concat!(
            "{\"type\":\"step\",\"buttons\":[\"RIGTH\"]}\n",
            "{not json}\n",
            "{\"type\":\"dance\"}\n",
            "{\"type\":\"step\",\"buttons\":[]}\n",
            "{\"type\":\"stop\"}\n"
        ),
    );

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let messages = messages(&output);
    assert_eq!(messages.len(), 6);
    for error in &messages[1..4] {
        assert_eq!(error["type"], "error");
        assert!(
            error["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
    }
    assert!(messages[1]["message"].as_str().unwrap().contains("RIGTH"));
    assert_eq!(messages[4]["type"], "frame");
    assert_eq!(messages[4]["seq"], 1);
    assert_eq!(messages[5]["type"], "exit");

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn stdin_eof_shuts_down_cleanly_after_emitting_exit() {
    let project = temporary_project("eof", "func draw() end\n");
    let output = spawn_host(&project, "{\"type\":\"step\",\"buttons\":[]}\n");

    assert!(output.status.success());
    let messages = messages(&output);
    assert_eq!(messages[0]["type"], "ready");
    assert_eq!(messages[1]["type"], "frame");
    assert_eq!(messages[1]["seq"], 1);
    assert_eq!(messages[2]["type"], "exit");
    assert!(output.stderr.is_empty());

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn host_runs_the_registered_a4_demo_without_a_native_window() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo");
    let output = spawn_host(
        &project,
        "{\"type\":\"step\",\"buttons\":[]}\n{\"type\":\"stop\"}\n",
    );

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let messages = messages(&output);
    assert_eq!(messages[0]["type"], "ready");
    assert_eq!(messages[1]["type"], "frame");
    assert_eq!(messages[1]["seq"], 1);
    let pixels = frame_pixels(&messages[1]);
    // Player draws A4 at its world origin. The demo Camera follows Player
    // during update, so (32,48) - camera origin (0,16) = screen (32,32).
    // The earlier demo's fixed (16,16) patch is now legitimately empty.
    // Verify the actual registered A4 pattern, not just an arbitrary lit pixel.
    let sheets = load_registered_tilesheets(&project).unwrap();
    let a4 = sheets["A"].cell(3).unwrap(); // Public A4 is 1-based cell 4.
    assert!(
        a4.iter().any(|index| *index != 0),
        "A4 must not be an empty sprite"
    );
    let mut expected = Framebuffer::new();
    expected.draw_sprite(a4, 32, 32);
    assert_eq!(pixels.len(), 64 * 64);
    for y in 32..40 {
        for x in 32..40 {
            assert_eq!(
                pixels[y * 64 + x].as_u64(),
                Some(u64::from(expected.pixels()[y * 64 + x])),
                "registered A4 pixel at screen ({x},{y}) differs after Player-follow Camera update"
            );
        }
    }
    assert_eq!(messages[2]["type"], "exit");
}

#[test]
fn spawned_host_executes_node_script_lifecycle_through_runtime_session() {
    let project = temporary_project("node-lifecycle", "func draw()\n    cls(0)\nend\n");
    fs::write(
        project.join("world.b8map"),
        "version = 1\nwidth = 1\nheight = 1\n--\n\n[node_state]\nnext_id = 2\n\n[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 8\ny = 0\nenabled = true\nscript = \"player.b8\"\n",
    )
    .unwrap();
    fs::write(
        project.join("player.b8"),
        "func init()\n    self.hp = 2\nend\nfunc update()\n    self.hp = self.hp + 1\n    if btn(\"RIGHT\") then self.x = self.x + 8 end\nend\nfunc draw()\n    pix(self.x, self.hp, 12)\nend\n",
    )
    .unwrap();

    let output = spawn_host(
        &project,
        "{\"type\":\"step\",\"buttons\":[]}\n{\"type\":\"step\",\"buttons\":[\"RIGHT\"]}\n{\"type\":\"stop\"}\n",
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let messages = messages(&output);
    assert_eq!(messages[0]["type"], "ready");
    assert_eq!(messages[1]["type"], "frame");
    assert_eq!(frame_pixels(&messages[1])[3 * 64 + 8], 0x29ADFF);
    assert_eq!(messages[2]["type"], "frame");
    assert_eq!(frame_pixels(&messages[2])[4 * 64 + 16], 0x29ADFF);
    assert_eq!(messages[3]["type"], "exit");

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn spawned_host_exposes_node_self_collide_during_script_update() {
    let project = temporary_project("node-collide", "func draw() cls(0) end\n");
    let mut image = image::RgbaImage::new(16, 8);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([29, 43, 83, 255]);
    }
    image::DynamicImage::ImageRgba8(image)
        .save(project.join("solid (A).png"))
        .unwrap();
    fs::write(
        project.join("bit8.assets.toml"),
        "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"solid (A).png\"\nsolid = [2]\n",
    )
    .unwrap();
    fs::write(
        project.join("world.b8map"),
        concat!(
            "version = 1\nwidth = 2\nheight = 1\n\n-- A2\n\n[node_state]\nnext_id = 2\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 0\ny = 0\nenabled = true\nscript = \"player.b8\"\n\n",
            "[node_state.nodes.collider]\nenabled = true\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n"
        ),
    )
    .unwrap();
    fs::write(
        project.join("player.b8"),
        "func update()\n    assert(self.collide ~= nil)\n    assert(self:collide(8, 0) == true)\nend\n",
    )
    .unwrap();

    let output = spawn_host(
        &project,
        "{\"type\":\"step\",\"buttons\":[]}\n{\"type\":\"stop\"}\n",
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let messages = messages(&output);
    assert_eq!(messages[0]["type"], "ready");
    assert_eq!(
        messages[1]["type"], "frame",
        "self:collide executed in the Node update callback"
    );
    assert_eq!(messages[1]["seq"], 1);
    assert_eq!(messages[2]["type"], "exit");

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn spawned_host_executes_node_self_move_and_resolves_solid_collision() {
    let project = temporary_project("node-move", "func draw() cls(0) end\n");
    let mut image = image::RgbaImage::new(24, 8);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([29, 43, 83, 255]);
    }
    image::DynamicImage::ImageRgba8(image)
        .save(project.join("solid (A).png"))
        .unwrap();
    fs::write(
        project.join("bit8.assets.toml"),
        "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"solid (A).png\"\nsolid = [3]\n",
    )
    .unwrap();
    fs::write(
        project.join("world.b8map"),
        concat!(
            "version = 1\nwidth = 3\nheight = 1\n\n-- -- A3\n\n[node_state]\nnext_id = 2\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 0\ny = 0\nenabled = true\nscript = \"player.b8\"\n\n",
            "[node_state.nodes.collider]\nenabled = true\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n"
        ),
    )
    .unwrap();
    fs::write(
        project.join("player.b8"),
        concat!(
            "func update()\n",
            "    assert(self.move ~= nil)\n",
            "    assert(self:collide(10, 0) == true)\n",
            "    self:move(10, 0)\n",
            "    assert(self.x == 8 and self.y == 0)\n",
            "end\n",
            "func draw()\n    pix(self.x, self.y, 1)\nend\n"
        ),
    )
    .unwrap();

    let output = spawn_host(
        &project,
        "{\"type\":\"step\",\"buttons\":[]}\n{\"type\":\"stop\"}\n",
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let messages = messages(&output);
    assert_eq!(messages[0]["type"], "ready");
    assert_eq!(
        messages[1]["type"], "frame",
        "self:move executed in the Node update callback"
    );
    assert_eq!(messages[1]["seq"], 1);
    assert_eq!(frame_pixels(&messages[1])[8], 0x1D2B53);
    assert_eq!(messages[2]["type"], "exit");

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn runtime_and_host_load_a_64_by_64_map_with_trailing_node_state_and_run_its_script() {
    let project = temporary_project("large-map-node-state", "func draw()\n    cls(0)\nend\n");
    let rows = (0..64)
        .map(|_| std::iter::repeat_n("--", 64).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    let map_source = format!(
        "version = 1\nwidth = 64\nheight = 64\n\n{rows}\n\n[node_state]\nnext_id = 17\n\n[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 8\ny = 16\nenabled = true\nscript = \"player.b8\"\n"
    );
    let map_path = project.join("world.b8map");
    fs::write(&map_path, &map_source).unwrap();
    fs::write(
        project.join("player.b8"),
        "func init()\n    self.ticks = 0\nend\nfunc update()\n    self.ticks = self.ticks + 1\nend\nfunc draw()\n    pix(self.x + self.ticks - 1, self.y, 12)\nend\n",
    )
    .unwrap();

    let parsed = Bit8Map::load(&map_path, &project).unwrap();
    assert_eq!((parsed.width(), parsed.height()), (64, 64));
    assert_eq!(parsed.cells().len(), 4096);
    assert_eq!(parsed.nodes().len(), 1);
    let node = &parsed.nodes()[0];
    assert_eq!(node.id, "N1");
    assert_eq!(node.name, "Player");
    assert_eq!((node.x, node.y, node.enabled), (8, 16, true));
    assert_eq!(node.script.as_deref(), Some("player.b8"));
    assert!(parsed.serialize().unwrap().contains("next_id = 17"));

    let mut session = RuntimeSession::start(&project).unwrap();
    let frame = session.step(0).unwrap();
    assert_eq!(frame.pixels()[16 * 64 + 8], 0x29ADFF);
    drop(frame);

    let output = spawn_host(
        &project,
        "{\"type\":\"step\",\"buttons\":[]}\n{\"type\":\"stop\"}\n",
    );
    assert!(output.status.success());
    let host_messages = messages(&output);
    assert_eq!(host_messages[0]["type"], "ready");
    assert_eq!(host_messages[1]["type"], "frame");
    assert_eq!(host_messages[1]["width"], 64);
    assert_eq!(host_messages[1]["height"], 64);
    assert_eq!(frame_pixels(&host_messages[1])[16 * 64 + 8], 0x29ADFF);
    assert_eq!(host_messages[2]["type"], "exit");

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn host_framebuffer_uses_the_same_runtime_camera_rendering_as_runtime_session() {
    let project = temporary_project(
        "camera-render",
        "func draw()\n    cls(0)\n    map()\n    spr(A2, 248, 264)\n    pix(0, 0, 7)\nend\n",
    );
    let mut sheet = image::RgbaImage::new(16, 8);
    for (range, palette_index) in [(0..8, 4_u8), (8..16, 2_u8)] {
        let color = match palette_index {
            2 => 0x7E2553,
            4 => 0xAB5236,
            _ => unreachable!(),
        };
        let pixel = image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]);
        for y in 0..8 {
            for x in range.clone() {
                sheet.put_pixel(x, y, pixel);
            }
        }
    }
    image::DynamicImage::ImageRgba8(sheet)
        .save(project.join("tilesheet (A).png"))
        .unwrap();
    fs::write(
        project.join("bit8.assets.toml"),
        "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"tilesheet (A).png\"\n",
    )
    .unwrap();
    let rows = (0..40)
        .map(|_| {
            (0..40)
                .map(|x| if x % 2 == 0 { "A1" } else { "A2" })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        project.join("world.b8map"),
        format!(
            "version = 1\nwidth = 40\nheight = 40\n\n{rows}\n\n[node_state]\nnext_id = 2\n\n\
             [[node_state.nodes]]\nid = \"N1\"\ntype = \"Camera\"\nname = \"View\"\nx = 248\ny = 264\nenabled = true\nscript = \"camera.b8\"\n"
        ),
    )
    .unwrap();
    fs::write(
        project.join("camera.b8"),
        "func update()\n    self.x = self.x + 1\nend\n",
    )
    .unwrap();

    let mut session = RuntimeSession::start(&project).unwrap();
    let expected = session.step(0).unwrap().pixels().to_vec();
    let output = spawn_host(
        &project,
        "{\"type\":\"step\",\"buttons\":[]}\n{\"type\":\"stop\"}\n",
    );
    assert!(output.status.success());
    let messages = messages(&output);
    let frame = &messages[1];
    assert_eq!(frame["type"], "frame");
    let actual = frame_pixels(frame)
        .iter()
        .map(|pixel| pixel.as_u64().unwrap() as u32)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert_eq!(actual[32 * 64 + 31], 0x7E2553);
    assert_eq!(actual[0], 0xFFF1E8);

    fs::remove_dir_all(project).unwrap();
}

#[test]
fn runtime_error_is_reported_with_b8_source_context() {
    let project = temporary_project("runtime-error", "func draw()\n    spr(A99, 0, 0)\nend\n");
    let output = spawn_host(&project, "{\"type\":\"step\",\"buttons\":[]}\n");

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let messages = messages(&output);
    assert_eq!(messages[0]["type"], "ready");
    assert_eq!(messages[1]["type"], "error");
    let message = messages[1]["message"].as_str().unwrap();
    assert!(
        message.contains("main.b8"),
        "missing source context: {message}"
    );
    assert!(message.contains("A99"), "missing asset ID: {message}");

    fs::remove_dir_all(project).unwrap();
}
