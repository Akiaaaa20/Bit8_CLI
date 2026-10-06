use crate::framebuffer::{Framebuffer, SPRITE_SIZE, WIDTH};
use crate::map::{Bit8Map, BoxCollider, MapCell};
use crate::node_type::NodeType;
use crate::project_assets::{self, AssetId};
use crate::tilesheet::Tilesheet;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io::Error as IoError;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const DEFAULT_MAP_FILE: &str = "world.b8map";

struct MapData {
    width: usize,
    height: usize,
    cells: Vec<MapCell>,
}

/// Mutable game-time map state. The project file is loaded once and never
/// modified; each RuntimeSession receives its own copy.
pub struct RuntimeMap {
    path: PathBuf,
    data: Option<MapData>,
    nodes: Vec<Rc<RefCell<RuntimeNode>>>,
    tilesheets: Rc<HashMap<String, Tilesheet>>,
    solid_cells: HashSet<AssetId>,
}

/// Camera translation from world pixels to the fixed framebuffer viewport.
/// `left` and `top` are inclusive world-space coordinates of screen pixel (0, 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraTransform {
    left: i128,
    top: i128,
}

impl CameraTransform {
    fn centered_at(x: i64, y: i64) -> Self {
        Self {
            left: x as i128 - crate::node_type::CAMERA_HALF_WIDTH as i128,
            top: y as i128 - crate::node_type::CAMERA_HALF_HEIGHT as i128,
        }
    }

    pub fn origin(self) -> (i128, i128) {
        (self.left, self.top)
    }

    pub fn world_to_screen(self, x: i64, y: i64) -> (i64, i64) {
        (
            (x as i128 - self.left).clamp(i64::MIN as i128, i64::MAX as i128) as i64,
            (y as i128 - self.top).clamp(i64::MIN as i128, i64::MAX as i128) as i64,
        )
    }
}

/// Mutable runtime copy of a node record. It is deliberately independent of
/// the persisted `Bit8Map` and never writes back to its source file.
#[derive(Debug)]
pub struct RuntimeNode {
    id: String,
    node_type: NodeType,
    name: String,
    x: i64,
    y: i64,
    enabled: bool,
    script: Option<String>,
    initial_sprite: Option<String>,
    collider: Option<BoxCollider>,
}

impl RuntimeNode {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn node_type(&self) -> NodeType {
        self.node_type
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn x(&self) -> i64 {
        self.x
    }
    pub fn y(&self) -> i64 {
        self.y
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn script(&self) -> Option<&str> {
        self.script.as_deref()
    }
    pub fn initial_sprite(&self) -> Option<&str> {
        self.initial_sprite.as_deref()
    }
    pub fn set_x(&mut self, x: i64) {
        self.x = x;
    }
    pub fn set_y(&mut self, y: i64) {
        self.y = y;
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn collider(&self) -> Option<&BoxCollider> {
        self.collider.as_ref()
    }
}

impl RuntimeMap {
    pub fn load_default(
        project: &Path,
        tilesheets: Rc<HashMap<String, Tilesheet>>,
    ) -> Result<Self, Box<dyn Error>> {
        let sprites =
            crate::sprite_definitions::SpriteRegistry::load_project(project)?.unwrap_or_default();
        Self::load_default_with_sprites(project, tilesheets, &sprites)
    }

    pub(crate) fn load_default_with_sprites(
        project: &Path,
        tilesheets: Rc<HashMap<String, Tilesheet>>,
        sprites: &crate::sprite_definitions::SpriteRegistry,
    ) -> Result<Self, Box<dyn Error>> {
        let solid_cells = project_assets::load_registered_solid_cells(project)?;
        let path = project.join(DEFAULT_MAP_FILE);
        let data = if path.try_exists()? {
            let source = fs::read_to_string(&path).map_err(|error| {
                IoError::new(
                    error.kind(),
                    format!("cannot read map file '{}': {error}", path.display()),
                )
            })?;
            let map = Bit8Map::parse(&source, &path, &tilesheets)?;
            map.validate_sprites(sprites, &path)?;
            let mut nodes: Vec<Rc<RefCell<RuntimeNode>>> = map
                .nodes()
                .iter()
                .map(|node| {
                    Rc::new(RefCell::new(RuntimeNode {
                        id: node.id.clone(),
                        node_type: node.node_type,
                        name: node.name.clone(),
                        x: node.x,
                        y: node.y,
                        enabled: node.enabled,
                        script: node.script.clone(),
                        initial_sprite: node.sprite.clone(),
                        collider: node.collider.clone(),
                    }))
                })
                .collect();
            nodes.sort_by(|left, right| {
                let left_id = left
                    .borrow()
                    .id()
                    .trim_start_matches('N')
                    .parse::<u64>()
                    .unwrap_or(0);
                let right_id = right
                    .borrow()
                    .id()
                    .trim_start_matches('N')
                    .parse::<u64>()
                    .unwrap_or(0);
                left_id.cmp(&right_id)
            });
            let data = Some(MapData {
                width: map.width(),
                height: map.height(),
                cells: map.cells().to_vec(),
            });
            return Ok(Self {
                path,
                data,
                nodes,
                tilesheets,
                solid_cells,
            });
        } else {
            None
        };

        Ok(Self {
            path,
            data,
            nodes: Vec::new(),
            tilesheets,
            solid_cells,
        })
    }

    pub fn node_by_name(&self, name: &str) -> Option<Rc<RefCell<RuntimeNode>>> {
        self.nodes
            .iter()
            .find(|node| node.borrow().name() == name)
            .cloned()
    }

    /// Returns the current map's runtime nodes in stable numeric ID order.
    pub fn nodes_in_stable_id_order(&self) -> Vec<Rc<RefCell<RuntimeNode>>> {
        self.nodes.clone()
    }

    /// Tests a prospective Node position against Solid map tiles without
    /// changing either the Node or the map. Bounds and offsets are game pixels.
    pub fn node_collides(&self, node: &RuntimeNode, dx: i64, dy: i64) -> bool {
        let Some(collider) = node.collider().filter(|collider| collider.enabled) else {
            return false;
        };
        let Some(data) = &self.data else {
            return false;
        };

        let left = node.x() as i128 + collider.offset_x as i128 + dx as i128;
        let top = node.y() as i128 + collider.offset_y as i128 + dy as i128;
        let right = left + collider.width as i128;
        let bottom = top + collider.height as i128;
        let map_width = data.width as i128 * SPRITE_SIZE as i128;
        let map_height = data.height as i128 * SPRITE_SIZE as i128;
        if left < 0 || top < 0 || right > map_width || bottom > map_height {
            return false;
        }

        let tile_size = SPRITE_SIZE as i128;
        let first_x = (left / tile_size) as usize;
        let first_y = (top / tile_size) as usize;
        // Collider dimensions are validated as positive integers. Subtracting
        // one makes a right/bottom edge touching a tile boundary non-overlap.
        let last_x = ((right - 1) / tile_size) as usize;
        let last_y = ((bottom - 1) / tile_size) as usize;
        (first_y..=last_y).any(|y| {
            (first_x..=last_x).any(|x| {
                matches!(get_cell(data, x, y), Some(MapCell::Asset(id)) if self.solid_cells.contains(id))
            })
        })
    }

    /// Moves a Node in integer game-pixel steps, resolving X before Y through
    /// the same Solid-tile query used by `node_collides`. An already-overlapping
    /// collider is not depenetrated: its first still-overlapping step is blocked.
    pub fn move_node(&mut self, node: &mut RuntimeNode, dx: i64, dy: i64) -> Result<(), String> {
        let has_collider = node.collider().is_some_and(|collider| collider.enabled);
        if !has_collider {
            let x = node
                .x()
                .checked_add(dx)
                .ok_or_else(|| "Node movement exceeds the integer game-pixel range".to_owned())?;
            let y = node
                .y()
                .checked_add(dy)
                .ok_or_else(|| "Node movement exceeds the integer game-pixel range".to_owned())?;
            node.set_x(x);
            node.set_y(y);
            return Ok(());
        }

        self.sweep_node_axis(node, dx, true)?;
        self.sweep_node_axis(node, dy, false)
    }

    fn sweep_node_axis(
        &self,
        node: &mut RuntimeNode,
        delta: i64,
        horizontal: bool,
    ) -> Result<(), String> {
        let step = if delta < 0 { -1 } else { 1 };
        let mut remaining = delta.unsigned_abs();
        while remaining > 0 {
            let (dx, dy) = if horizontal { (step, 0) } else { (0, step) };
            if self.node_collides(node, dx, dy) {
                break;
            }
            if horizontal {
                node.set_x(node.x().checked_add(step).ok_or_else(|| {
                    "Node movement exceeds the integer game-pixel range".to_owned()
                })?);
            } else {
                node.set_y(node.y().checked_add(step).ok_or_else(|| {
                    "Node movement exceeds the integer game-pixel range".to_owned()
                })?);
            }
            remaining -= 1;
        }
        Ok(())
    }

    /// Resolves the enabled Camera with the smallest numeric stable Node ID
    /// from current runtime state. No enabled Camera is the legacy origin.
    pub fn camera_transform(&self) -> CameraTransform {
        let camera = self
            .nodes
            .iter()
            .filter_map(|node| {
                let node = node.borrow();
                (node.enabled() && node.node_type() == NodeType::Camera)
                    .then(|| (node_id_number(node.id()), node.x(), node.y()))
            })
            .min_by_key(|(id, _, _)| *id);

        match camera {
            Some((_, x, y)) => CameraTransform::centered_at(x, y),
            None => CameraTransform { left: 0, top: 0 },
        }
    }

    pub fn get(&self, x: i64, y: i64) -> Result<Option<String>, String> {
        let data = self.loaded_data()?;
        let cell = usize::try_from(x)
            .ok()
            .zip(usize::try_from(y).ok())
            .and_then(|(x, y)| get_cell(data, x, y));
        Ok(cell.and_then(|cell| match cell {
            MapCell::Empty => None,
            MapCell::Asset(id) => Some(asset_symbol(id)),
        }))
    }

    /// `None` clears a cell. Writes outside the declared map are errors.
    pub fn set(&mut self, x: i64, y: i64, symbol: Option<&str>) -> Result<(), String> {
        let (width, height) = {
            let data = self.loaded_data()?;
            (data.width, data.height)
        };
        let (x, y) = usize::try_from(x)
            .ok()
            .zip(usize::try_from(y).ok())
            .filter(|(x, y)| *x < width && *y < height)
            .ok_or_else(|| {
                format!(
                    "map write ({x}, {y}) is outside {width}x{height} map in '{}'",
                    self.path.display()
                )
            })?;
        let index = y * width + x;
        let value = if let Some(symbol) = symbol {
            let reference = project_assets::parse_asset_reference(symbol).ok_or_else(|| {
                format!("invalid map tile '{symbol}'; expected a registered asset ID or nil")
            })?;
            MapCell::Asset(
                project_assets::resolve_asset_reference(&reference, &self.tilesheets)
                    .map_err(|error| format!("{} in '{}'", error, self.path.display()))?
                    .id,
            )
        } else {
            MapCell::Empty
        };
        self.data.as_mut().expect("loaded data checked").cells[index] = value;
        Ok(())
    }

    pub fn render(&self, framebuffer: &mut Framebuffer) -> Result<(), String> {
        let data = self.loaded_data()?;
        let transform = self.camera_transform();
        let Some(columns) = visible_tile_range(transform.left, data.width) else {
            return Ok(());
        };
        let Some(rows) = visible_tile_range(transform.top, data.height) else {
            return Ok(());
        };
        for y in rows {
            for x in columns.clone() {
                let Some(MapCell::Asset(id)) = get_cell(data, x, y) else {
                    continue;
                };
                let sprite = project_assets::resolve_asset_id(id, &self.tilesheets)
                    .map_err(|error| format!("{} in '{}'", error, self.path.display()))?
                    .sprite;
                let screen_x = (x as i128 * SPRITE_SIZE as i128) - transform.left;
                let screen_y = (y as i128 * SPRITE_SIZE as i128) - transform.top;
                framebuffer.draw_sprite(sprite, screen_x as i64, screen_y as i64);
            }
        }
        Ok(())
    }

    fn loaded_data(&self) -> Result<&MapData, String> {
        self.data.as_ref().ok_or_else(|| {
            format!(
                "project has no default map '{}'; expected '{}'",
                DEFAULT_MAP_FILE,
                self.path.display()
            )
        })
    }
}

fn node_id_number(id: &str) -> u64 {
    id.strip_prefix('N')
        .and_then(|number| number.parse().ok())
        .unwrap_or(u64::MAX)
}

fn visible_tile_range(origin: i128, tile_count: usize) -> Option<RangeInclusive<usize>> {
    if tile_count == 0 {
        return None;
    }
    let tile_size = SPRITE_SIZE as i128;
    let first = origin.div_euclid(tile_size).max(0);
    let last = ((origin + WIDTH as i128 - 1).div_euclid(tile_size)).min(tile_count as i128 - 1);
    (first <= last).then_some(first as usize..=last as usize)
}

fn get_cell(data: &MapData, x: usize, y: usize) -> Option<&MapCell> {
    if x >= data.width || y >= data.height {
        return None;
    }
    data.cells.get(y.checked_mul(data.width)?.checked_add(x)?)
}

fn asset_symbol(id: &AssetId) -> String {
    format!("{}{}", id.group, id.cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framebuffer::HEIGHT;
    use crate::palette::PALETTE;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

    fn project() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "bit8-runtime-map-{}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo/tilesheet (A).png"),
            directory.join("sheet.png"),
        )
        .unwrap();
        let color = PALETTE[5];
        let pixel = image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]);
        let mut second_sheet = image::RgbaImage::new(8, 8);
        for pixel_slot in second_sheet.pixels_mut() {
            *pixel_slot = pixel;
        }
        image::DynamicImage::ImageRgba8(second_sheet)
            .save(directory.join("second.png"))
            .unwrap();
        fs::write(
            directory.join(project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n\n[[tilesheets]]\ngroup = \"B\"\nfile = \"second.png\"\n",
        )
        .unwrap();
        directory
    }

    fn sheets(project: &Path) -> Rc<HashMap<String, Tilesheet>> {
        Rc::new(project_assets::load_registered_tilesheets(project).unwrap())
    }

    fn collision_project() -> PathBuf {
        let project = project();
        project_assets::set_tile_solid(&project, "A1", true).unwrap();
        let solids = [(2, 0), (2, 1), (1, 2), (4, 4)];
        let rows = (0..8)
            .map(|y| {
                (0..8)
                    .map(|x| {
                        if solids.contains(&(x, y)) {
                            "A1"
                        } else if (x, y) == (5, 0) {
                            "A2"
                        } else {
                            "--"
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            project.join(DEFAULT_MAP_FILE),
            format!("version = 1\nwidth = 8\nheight = 8\n\n{rows}\n"),
        )
        .unwrap();
        project
    }

    fn movement_node(x: i64, y: i64, collider: Option<BoxCollider>) -> RuntimeNode {
        RuntimeNode {
            id: "N1".to_owned(),
            node_type: NodeType::Node,
            name: "Player".to_owned(),
            x,
            y,
            enabled: true,
            script: None,
            initial_sprite: None,
            collider,
        }
    }

    fn box_collider(width: i64, height: i64, offset_x: i64, offset_y: i64) -> BoxCollider {
        BoxCollider {
            enabled: true,
            offset_x,
            offset_y,
            width,
            height,
        }
    }

    #[test]
    fn camera_transform_uses_center_coordinates_and_a_64_pixel_extent() {
        assert_eq!(CameraTransform::centered_at(32, 32).origin(), (0, 0));
        let transform = CameraTransform::centered_at(248, 264);
        assert_eq!(transform.origin(), (216, 232));
        assert_eq!(transform.world_to_screen(248, 264), (32, 32));
        assert_eq!(CameraTransform::centered_at(16, 16).origin(), (-16, -16));
        let extreme = CameraTransform::centered_at(i64::MIN, i64::MIN);
        assert_eq!(extreme.world_to_screen(i64::MIN, i64::MIN), (32, 32));
        assert_eq!(visible_tile_range(0, 100), Some(0..=7));
        assert_eq!(visible_tile_range(1, 100), Some(0..=8));
        assert_eq!(visible_tile_range(-32, 100), Some(0..=3));
        assert_eq!(visible_tile_range(0, 0), None);
    }

    #[test]
    fn active_camera_is_smallest_numeric_enabled_id_and_uses_mutable_state() {
        let project = project();
        fs::write(
            project.join(DEFAULT_MAP_FILE),
            concat!(
                "version = 1\nwidth = 1\nheight = 1\n--\n\n[node_state]\nnext_id = 11\n\n",
                "[[node_state.nodes]]\nid = \"N10\"\ntype = \"Camera\"\nname = \"Alpha\"\nx = 300\ny = 400\nenabled = true\n\n",
                "[[node_state.nodes]]\nid = \"N2\"\ntype = \"Camera\"\nname = \"Zulu\"\nx = 248\ny = 264\nenabled = true\n\n",
                "[[node_state.nodes]]\nid = \"N1\"\nname = \"NotCamera\"\nx = 32\ny = 32\nenabled = true\n"
            ),
        )
        .unwrap();
        let map = RuntimeMap::load_default(&project, sheets(&project)).unwrap();
        assert_eq!(map.camera_transform().origin(), (216, 232));

        map.node_by_name("Zulu")
            .unwrap()
            .borrow_mut()
            .set_enabled(false);
        assert_eq!(map.camera_transform().origin(), (268, 368));
        let alpha = map.node_by_name("Alpha").unwrap();
        alpha.borrow_mut().set_x(32);
        alpha.borrow_mut().set_y(32);
        assert_eq!(map.camera_transform().origin(), (0, 0));
        alpha.borrow_mut().set_enabled(false);
        assert_eq!(map.camera_transform().origin(), (0, 0));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn map_render_applies_pixel_camera_offsets_and_preserves_outside_background() {
        let project = project();
        let row = std::iter::repeat_n("A1", 16).collect::<Vec<_>>().join(" ");
        fs::write(
            project.join(DEFAULT_MAP_FILE),
            format!("version = 1\nwidth = 16\nheight = 1\n{row}\n"),
        )
        .unwrap();
        let map = RuntimeMap::load_default(&project, sheets(&project)).unwrap();
        let camera = Rc::new(RefCell::new(RuntimeNode {
            id: "N1".to_owned(),
            node_type: NodeType::Camera,
            name: "Camera".to_owned(),
            x: 33,
            y: 4,
            enabled: true,
            script: None,
            initial_sprite: None,
            collider: None,
        }));
        let mut map = map;
        map.nodes.push(camera);
        let transform = map.camera_transform();
        assert_eq!(transform.origin(), (1, -28));

        let mut rendered = Framebuffer::new();
        rendered.clear(PALETTE[7]);
        map.render(&mut rendered).unwrap();
        let mut expected = Framebuffer::new();
        expected.clear(PALETTE[7]);
        let registered_sheets = sheets(&project);
        let sheet = project_assets::resolve_asset_reference(
            &project_assets::parse_asset_reference("A1").unwrap(),
            &registered_sheets,
        )
        .unwrap();
        for tile_x in 0..16 {
            expected.draw_sprite(sheet.sprite, tile_x * 8 - 1, 28);
        }
        assert_eq!(rendered.pixels(), expected.pixels());

        map.nodes[0].borrow_mut().set_x(10_000);
        map.nodes[0].borrow_mut().set_y(10_000);
        let mut outside = Framebuffer::new();
        outside.clear(PALETTE[6]);
        map.render(&mut outside).unwrap();
        assert!(outside.pixels().iter().all(|pixel| *pixel == PALETTE[6]));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn mutable_cells_read_clear_and_validate_registered_symbols() {
        let project = project();
        fs::write(
            project.join(DEFAULT_MAP_FILE),
            "version = 1\nwidth = 2\nheight = 1\nA1 A4\n",
        )
        .unwrap();
        let mut map = RuntimeMap::load_default(&project, sheets(&project)).unwrap();

        assert_eq!(map.get(0, 0).unwrap().as_deref(), Some("A1"));
        assert_eq!(map.get(1, 0).unwrap().as_deref(), Some("A4"));
        map.set(1, 0, Some("B1")).unwrap();
        assert_eq!(map.get(1, 0).unwrap().as_deref(), Some("B1"));
        let mut second_group_frame = Framebuffer::new();
        map.render(&mut second_group_frame).unwrap();
        assert!(
            (0..8).any(|y| {
                (0..8).any(|x| second_group_frame.pixels()[y * WIDTH + x] != PALETTE[0])
            })
        );
        assert_eq!(second_group_frame.pixels()[8], PALETTE[5]);
        assert_eq!(map.get(2, 0).unwrap(), None);
        assert_eq!(map.get(-1, 0).unwrap(), None);
        map.set(1, 0, None).unwrap();
        assert_eq!(map.get(1, 0).unwrap(), None);
        let mut preserved = Framebuffer::new();
        preserved.clear(PALETTE[7]);
        map.render(&mut preserved).unwrap();
        assert!(
            (0..8).all(|y| { (8..16).all(|x| preserved.pixels()[y * WIDTH + x] == PALETTE[7]) })
        );
        assert!(map.set(0, 0, Some("A99")).unwrap_err().contains("A99"));
        assert!(map.set(2, 0, Some("A1")).unwrap_err().contains("outside"));

        let reloaded = RuntimeMap::load_default(&project, sheets(&project)).unwrap();
        assert_eq!(reloaded.get(1, 0).unwrap().as_deref(), Some("A4"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_movement_sweeps_pixels_x_then_y_and_reuses_solid_collision_query() {
        let project = collision_project();
        let mut map = RuntimeMap::load_default(&project, sheets(&project)).unwrap();

        let mut node = movement_node(0, 8, Some(box_collider(8, 8, 0, 0)));
        assert!(map.node_collides(&node, 20, 0));
        assert_eq!((node.x(), node.y()), (0, 8), "collide remains non-mutating");
        map.move_node(&mut node, 20, 0).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (8, 8),
            "large movement stops before the first overlap"
        );

        node.set_x(0);
        node.set_y(8);
        map.move_node(&mut node, 40, 0).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (8, 8),
            "movement beyond one tile cannot tunnel"
        );

        node.set_x(40);
        node.set_y(8);
        map.move_node(&mut node, -32, 0).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (24, 8),
            "negative X stops immediately before Solid"
        );

        node.set_x(8);
        node.set_y(0);
        map.move_node(&mut node, 0, 20).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (8, 8),
            "positive Y stops before Solid"
        );

        node.set_x(8);
        node.set_y(40);
        map.move_node(&mut node, 0, -32).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (8, 24),
            "negative Y stops before Solid"
        );

        node = movement_node(40, 0, Some(box_collider(8, 8, 0, 0)));
        map.move_node(&mut node, 3, 2).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (43, 2),
            "unobstructed movement resolves the full request"
        );

        node.set_x(0);
        node.set_y(8);
        map.move_node(&mut node, 20, -8).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (8, 0),
            "blocked X still resolves free Y afterward"
        );

        node.set_x(0);
        node.set_y(8);
        map.move_node(&mut node, 8, 20).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (8, 8),
            "Y is tested after the successful X movement"
        );

        node = movement_node(0, 8, Some(box_collider(8, 8, 8, 0)));
        map.move_node(&mut node, 1, 0).unwrap();
        assert_eq!(
            node.x(),
            0,
            "collider offset is included in the prospective bounds"
        );

        node = movement_node(0, 8, Some(box_collider(16, 8, 0, 0)));
        map.move_node(&mut node, 1, 0).unwrap();
        assert_eq!(
            node.x(),
            0,
            "collider width is used, not a fixed tile-sized box"
        );

        node = movement_node(8, 0, Some(box_collider(8, 16, 0, 0)));
        map.move_node(&mut node, 0, 1).unwrap();
        assert_eq!(
            node.y(),
            0,
            "collider height is used, not a fixed tile-sized box"
        );

        node = movement_node(0, 8, Some(box_collider(8, 8, 0, 0)));
        map.move_node(&mut node, 8, 0).unwrap();
        assert_eq!(node.x(), 8, "edge touching remains passable");

        node = movement_node(64, 0, Some(box_collider(8, 8, 0, 0)));
        map.move_node(&mut node, 3, 0).unwrap();
        assert_eq!(node.x(), 67, "outside-map space is non-Solid");
        node = movement_node(-20, -24, Some(box_collider(8, 8, 0, 0)));
        map.move_node(&mut node, -3, -5).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (-23, -29),
            "negative world positions remain supported"
        );

        node = movement_node(0, 0, None);
        map.move_node(&mut node, 100, -7).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (100, -7),
            "a Node without a collider moves freely"
        );
        let mut disabled = box_collider(8, 8, 0, 0);
        disabled.enabled = false;
        node = movement_node(0, 0, Some(disabled));
        map.move_node(&mut node, 100, -7).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (100, -7),
            "a disabled collider moves freely"
        );

        node = movement_node(32, 32, Some(box_collider(8, 8, 0, 0)));
        map.move_node(&mut node, 1, 0).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (32, 32),
            "an initially overlapping collider is not depenetrated"
        );

        let camera = Rc::new(RefCell::new(RuntimeNode {
            id: "N2".to_owned(),
            node_type: NodeType::Camera,
            name: "Camera".to_owned(),
            x: 0,
            y: 0,
            enabled: true,
            script: None,
            initial_sprite: None,
            collider: None,
        }));
        map.nodes.push(Rc::clone(&camera));
        camera.borrow_mut().set_x(500);
        camera.borrow_mut().set_y(-700);
        node = movement_node(0, 8, Some(box_collider(8, 8, 0, 0)));
        map.move_node(&mut node, 20, 0).unwrap();
        assert_eq!(
            (node.x(), node.y()),
            (8, 8),
            "Camera position does not alter world-space collision"
        );

        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn map_render_matches_spr_palette_output_and_clips_large_maps() {
        let project = project();
        let row = std::iter::repeat_n("A4", 9).collect::<Vec<_>>().join(" ");
        let source = format!(
            "version = 1\nwidth = 9\nheight = 9\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            row, row, row, row, row, row, row, row, row
        );
        fs::write(project.join(DEFAULT_MAP_FILE), source).unwrap();
        let map = RuntimeMap::load_default(&project, sheets(&project)).unwrap();
        let mut map_frame = Framebuffer::new();
        let mut sprite_frame = Framebuffer::new();
        map.render(&mut map_frame).unwrap();
        let registered = project_assets::load_registered_tilesheets(&project).unwrap();
        let sprite = project_assets::resolve_asset_reference(
            &project_assets::parse_asset_reference("A4").unwrap(),
            &registered,
        )
        .unwrap()
        .sprite;
        sprite_frame.draw_sprite(sprite, 0, 0);
        assert_eq!(
            &map_frame.pixels()[..SPRITE_SIZE],
            &sprite_frame.pixels()[..SPRITE_SIZE]
        );
        assert_eq!(map_frame.pixels().len(), WIDTH * HEIGHT);
        assert!(
            (56..64).any(|y| { (56..64).any(|x| map_frame.pixels()[y * WIDTH + x] != PALETTE[0]) })
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn projects_without_default_map_fail_map_api_cleanly_but_can_still_run() {
        let project = project();
        let nested_map = project.join("maps");
        fs::create_dir(&nested_map).unwrap();
        fs::write(
            nested_map.join(DEFAULT_MAP_FILE),
            "version = 1\nwidth = 1\nheight = 1\nA1\n",
        )
        .unwrap();
        let map = RuntimeMap::load_default(&project, sheets(&project)).unwrap();
        let error = map.get(0, 0).unwrap_err();
        assert!(error.contains("world.b8map"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_lookup_returns_mutable_runtime_copies_without_persisting_changes() {
        let project = project();
        let path = project.join(DEFAULT_MAP_FILE);
        let source = concat!(
            "version = 1\nwidth = 1\nheight = 1\n--\n\n",
            "[node_state]\nnext_id = 2\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 256\ny = 224\nenabled = true\nscript = \"player.b8\"\n"
        );
        fs::write(&path, source).unwrap();
        let map = RuntimeMap::load_default(&project, sheets(&project)).unwrap();
        assert!(map.node_by_name("Banana").is_none());

        let player = map.node_by_name("Player").unwrap();
        assert_eq!(player.borrow().id(), "N1");
        assert_eq!(player.borrow().name(), "Player");
        assert_eq!((player.borrow().x(), player.borrow().y()), (256, 224));
        assert!(player.borrow().enabled());
        player.borrow_mut().set_x(120);
        player.borrow_mut().set_y(80);
        player.borrow_mut().set_enabled(false);
        assert_eq!((player.borrow().x(), player.borrow().y()), (120, 80));
        assert!(!player.borrow().enabled());
        assert_eq!(map.node_by_name("Player").unwrap().borrow().x(), 120);
        assert_eq!(fs::read_to_string(path).unwrap(), source);
        fs::remove_dir_all(project).unwrap();
    }
}
