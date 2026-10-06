use crate::framebuffer::Framebuffer;
use crate::lua::LuaRuntime;
use crate::project_assets;
use crate::runtime_map::RuntimeMap;
use crate::runtime_timing::{FIXED_DT, FixedClock};
use crate::tilesheet::Tilesheet;
use std::cell::{Ref, RefCell};
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// Owns a loaded Bit8 project and executes its game lifecycle synchronously.
/// Frontends control frame scheduling and presentation.
pub struct RuntimeSession {
    framebuffer: Rc<RefCell<Framebuffer>>,
    game: LuaRuntime,
    clock: FixedClock,
}

impl RuntimeSession {
    /// Loads a project, its tilesheets, and entry source, then runs `init()`.
    pub fn start(project_path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let (project_dir, entry_file) = resolve_project(project_path.as_ref())?;
        let legacy_tilesheet_path = ["tilesheet.png", "tilesheet (A).png"]
            .into_iter()
            .map(|file| project_dir.join(file))
            .find(|path| path.is_file())
            .unwrap_or_else(|| project_dir.join("tilesheet.png"));

        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let legacy_tilesheet = Rc::new(if legacy_tilesheet_path.is_file() {
            Tilesheet::load_png(legacy_tilesheet_path)?
        } else {
            Tilesheet::empty()
        });
        let registered_tilesheets = project_assets::load_registered_tilesheets(&project_dir)?;
        let registered_tilesheets = Rc::new(registered_tilesheets);
        let sprites = crate::sprite_definitions::SpriteRegistry::load_project(&project_dir)?
            .unwrap_or_default();
        let runtime_map = Rc::new(RefCell::new(RuntimeMap::load_default_with_sprites(
            &project_dir,
            Rc::clone(&registered_tilesheets),
            &sprites,
        )?));
        let mut game = LuaRuntime::new_with_runtime_map(
            Rc::clone(&framebuffer),
            legacy_tilesheet,
            registered_tilesheets,
            Some(Rc::clone(&runtime_map)),
        )?;
        game.set_sprite_registry(sprites);
        game.load_file_deferred(entry_file)?;
        game.load_node_scripts(
            &project_dir,
            runtime_map.borrow().nodes_in_stable_id_order(),
        )?;
        game.initialize()?;

        Ok(Self {
            framebuffer,
            game,
            clock: FixedClock::default(),
        })
    }

    /// Deterministic headless convenience: supply exactly one fixed interval.
    /// Real frontends must use `set_buttons` and `advance` with measured elapsed.
    pub fn step(&mut self, button_state: u8) -> Result<Ref<'_, Framebuffer>, Box<dyn Error>> {
        self.set_buttons(button_state);
        self.advance(FIXED_DT)
    }

    /// Sample the complete held mask; press edges latch until the next tick.
    pub fn set_buttons(&mut self, button_state: u8) {
        self.game.set_button_mask(button_state);
    }

    /// Execute up to five fixed updates, then draw once, even with zero ticks.
    /// Whole excess tick debt is discarded; a sub-tick remainder is retained.
    pub fn advance(&mut self, elapsed: Duration) -> Result<Ref<'_, Framebuffer>, Box<dyn Error>> {
        for _ in 0..self.clock.advance(elapsed) {
            self.game.update_tick()?;
        }
        self.draw()
    }

    /// Presentation only: no updates, animation advancement or edge consumption.
    pub fn draw(&self) -> Result<Ref<'_, Framebuffer>, Box<dyn Error>> {
        self.game.draw()?;
        Ok(self.framebuffer.borrow())
    }

    /// Reads the current framebuffer without advancing the game.
    pub fn framebuffer(&self) -> Ref<'_, Framebuffer> {
        self.framebuffer.borrow()
    }
}

fn resolve_project(requested_path: &Path) -> Result<(PathBuf, PathBuf), IoError> {
    let project_dir = requested_path.canonicalize().map_err(|error| {
        IoError::new(
            error.kind(),
            format!(
                "cannot resolve project directory '{}': {error}",
                requested_path.display()
            ),
        )
    })?;
    if !project_dir.is_dir() {
        return Err(IoError::new(
            ErrorKind::NotFound,
            format!("project directory not found: {}", project_dir.display()),
        ));
    }

    let entry_file = source_entry_file(&project_dir).ok_or_else(|| {
        IoError::new(
            ErrorKind::NotFound,
            format!(
                "project is missing main.b8 or main.lua: {}",
                project_dir.display()
            ),
        )
    })?;

    Ok((project_dir, entry_file))
}

fn source_entry_file(project_dir: &Path) -> Option<PathBuf> {
    let main_b8 = project_dir.join("main.b8");
    if main_b8.is_file() {
        return Some(main_b8);
    }

    let main_lua = project_dir.join("main.lua");
    main_lua.is_file().then_some(main_lua)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::{HEIGHT, WIDTH};
    use crate::input::Button;
    use crate::palette::PALETTE;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

    fn temporary_project(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "bit8-session-{name}-{}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        directory
    }

    fn write_node_map(project: &Path, next_id: u64, records: &str) {
        fs::write(
            project.join("world.b8map"),
            format!(
                "version = 1\nwidth = 8\nheight = 8\n\n{}\n[node_state]\nnext_id = {next_id}\n\n{records}",
                (0..8).map(|_| "-- -- -- -- -- -- -- --").collect::<Vec<_>>().join("\n")
            ),
        )
        .unwrap();
    }

    fn write_camera_render_project(project: &Path) {
        let mut sheet = image::RgbaImage::new(16, 8);
        for (x, color_index) in [(0..8, 4_u8), (8..16, 2_u8)] {
            let color = PALETTE[color_index as usize];
            let pixel = image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]);
            for y in 0..8 {
                for x in x.clone() {
                    sheet.put_pixel(x, y, pixel);
                }
            }
        }
        image::DynamicImage::ImageRgba8(sheet)
            .save(project.join("tilesheet (A).png"))
            .unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
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
                "version = 1\nwidth = 40\nheight = 40\n\n{rows}\n\n[node_state]\nnext_id = 3\n\n\
                 [[node_state.nodes]]\nid = \"N1\"\ntype = \"Camera\"\nname = \"View\"\nx = 248\ny = 264\nenabled = true\nscript = \"camera.b8\"\n\n\
                 [[node_state.nodes]]\nid = \"N2\"\nname = \"Player\"\nx = 248\ny = 264\nenabled = true\nscript = \"player.b8\"\n"
            ),
        )
        .unwrap();
        fs::write(
            project.join("camera.b8"),
            "func init()\n    self.init_count = (self.init_count or 0) + 1\nend\nfunc update()\n    assert(self.init_count == 1)\n    self.x = self.x + 1\nend\n",
        )
        .unwrap();
        fs::write(
            project.join("player.b8"),
            "func draw()\n    spr(A2, self.x, self.y)\nend\n",
        )
        .unwrap();
        fs::write(
            project.join("main.b8"),
            "func draw()\n    cls(10)\n    map()\n    spr(A2, 240, 264)\n    pix(0, 0, 2)\n    rectfill(1, 0, 1, 1, 3)\nend\n",
        )
        .unwrap();
    }

    fn write_solid_collision_project(project: &Path) {
        fs::write(project.join("main.b8"), "func draw()\n    cls(0)\nend\n").unwrap();
        let color = PALETTE[1];
        let pixel = image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]);
        let mut sheet = image::RgbaImage::new(16, 8);
        for pixel_slot in sheet.pixels_mut() {
            *pixel_slot = pixel;
        }
        image::DynamicImage::ImageRgba8(sheet)
            .save(project.join("solid (A).png"))
            .unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"solid (A).png\"\nsolid = [2]\n",
        )
        .unwrap();
        fs::write(
            project.join("world.b8map"),
            concat!(
                "version = 1\nwidth = 2\nheight = 1\n\n-- A2\n\n[node_state]\nnext_id = 4\n\n",
                "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 0\ny = 0\nenabled = true\nscript = \"player.b8\"\n\n",
                "[node_state.nodes.collider]\nenabled = true\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n\n",
                "[[node_state.nodes]]\nid = \"N2\"\nname = \"Disabled Collider\"\nx = 0\ny = 0\nenabled = true\nscript = \"player.b8\"\n\n",
                "[node_state.nodes.collider]\nenabled = false\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n\n",
                "[[node_state.nodes]]\nid = \"N3\"\nname = \"No Collider\"\nx = 0\ny = 0\nenabled = true\nscript = \"player.b8\"\n"
            ),
        )
        .unwrap();
        fs::write(
            project.join("player.b8"),
            concat!(
                "func update()\n",
                "    assert(self.collide ~= nil)\n",
                "    if self.name == 'Player' then\n",
                "        assert(self:collide(0, 0) == false) -- touching edge only\n",
                "        assert(self:collide(8, 0) == true) -- prospective overlap with A1\n",
                "        assert(self:collide(-8, 0) == false) -- outside map\n",
                "        assert(self.x == 0 and self.y == 0) -- query is non-mutating\n",
                "    else\n",
                "        assert(self:collide(8, 0) == false)\n",
                "    end\n",
                "end\n",
                "func draw()\n    pix(0, 0, 1)\nend\n"
            ),
        )
        .unwrap();
    }

    fn write_movement_project(project: &Path) {
        fs::write(
            project.join("main.b8"),
            "func draw()\n    assert(node('Player').x == 20)\nend\n",
        )
        .unwrap();
        let color = PALETTE[1];
        let pixel = image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]);
        let mut sheet = image::RgbaImage::new(24, 8);
        for pixel_slot in sheet.pixels_mut() {
            *pixel_slot = pixel;
        }
        image::DynamicImage::ImageRgba8(sheet)
            .save(project.join("solid (A).png"))
            .unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
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
                "    assert(self.x == 0) -- collide is non-mutating\n",
                "    self:move(10, 0)\n",
                "    assert(self.x == 8 and self.y == 0)\n",
                "    self.x = 20 -- direct assignment bypasses collision\n",
                "end\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn start_runs_init_and_step_runs_update_then_draw_and_exposes_pixels() {
        let project = temporary_project("lifecycle");
        fs::write(
            project.join("main.b8"),
            "x = 10\ny = 10\nfunc init()\n    pix(0, 0, 5)\nend\nfunc update()\n    if btn(RIGHT) then x = x + 1 end\n    if btnp(A) then y = y + 1 end\nend\nfunc draw()\n    cls(0)\n    pix(x, y, 2)\nend\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        assert_eq!(session.framebuffer().pixels()[0], PALETTE[5]);

        let frame = session
            .step(Button::Right.mask() | Button::A.mask())
            .unwrap();
        assert_eq!(frame.pixels()[10 * WIDTH + 10], PALETTE[0]);
        assert_eq!(frame.pixels()[11 * WIDTH + 11], PALETTE[2]);
        assert_eq!(frame.pixels().len(), WIDTH * HEIGHT);
        drop(frame);

        let frame = session
            .step(Button::Right.mask() | Button::A.mask())
            .unwrap();
        assert_eq!(frame.pixels()[11 * WIDTH + 12], PALETTE[2]);
        drop(frame);

        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[11 * WIDTH + 12], PALETTE[2]);
        drop(frame);

        let frame = session.step(Button::A.mask()).unwrap();
        assert_eq!(frame.pixels()[12 * WIDTH + 12], PALETTE[2]);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_scripts_have_independent_self_state_and_stable_lifecycle_order() {
        let project = temporary_project("node-lifecycle-order");
        let original_map = "version = 1\nwidth = 8\nheight = 8\n\n-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n\n[node_state]\nnext_id = 4\n\n".to_owned()
            + "[[node_state.nodes]]\nid = \"N3\"\nname = \"Three\"\nx = 24\ny = 0\nenabled = true\nscript = \"shared.b8\"\n\n"
            + "[[node_state.nodes]]\nid = \"N2\"\nname = \"Two\"\nx = 16\ny = 0\nenabled = false\nscript = \"shared.b8\"\n\n"
            + "[[node_state.nodes]]\nid = \"N1\"\nname = \"One\"\nx = 8\ny = 0\nenabled = true\nscript = \"shared.b8\"\n";
        fs::write(project.join("world.b8map"), &original_map).unwrap();
        fs::write(
            project.join("main.b8"),
            "events = {}\nframe_number = 0\n\
             func init()\n\
                 table.insert(events, 'main:init')\n\
                 assert(node('One').id == 'N1' and node('One').name == 'One')\n\
                 node('One').x = 8\n\
             end\n\
             func update()\n\
                 frame_number = frame_number + 1\n\
                 table.insert(events, 'main:update')\n\
                 if frame_number == 2 then node('Two').enabled = true end\n\
                 node('One').x = node('One').x + 1\n\
             end\n\
             func draw()\n\
                 table.insert(events, 'main:draw')\n\
                 if frame_number == 1 then\n\
                     assert(table.concat(events, ',') == 'main:init,N1:init,N2:init,N3:init,main:update,N1:update,N3:update,main:draw', table.concat(events, ','))\n\
                 elseif frame_number == 2 then\n\
                     assert(table.concat(events, ',') == 'main:init,N1:init,N2:init,N3:init,main:update,N1:update,N3:update,main:draw,N1:draw,N3:draw,main:update,N1:update,N2:update,N3:update,main:draw')\n\
                 end\n\
                 if node('One').x == 12 and node('One').y == 5 then pix(0, 0, 1) end\n\
             end\n",
        )
        .unwrap();
        fs::write(
            project.join("shared.b8"),
            "func init()\n\
                 self.hp = self.id == 'N1' and 0 or (self.id == 'N2' and 10 or 20)\n\
                 table.insert(events, self.id .. ':init')\n\
                 assert(node(self.name).x == self.x)\n\
             end\n\
             func update()\n\
                 self.hp = self.hp + 1\n\
                 table.insert(events, self.id .. ':update')\n\
                 if self.id == 'N1' then\n\
                     self.x = self.x + 1\n\
                     self.y = 5\n\
                 end\n\
             end\n\
             func draw()\n\
                 table.insert(events, self.id .. ':draw')\n\
                 pix(self.x, self.hp, 1)\n\
                 if self.id == 'N3' and frame_number == 2 then\n\
                     assert(table.concat(events, ',') == 'main:init,N1:init,N2:init,N3:init,main:update,N1:update,N3:update,main:draw,N1:draw,N3:draw,main:update,N1:update,N2:update,N3:update,main:draw,N1:draw,N2:draw,N3:draw', table.concat(events, ','))\n\
                 end\n\
             end\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        let first = session.step(0).unwrap();
        assert_eq!(first.pixels()[WIDTH + 10], PALETTE[1]);
        assert_eq!(first.pixels()[21 * WIDTH + 24], PALETTE[1]);
        assert_eq!(first.pixels()[0], PALETTE[0]);
        drop(first);

        let second = session.step(0).unwrap();
        assert_eq!(second.pixels()[2 * WIDTH + 12], PALETTE[1]);
        assert_eq!(second.pixels()[11 * WIDTH + 16], PALETTE[1]);
        assert_eq!(second.pixels()[22 * WIDTH + 24], PALETTE[1]);
        drop(second);

        let third = session.step(0).unwrap();
        assert_eq!(third.pixels()[3 * WIDTH + 14], PALETTE[1]);
        assert_eq!(third.pixels()[12 * WIDTH + 16], PALETTE[1]);
        assert_eq!(third.pixels()[23 * WIDTH + 24], PALETTE[1]);
        assert_eq!(
            fs::read_to_string(project.join("world.b8map")).unwrap(),
            original_map
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn camera_node_is_read_only_and_framebuffer_pixels_remain_screen_space() {
        let project = temporary_project("camera-node-type-identity");
        write_node_map(
            &project,
            2,
            "[[node_state.nodes]]\nid = \"N1\"\ntype = \"Camera\"\nname = \"View\"\nx = 8\ny = 16\nenabled = true\nscript = \"camera.b8\"\n",
        );
        fs::write(
            project.join("main.b8"),
            "func init()\n    assert(node('View').type == 'Camera')\nend\nfunc draw()\n    pix(node('View').x, node('View').y, 2)\nend\n",
        )
        .unwrap();
        fs::write(
            project.join("camera.b8"),
            "func init()\n    assert(self.type == 'Camera')\n    self.x = 16\nend\nfunc update()\n    self.x = self.x + 8\nend\nfunc draw()\n    assert(node(self.name).type == self.type)\n    pix(self.x, self.y, 1)\nend\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        let first = session.step(0).unwrap();
        assert_eq!(first.pixels()[16 * WIDTH + 24], PALETTE[1]);
        assert_eq!(first.pixels()[16 * WIDTH + 8], PALETTE[0]);
        drop(first);
        let second = session.step(0).unwrap();
        assert_eq!(second.pixels()[16 * WIDTH + 32], PALETTE[1]);
        drop(second);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn runtime_session_camera_transforms_map_and_sprites_after_current_frame_updates() {
        let project = temporary_project("camera-render-session");
        write_camera_render_project(&project);

        let mut session = RuntimeSession::start(&project).unwrap();
        let first = session.step(0).unwrap();
        assert_eq!(first.pixels()[32 * WIDTH + 31], PALETTE[2]);
        assert_eq!(first.pixels()[32 * WIDTH + 23], PALETTE[2]);
        assert_eq!(first.pixels()[0], PALETTE[2], "pix remains screen-space");
        assert_eq!(
            first.pixels()[1],
            PALETTE[3],
            "rectfill remains screen-space"
        );
        assert_eq!(first.pixels()[6], PALETTE[2]);
        assert_eq!(first.pixels()[7], PALETTE[4]);
        drop(first);

        let second = session.step(0).unwrap();
        assert_eq!(second.pixels()[32 * WIDTH + 30], PALETTE[2]);
        assert_eq!(second.pixels()[32 * WIDTH + 22], PALETTE[2]);
        assert_eq!(second.pixels()[6], PALETTE[4]);
        assert_eq!(second.pixels()[7], PALETTE[4]);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn inspected_row_major_cells_match_runtime_spr_and_map_pixels() {
        let project = temporary_project("tilesheet-slicing-agreement");
        let mut image = image::RgbaImage::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let source_cell = (y / 8) * 4 + x / 8;
                let palette_index = 1 + ((source_cell * 3 + (x % 8) + (y % 8) * 2) % 15) as usize;
                let pixel = if source_cell == 3 && x == 24 && y == 0 {
                    image::Rgba([0, 0, 0, 0])
                } else if source_cell == 3 && x == 25 && y == 0 {
                    image::Rgba([70, 130, 215, 255])
                } else {
                    let color = PALETTE[palette_index];
                    image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255])
                };
                image.put_pixel(x, y, pixel);
            }
        }
        image::DynamicImage::ImageRgba8(image)
            .save(project.join("tilesheet (A).png"))
            .unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"tilesheet (A).png\"\n",
        )
        .unwrap();

        let inspection = project_assets::inspect_tilesheets(&project).unwrap();
        let cells = &inspection.tilesheets[0].cells;
        assert_eq!(
            (
                inspection.tilesheets[0].width,
                inspection.tilesheets[0].height
            ),
            (32, 32)
        );
        assert_eq!(cells.len(), 16);
        assert_eq!((cells[3].id.as_str(), cells[3].x, cells[3].y), ("A4", 3, 0));
        assert_eq!(
            (cells[15].id.as_str(), cells[15].x, cells[15].y),
            ("A16", 3, 3)
        );
        assert_eq!(inspection.tilesheets[0].palette, PALETTE);
        assert_eq!(
            cells[3].pixels[0], 0,
            "the editor inspection preserves transparent index 0"
        );
        assert_eq!(
            cells[3].pixels[1], 12,
            "off-palette sky-blue uses the Runtime's cyan-blue entry"
        );

        fs::write(
            project.join("world.b8map"),
            "version = 1\nwidth = 2\nheight = 1\nA4 A16\n",
        )
        .unwrap();
        fs::write(
            project.join("main.b8"),
            "func draw()\n    cls(0)\n    map()\n    spr(A4, 16, 0)\n    spr(A16, 24, 0)\nend\n",
        )
        .unwrap();
        let mut session = RuntimeSession::start(&project).unwrap();
        let frame = session.step(0).unwrap();
        assert_eq!(
            frame.pixels()[0],
            PALETTE[0],
            "transparent map/sprite pixels preserve the clear color"
        );
        assert_eq!(
            frame.pixels()[1],
            PALETTE[12],
            "off-palette source colors render with the Runtime palette"
        );
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(
                    frame.pixels()[y * WIDTH + x],
                    frame.pixels()[y * WIDTH + 16 + x]
                );
                assert_eq!(
                    frame.pixels()[y * WIDTH + 8 + x],
                    frame.pixels()[y * WIDTH + 24 + x]
                );
            }
        }
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn self_enabled_x_and_y_share_node_proxy_state_and_reenable_without_reinitializing() {
        let project = temporary_project("node-self-state");
        write_node_map(
            &project,
            2,
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 8\ny = 0\nenabled = true\nscript = \"player.b8\"\n",
        );
        fs::write(
            project.join("main.b8"),
            "frame_number = 0\n\
             func update()\n\
                 frame_number = frame_number + 1\n\
                 if frame_number == 1 then node('Player').x = 16 end\n\
                 if frame_number == 2 then node('Player').enabled = true end\n\
             end\n\
             func draw()\n\
                 if frame_number == 1 then assert(node('Player').x == 24 and node('Player').enabled == false) end\n\
                 if frame_number == 2 then assert(node('Player').x == 32 and node('Player').y == 5 and node('Player').enabled == true) end\n\
             end\n",
        )
        .unwrap();
        fs::write(
            project.join("player.b8"),
            "func init()\n\
                 self.hp = 0\n\
                 self.y = 5\n\
                 assert(self.id == 'N1' and self.name == 'Player')\n\
             end\n\
             func update()\n\
                 self.hp = self.hp + 1\n\
                 self.x = self.x + 8\n\
                 if self.hp == 1 then self.enabled = false end\n\
             end\n\
             func draw()\n\
                 pix(self.x, self.hp, 1)\n\
             end\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        let first = session.step(0).unwrap();
        assert_eq!(
            first.pixels()[0],
            PALETTE[0],
            "disabled after update skips draw"
        );
        drop(first);
        let second = session.step(0).unwrap();
        assert_eq!(
            second.pixels()[2 * WIDTH + 32],
            PALETTE[1],
            "custom state survives disable and init is not repeated"
        );
        drop(second);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn self_identity_fields_are_read_only_and_script_callbacks_are_optional() {
        for field in ["id", "name", "type"] {
            let project = temporary_project("node-readonly-self");
            write_node_map(
                &project,
                2,
                "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 0\ny = 0\nenabled = true\nscript = \"readonly.b8\"\n",
            );
            fs::write(project.join("main.b8"), "func draw()\nend\n").unwrap();
            fs::write(
                project.join("readonly.b8"),
                format!("func init()\n    self.{field} = 'changed'\nend\n"),
            )
            .unwrap();
            let error = match RuntimeSession::start(&project) {
                Ok(_) => panic!("self.{field} must be read-only"),
                Err(error) => error.to_string(),
            };
            assert!(error.contains("node=N1 (Player)"), "{error}");
            assert!(error.contains("callback=init"), "{error}");
            assert!(
                error.contains(&format!("self.{field} is read-only")),
                "{error}"
            );
            fs::remove_dir_all(project).unwrap();
        }

        let project = temporary_project("node-empty-script");
        write_node_map(
            &project,
            3,
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Empty\"\nx = 0\ny = 0\nenabled = true\nscript = \"empty.b8\"\n\n[[node_state.nodes]]\nid = \"N2\"\nname = \"NoScript\"\nx = 8\ny = 0\nenabled = true\n",
        );
        fs::write(project.join("main.b8"), "func draw()\nend\n").unwrap();
        fs::write(project.join("empty.b8"), "-- no callbacks\n").unwrap();
        let mut session = RuntimeSession::start(&project).unwrap();
        assert_eq!(session.step(0).unwrap().pixels().len(), WIDTH * HEIGHT);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_script_errors_include_identity_path_callback_and_source_location() {
        let project = temporary_project("node-script-error");
        write_node_map(
            &project,
            2,
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Broken\"\nx = 0\ny = 0\nenabled = true\nscript = \"scripts/broken.b8\"\n",
        );
        fs::create_dir(project.join("scripts")).unwrap();
        fs::write(project.join("main.b8"), "func draw()\nend\n").unwrap();
        fs::write(
            project.join("scripts/broken.b8"),
            "func update()\n    error('node exploded')\nend\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        let error = match session.step(0) {
            Ok(_) => panic!("node update should return its error"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("node=N1 (Broken)"), "{error}");
        assert!(error.contains("script=scripts/broken.b8"), "{error}");
        assert!(error.contains("callback=update"), "{error}");
        assert!(error.contains("broken.b8:2"), "{error}");
        assert!(error.contains("node exploded"), "{error}");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_script_syntax_errors_include_node_and_script_load_context() {
        let project = temporary_project("node-script-syntax-error");
        write_node_map(
            &project,
            2,
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Malformed\"\nx = 0\ny = 0\nenabled = true\nscript = \"broken.b8\"\n",
        );
        fs::write(project.join("main.b8"), "func draw()\nend\n").unwrap();
        fs::write(project.join("broken.b8"), "func update(\n").unwrap();

        let error = match RuntimeSession::start(&project) {
            Ok(_) => panic!("malformed Node script should fail startup"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("node=N1 (Malformed)"), "{error}");
        assert!(error.contains("script=broken.b8"), "{error}");
        assert!(error.contains("callback=load"), "{error}");
        assert!(error.contains("broken.b8:1"), "{error}");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn missing_or_escaping_node_scripts_fail_with_node_context() {
        let project = temporary_project("node-script-missing");
        fs::write(project.join("main.b8"), "func draw()\nend\n").unwrap();
        write_node_map(
            &project,
            2,
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Absent\"\nx = 0\ny = 0\nenabled = true\nscript = \"missing.b8\"\n",
        );
        let error = match RuntimeSession::start(&project) {
            Ok(_) => panic!("missing node script should fail startup"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("node=N1 (Absent)"), "{error}");
        assert!(error.contains("script=missing.b8"), "{error}");
        assert!(error.contains("cannot resolve script"), "{error}");

        let outside_name = format!(
            "{}-outside.b8",
            project.file_name().unwrap().to_string_lossy()
        );
        let outside_path = project.parent().unwrap().join(&outside_name);
        write_node_map(
            &project,
            2,
            &format!(
                "[[node_state.nodes]]\nid = \"N1\"\nname = \"Escape\"\nx = 0\ny = 0\nenabled = true\nscript = \"../{outside_name}\"\n"
            ),
        );
        fs::write(&outside_path, "").unwrap();
        let error = match RuntimeSession::start(&project) {
            Ok(_) => panic!("escaping node script should fail startup"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("node=N1 (Escape)"), "{error}");
        assert!(
            error.contains(&format!("script=../{outside_name}")),
            "{error}"
        );
        assert!(error.contains("escapes the Bit8 project root"), "{error}");
        fs::remove_file(outside_path).unwrap();
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn registered_a4_reference_runs_through_a_headless_session() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo");
        let mut session = RuntimeSession::start(&project).unwrap();
        let frame = session.step(0).unwrap();

        assert!(
            (32..40).any(|y| { (32..40).any(|x| frame.pixels()[y * WIDTH + x] != PALETTE[0]) })
        );
    }

    #[test]
    fn solid_tiles_and_node_colliders_do_not_intercept_scripted_movement() {
        let project = temporary_project("collision-data-does-not-change-movement");
        let mut image = image::RgbaImage::new(8, 8);
        for pixel in image.pixels_mut() {
            *pixel = image::Rgba([29, 43, 83, 255]);
        }
        image::DynamicImage::ImageRgba8(image)
            .save(project.join("solid (A).png"))
            .unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"solid (A).png\"\nsolid = [1]\n",
        )
        .unwrap();
        fs::write(
            project.join("world.b8map"),
            concat!(
                "version = 1\nwidth = 8\nheight = 8\n\n",
                "-- A1 -- -- -- -- -- --\n",
                "-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n",
                "-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n",
                "-- -- -- -- -- -- -- --\n-- -- -- -- -- -- -- --\n",
                "-- -- -- -- -- -- -- --\n\n[node_state]\nnext_id = 2\n\n",
                "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 0\ny = 0\nenabled = true\nscript = \"player.b8\"\nvisual = \"A99\"\n\n",
                "[node_state.nodes.collider]\nenabled = true\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n"
            ),
        )
        .unwrap();
        fs::write(
            project.join("main.b8"),
            "func draw()\n    cls(0)\n    assert(node('Player').x == 8)\n    pix(8, 0, 1)\nend\n",
        )
        .unwrap();
        fs::write(
            project.join("player.b8"),
            "func update()\n    self.x = self.x + 8\nend\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        let frame = session.step(0).unwrap();
        assert_eq!(
            frame.pixels()[8],
            PALETTE[1],
            "an unresolved editor-only visual does not alter runtime loading or scripted movement into a Solid tile"
        );
        assert!(
            project.join("world.b8map").is_file(),
            "runtime mutation does not rewrite map data"
        );
        drop(frame);
        drop(session);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_self_collide_is_callable_and_queries_prospective_solid_overlap() {
        let project = temporary_project("node-self-collide");
        write_solid_collision_project(&project);

        let mut session = RuntimeSession::start(&project).unwrap();
        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[0], PALETTE[1]);

        drop(frame);
        drop(session);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_self_move_executes_in_script_and_raw_position_assignment_stays_unconstrained() {
        let project = temporary_project("node-self-move");
        write_movement_project(&project);

        let mut session = RuntimeSession::start(&project).unwrap();
        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[0], PALETTE[0]);

        drop(frame);
        drop(session);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn lua_map_apis_mutate_runtime_state_and_camera_translates_world_sprites() {
        let project = temporary_project("map-api");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo/tilesheet (A).png"),
            project.join("sheet.png"),
        )
        .unwrap();
        let color = PALETTE[5];
        let mut second_sheet = image::RgbaImage::new(8, 8);
        for pixel in second_sheet.pixels_mut() {
            *pixel = image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]);
        }
        image::DynamicImage::ImageRgba8(second_sheet)
            .save(project.join("second.png"))
            .unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n\n[[tilesheets]]\ngroup = \"B\"\nfile = \"second.png\"\n",
        )
        .unwrap();
        fs::write(
            project.join("world.b8map"),
            "version = 1\nwidth = 2\nheight = 1\nA1 --\n\n[node_state]\nnext_id = 2\n\n[[node_state.nodes]]\nid = \"N1\"\ntype = \"Camera\"\nname = \"Camera\"\nx = 248\ny = 264\nenabled = true\n",
        )
        .unwrap();
        fs::write(
            project.join("main.b8"),
            "func update()\n    if btn(A) then\n        mset(1, 0, B1)\n        mset(0, 0, nil)\n    end\nend\nfunc draw()\n    cls(0)\n    assert(node('Camera').type == 'Camera')\n    map()\n    spr(A1, 248, 264)\n    if mget(0, 0) == 'A1' then pix(60, 0, 2) end\n    if mget(1, 0) == nil then pix(61, 0, 3) end\n    if mget(0, 0) == nil and mget(1, 0) == 'B1' then pix(62, 0, 4) end\nend\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        let frame = session.step(0).unwrap();
        assert_eq!(frame.pixels()[60], PALETTE[2]);
        assert_eq!(frame.pixels()[61], PALETTE[3]);
        assert!(
            (32..40).any(|y| { (32..40).any(|x| frame.pixels()[y * WIDTH + x] != PALETTE[0]) })
        );
        assert!((0..8).all(|y| { (0..16).all(|x| frame.pixels()[y * WIDTH + x] == PALETTE[0]) }));
        drop(frame);

        let frame = session.step(Button::A.mask()).unwrap();
        assert_eq!(frame.pixels()[0], PALETTE[0]);
        assert_eq!(frame.pixels()[60], PALETTE[0]);
        assert_eq!(frame.pixels()[61], PALETTE[0]);
        assert_eq!(frame.pixels()[62], PALETTE[4]);
        assert!((0..8).all(|y| { (0..16).all(|x| frame.pixels()[y * WIDTH + x] == PALETTE[0]) }));
        assert!(
            fs::read_to_string(project.join("world.b8map"))
                .unwrap()
                .contains("A1 --")
        );
        drop(frame);
        drop(session);

        let mut restarted = RuntimeSession::start(&project).unwrap();
        let frame = restarted.step(0).unwrap();
        assert_eq!(frame.pixels()[60], PALETTE[2]);
        assert_eq!(frame.pixels()[61], PALETTE[3]);
        assert!((0..8).all(|y| { (0..8).all(|x| frame.pixels()[y * WIDTH + x] == PALETTE[0]) }));
        drop(frame);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn map_api_on_a_project_without_default_map_reports_source_context() {
        let project = temporary_project("missing-map");
        fs::write(project.join("main.b8"), "func draw()\n    map()\nend\n").unwrap();
        let mut session = RuntimeSession::start(&project).unwrap();
        let error = match session.step(0) {
            Ok(_) => panic!("map() without world.b8map should fail"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("main.b8"));
        assert!(error.contains("world.b8map"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn mset_rejects_unknown_or_out_of_range_tiles_with_source_context() {
        let project = temporary_project("map-invalid-tile");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo/tilesheet (A).png"),
            project.join("sheet.png"),
        )
        .unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        )
        .unwrap();
        fs::write(
            project.join("world.b8map"),
            "version = 1\nwidth = 1\nheight = 1\nA1\n",
        )
        .unwrap();
        fs::write(
            project.join("main.b8"),
            "func draw()\n    mset(0, 0, A99)\nend\n",
        )
        .unwrap();

        let mut session = RuntimeSession::start(&project).unwrap();
        let error = match session.step(0) {
            Ok(_) => panic!("out-of-range mset tile should fail"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("main.b8"), "unexpected error: {error}");
        assert!(error.contains("A99"), "unexpected error: {error}");
        assert!(error.contains("out of range"), "unexpected error: {error}");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn source_and_runtime_errors_keep_b8_context() {
        let syntax_project = temporary_project("syntax-error");
        fs::write(syntax_project.join("main.b8"), "func draw(\n").unwrap();
        let error = RuntimeSession::start(&syntax_project).err().unwrap();
        assert!(error.to_string().contains("main.b8"));
        fs::remove_dir_all(syntax_project).unwrap();

        let runtime_project = temporary_project("runtime-error");
        fs::write(
            runtime_project.join("main.b8"),
            "func draw()\n    spr(A99, 0, 0)\nend\n",
        )
        .unwrap();
        let mut session = RuntimeSession::start(&runtime_project).unwrap();
        let error = session.step(0).err().unwrap().to_string();
        assert!(
            error.contains("main.b8"),
            "unexpected runtime error: {error}"
        );
        assert!(error.contains("A99"), "unexpected runtime error: {error}");
        fs::remove_dir_all(runtime_project).unwrap();
    }

    #[test]
    fn dropping_a_session_does_not_require_a_native_window() {
        let project = temporary_project("drop");
        fs::write(project.join("main.b8"), "func draw() end\n").unwrap();

        let session = RuntimeSession::start(&project).unwrap();
        drop(session);

        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn main_b8_is_preferred_and_legacy_main_lua_still_loads() {
        let project = temporary_project("entry-selection");
        fs::write(project.join("main.lua"), "func draw() end\n").unwrap();
        fs::write(project.join("main.b8"), "func draw() end\n").unwrap();
        assert!(RuntimeSession::start(&project).is_ok());
        fs::remove_file(project.join("main.b8")).unwrap();
        assert!(RuntimeSession::start(&project).is_ok());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn missing_project_and_missing_entry_have_clear_errors() {
        let missing = std::env::temp_dir().join(format!("bit8-no-project-{}", std::process::id()));
        assert!(
            RuntimeSession::start(missing)
                .err()
                .unwrap()
                .to_string()
                .contains("cannot resolve project directory")
        );

        let project = temporary_project("no-entry");
        let error = RuntimeSession::start(&project).err().unwrap().to_string();
        assert!(error.contains("main.b8 or main.lua"));
        fs::remove_dir_all(project).unwrap();
    }
}
