use crate::node_type::NodeType;
use crate::project_assets::{self, AssetId, parse_asset_reference, resolve_asset_reference};
use crate::tilesheet::Tilesheet;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

pub const MAX_MAP_WIDTH: usize = 256;
pub const MAX_MAP_HEIGHT: usize = 256;
pub const MAX_MAP_CELLS: usize = MAX_MAP_WIDTH * MAX_MAP_HEIGHT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapCell {
    Empty,
    Asset(AssetId),
}

/// Persisted node identity and initial game-pixel state for a map. For a
/// Camera node, x/y identify the viewport center; other nodes retain their
/// existing position semantics.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MapNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    pub name: String,
    /// Game-pixel position; for Camera this is the viewport center on X.
    pub x: i64,
    /// Game-pixel position; for Camera this is the viewport center on Y.
    pub y: i64,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// Optional editor-only tilesheet cell ID. Runtime nodes deliberately do
    /// not consume this presentation metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual: Option<String>,
    /// Optional project SpriteDefinition name; never playback state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collider: Option<BoxCollider>,
}

/// Optional editor-authored box collider metadata in integer game pixels.
/// Runtime movement does not consult this data in Phase 1.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BoxCollider {
    pub enabled: bool,
    pub offset_x: i64,
    pub offset_y: i64,
    pub width: i64,
    pub height: i64,
}

/// A rectangular, row-major grid whose asset cells preserve registered group/cell identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bit8Map {
    width: usize,
    height: usize,
    cells: Vec<MapCell>,
    nodes: Vec<MapNode>,
    next_node_id: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSectionDocument {
    #[serde(default)]
    node_state: NodeSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSection {
    next_id: Option<u64>,
    #[serde(default)]
    nodes: Vec<PersistedMapNode>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedMapNode {
    id: String,
    #[serde(rename = "type")]
    node_type: Option<String>,
    name: String,
    x: i64,
    y: i64,
    enabled: bool,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    collider: Option<toml::Value>,
    #[serde(default)]
    visual: Option<toml::Value>,
    #[serde(default)]
    sprite: Option<String>,
}

#[derive(Serialize)]
struct NodeSectionForSerialization<'a> {
    node_state: NodeSectionForSerializationData<'a>,
}

#[derive(Serialize)]
struct NodeSectionForSerializationData<'a> {
    next_id: u64,
    nodes: &'a [MapNode],
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct MapInspectionReport {
    pub width: usize,
    pub height: usize,
    pub cells: usize,
    pub used_assets: Vec<String>,
}

#[derive(Debug)]
pub struct MapError(String);

impl Display for MapError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for MapError {}

impl Bit8Map {
    /// Parses a `.b8map` document and validates asset tokens against the project's registered sheets.
    pub fn parse(
        source: &str,
        source_path: &Path,
        tilesheets: &HashMap<String, Tilesheet>,
    ) -> Result<Self, MapError> {
        let mut version = None;
        let mut width = None;
        let mut height = None;
        let mut rows = Vec::new();
        let mut in_rows = false;
        let mut node_source = None;

        for (line_index, text) in source.lines().enumerate() {
            let line_number = line_index + 1;
            let line = text.trim();
            if line.is_empty() {
                continue;
            }

            if !in_rows {
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim();
                    match key {
                        "version" => {
                            set_header(&mut version, value, "version", source_path, line_number)?
                        }
                        "width" => {
                            set_header(&mut width, value, "width", source_path, line_number)?
                        }
                        "height" => {
                            set_header(&mut height, value, "height", source_path, line_number)?
                        }
                        _ => {
                            return Err(MapError(format!(
                                "{}:{line_number}: unknown map header '{key}'",
                                source_path.display()
                            )));
                        }
                    }
                    continue;
                }
                if line.starts_with("version")
                    || line.starts_with("width")
                    || line.starts_with("height")
                {
                    return Err(MapError(format!(
                        "{}:{line_number}: malformed map header; expected 'key = value'",
                        source_path.display()
                    )));
                }
                in_rows = true;
            }

            if line == "[node_state]" {
                node_source = Some(
                    source
                        .lines()
                        .skip(line_index)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                break;
            }

            rows.push((line_number, line));
        }

        let version = version.ok_or_else(|| {
            MapError(format!(
                "{}: missing version header (expected version = 1)",
                source_path.display()
            ))
        })?;
        if version != 1 {
            return Err(MapError(format!(
                "{}: unsupported map version {version}; expected 1",
                source_path.display()
            )));
        }
        let width = width
            .ok_or_else(|| MapError(format!("{}: missing width header", source_path.display())))?;
        let height = height
            .ok_or_else(|| MapError(format!("{}: missing height header", source_path.display())))?;
        validate_dimensions(width, height, source_path)?;

        if rows.len() != height {
            return Err(MapError(format!(
                "{}: expected {height} map rows, found {}",
                source_path.display(),
                rows.len()
            )));
        }

        let mut cells = Vec::with_capacity(width * height);
        for (row_index, (line_number, row_text)) in rows.into_iter().enumerate() {
            let tokens = row_text.split_ascii_whitespace().collect::<Vec<_>>();
            let map_row = row_index + 1;
            if tokens.len() != width {
                return Err(MapError(format!(
                    "{}:{line_number}: row {map_row} has {} cells; expected {width}",
                    source_path.display(),
                    tokens.len()
                )));
            }

            for (column_index, token) in tokens.into_iter().enumerate() {
                let column = column_index + 1;
                if token == "--" {
                    cells.push(MapCell::Empty);
                    continue;
                }
                let reference = parse_asset_reference(token).ok_or_else(|| {
                    cell_error(
                        source_path,
                        line_number,
                        map_row,
                        column,
                        format!("malformed asset reference/cell token '{token}'; expected '--' or a registered ID"),
                    )
                })?;
                let resolved =
                    resolve_asset_reference(&reference, tilesheets).map_err(|error| {
                        cell_error(source_path, line_number, map_row, column, error.to_string())
                    })?;
                cells.push(MapCell::Asset(resolved.id));
            }
        }

        let node_section = node_source
            .as_deref()
            .map(toml::from_str::<NodeSectionDocument>)
            .transpose()
            .map_err(|error| {
                MapError(format!(
                    "{}: invalid node section: {error}",
                    source_path.display()
                ))
            })?
            .unwrap_or_default()
            .node_state;
        let nodes = node_section
            .nodes
            .into_iter()
            .map(|node| persisted_node(node, source_path))
            .collect::<Result<Vec<_>, _>>()?;
        let next_node_id = validate_nodes(&nodes, node_section.next_id, source_path)?;

        Ok(Self {
            width,
            height,
            cells,
            nodes,
            next_node_id,
        })
    }

    /// Loads a map file and validates it using this project's `bit8.assets.toml` registry.
    pub fn load(map_path: &Path, project_path: &Path) -> Result<Self, MapError> {
        if !map_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("b8map"))
        {
            return Err(MapError(format!(
                "map path '{}' must use the .b8map extension",
                map_path.display()
            )));
        }
        let source = fs::read_to_string(map_path).map_err(|error| {
            MapError(format!(
                "cannot read map file '{}': {error}",
                map_path.display()
            ))
        })?;
        let tilesheets =
            project_assets::load_registered_tilesheets(project_path).map_err(|error| {
                MapError(format!(
                    "cannot load registered assets for map '{}': {error}",
                    map_path.display()
                ))
            })?;
        let map = Self::parse(&source, map_path, &tilesheets)?;
        let sprites = crate::sprite_definitions::SpriteRegistry::load_project(project_path)
            .map_err(|error| MapError(format!("{}: {error}", map_path.display())))?
            .unwrap_or_default();
        map.validate_sprites(&sprites, map_path)?;
        Ok(map)
    }

    pub fn validate_sprites(
        &self,
        sprites: &crate::sprite_definitions::SpriteRegistry,
        path: &Path,
    ) -> Result<(), MapError> {
        for node in &self.nodes {
            if let Some(name) = &node.sprite
                && sprites.get_sprite(name).is_none()
            {
                return Err(MapError(format!(
                    "{}: Node {:?} ({}) references unknown sprite {:?}",
                    path.display(),
                    node.name,
                    node.id,
                    name
                )));
            }
        }
        Ok(())
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn cells(&self) -> &[MapCell] {
        &self.cells
    }

    pub fn nodes(&self) -> &[MapNode] {
        &self.nodes
    }

    /// Reserves the next stable map-local ID. The counter is serialized even
    /// when a previously allocated node is later removed.
    pub fn allocate_node_id(&mut self) -> Result<String, MapError> {
        if self.next_node_id == u64::MAX {
            return Err(MapError("node ID space exhausted".to_owned()));
        }
        let id = format!("N{}", self.next_node_id);
        self.next_node_id += 1;
        Ok(id)
    }

    /// Serializes the map and optional node state without changing the source file.
    pub fn serialize(&self) -> Result<String, MapError> {
        let mut source = format!(
            "version = 1\nwidth = {}\nheight = {}\n\n",
            self.width, self.height
        );
        for row in self.cells.chunks(self.width) {
            let tokens = row
                .iter()
                .map(|cell| match cell {
                    MapCell::Empty => "--".to_owned(),
                    MapCell::Asset(id) => format!("{}{}", id.group, id.cell),
                })
                .collect::<Vec<_>>();
            source.push_str(&tokens.join(" "));
            source.push('\n');
        }
        if !self.nodes.is_empty() || self.next_node_id != 1 {
            source.push('\n');
            source.push_str(
                &toml::to_string(&NodeSectionForSerialization {
                    node_state: NodeSectionForSerializationData {
                        next_id: self.next_node_id,
                        nodes: &self.nodes,
                    },
                })
                .map_err(|error| MapError(format!("cannot serialize node section: {error}")))?,
            );
        }
        Ok(source)
    }

    /// Queries zero-based tile coordinates; returns `None` outside the map.
    pub fn cell_at(&self, x: usize, y: usize) -> Option<&MapCell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(y.checked_mul(self.width)?.checked_add(x)?)
    }

    pub fn inspection_report(&self) -> MapInspectionReport {
        let used_assets = self
            .cells
            .iter()
            .filter_map(|cell| match cell {
                MapCell::Empty => None,
                MapCell::Asset(id) => Some(id.clone()),
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|id| format!("{}{}", id.group, id.cell))
            .collect();
        MapInspectionReport {
            width: self.width,
            height: self.height,
            cells: self.cells.len(),
            used_assets,
        }
    }
}

fn persisted_node(node: PersistedMapNode, path: &Path) -> Result<MapNode, MapError> {
    let node_type = match node.node_type.as_deref() {
        None => NodeType::Node,
        Some(identifier) => NodeType::from_identifier(identifier).ok_or_else(|| {
            MapError(format!(
                "{}: node '{}' ('{}') has unknown type '{}'; expected Node or Camera",
                path.display(),
                node.id,
                node.name,
                identifier
            ))
        })?,
    };
    let collider = node
        .collider
        .map(|value| persisted_collider(value, &node.id, &node.name, path))
        .transpose()?;
    Ok(MapNode {
        id: node.id,
        node_type,
        name: node.name,
        x: node.x,
        y: node.y,
        enabled: node.enabled,
        script: node.script,
        sprite: node.sprite,
        collider,
        // Keep arbitrary/missing asset IDs loadable so stale editor metadata
        // cannot prevent opening a map. The editor marks unresolved IDs.
        visual: node
            .visual
            .and_then(|value| value.as_str().map(str::to_owned)),
    })
}

fn persisted_collider(
    value: toml::Value,
    id: &str,
    name: &str,
    path: &Path,
) -> Result<BoxCollider, MapError> {
    let invalid = |detail: &str| {
        MapError(format!(
            "{}: node '{}' ('{}') has invalid collider: {detail}",
            path.display(),
            id,
            name
        ))
    };
    let toml::Value::Table(fields) = value else {
        return Err(invalid("expected a table"));
    };
    for key in fields.keys() {
        if !matches!(
            key.as_str(),
            "enabled" | "offset_x" | "offset_y" | "width" | "height"
        ) {
            return Err(invalid(&format!("unknown field '{key}'")));
        }
    }
    let enabled = fields
        .get("enabled")
        .and_then(toml::Value::as_bool)
        .ok_or_else(|| invalid("enabled must be true or false"))?;
    let integer = |key: &str| {
        fields
            .get(key)
            .and_then(toml::Value::as_integer)
            .ok_or_else(|| invalid(&format!("{key} must be an integer")))
    };
    let offset_x = integer("offset_x")?;
    let offset_y = integer("offset_y")?;
    let width = integer("width")?;
    let height = integer("height")?;
    if width <= 0 {
        return Err(invalid("width must be a positive integer"));
    }
    if height <= 0 {
        return Err(invalid("height must be a positive integer"));
    }
    Ok(BoxCollider {
        enabled,
        offset_x,
        offset_y,
        width,
        height,
    })
}

fn validate_nodes(
    nodes: &[MapNode],
    serialized_next_id: Option<u64>,
    path: &Path,
) -> Result<u64, MapError> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    let mut highest_id = 0_u64;
    for (index, node) in nodes.iter().enumerate() {
        let suffix = node
            .id
            .strip_prefix('N')
            .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|suffix| suffix.parse::<u64>().ok())
            .filter(|suffix| *suffix > 0)
            .ok_or_else(|| {
                MapError(format!(
                    "{}: node {} has invalid ID '{}'; expected N followed by a positive integer",
                    path.display(),
                    index + 1,
                    node.id
                ))
            })?;
        if !ids.insert(node.id.as_str()) {
            return Err(MapError(format!(
                "{}: duplicate node ID '{}'",
                path.display(),
                node.id
            )));
        }
        if node.name.trim().is_empty() {
            return Err(MapError(format!(
                "{}: node {} name must not be empty",
                path.display(),
                node.id
            )));
        }
        if !names.insert(node.name.as_str()) {
            return Err(MapError(format!(
                "{}: duplicate node name '{}'",
                path.display(),
                node.name
            )));
        }
        highest_id = highest_id.max(suffix);
    }

    let minimum_next_id = highest_id
        .checked_add(1)
        .ok_or_else(|| MapError(format!("{}: node ID space exhausted", path.display())))?;
    let next_id = serialized_next_id.unwrap_or(minimum_next_id);
    if next_id == 0 || next_id < minimum_next_id {
        return Err(MapError(format!(
            "{}: node next_id must be at least {minimum_next_id}",
            path.display()
        )));
    }
    Ok(next_id)
}

fn set_header(
    target: &mut Option<usize>,
    raw: &str,
    name: &str,
    path: &Path,
    line: usize,
) -> Result<(), MapError> {
    if target.is_some() {
        return Err(MapError(format!(
            "{}:{line}: duplicate {name} header",
            path.display()
        )));
    }
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(MapError(format!(
            "{}:{line}: {name} must be a positive integer, found '{raw}'",
            path.display()
        )));
    }
    let parsed = raw.parse::<usize>().map_err(|_| {
        MapError(format!(
            "{}:{line}: {name} is too large: '{raw}'",
            path.display()
        ))
    })?;
    *target = Some(parsed);
    Ok(())
}

fn validate_dimensions(width: usize, height: usize, path: &Path) -> Result<(), MapError> {
    if width == 0 || height == 0 {
        return Err(MapError(format!(
            "{}: map dimensions must be greater than zero (found {width} × {height})",
            path.display()
        )));
    }
    if width > MAX_MAP_WIDTH || height > MAX_MAP_HEIGHT {
        return Err(MapError(format!(
            "{}: map dimensions may not exceed {MAX_MAP_WIDTH} × {MAX_MAP_HEIGHT} tiles",
            path.display()
        )));
    }
    let Some(cell_count) = width.checked_mul(height) else {
        return Err(MapError(format!(
            "{}: map cell count is too large",
            path.display()
        )));
    };
    if cell_count > MAX_MAP_CELLS {
        return Err(MapError(format!(
            "{}: map may not contain more than {MAX_MAP_CELLS} cells",
            path.display()
        )));
    }
    Ok(())
}

fn cell_error(path: &Path, line: usize, row: usize, column: usize, message: String) -> MapError {
    MapError(format!(
        "{}:{line} (row {row}, column {column}): {message}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_type::{BUILT_IN_NODE_TYPES, NodeType};
    use image::{Rgba, RgbaImage};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

    fn project_with_assets() -> std::path::PathBuf {
        let project = std::env::temp_dir().join(format!(
            "bit8-map-core-{}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&project).unwrap();
        let sheet_a = RgbaImage::from_pixel(32, 8, Rgba([0, 0, 0, 255]));
        let sheet_b = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));
        sheet_a.save(project.join("a.png")).unwrap();
        sheet_b.save(project.join("b.png")).unwrap();
        sheet_b.save(project.join("c.png")).unwrap();
        fs::write(
            project.join(project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"a.png\"\n\n[[tilesheets]]\ngroup = \"B\"\nfile = \"b.png\"\n",
        )
        .unwrap();
        project
    }

    fn sheets(project: &Path) -> HashMap<String, Tilesheet> {
        project_assets::load_registered_tilesheets(project).unwrap()
    }

    fn map_text(width: usize, height: usize, rows: &[&str]) -> String {
        format!(
            "version = 1\nwidth = {width}\nheight = {height}\n\n{}\n",
            rows.join("\n")
        )
    }

    #[test]
    fn parses_eight_by_eight_map_empty_cells_and_asset_identity() {
        let project = project_with_assets();
        let rows = [
            "A1 A1 A1 A1 A1 A1 A1 A1",
            "A1 -- -- -- -- -- -- A1",
            "A1 -- A4 -- B1 -- -- A1",
            "A1 -- -- -- -- -- -- A1",
            "A2 A2 A2 A2 A2 A2 A2 A2",
            "A1 A1 A1 A1 A1 A1 A1 A1",
            "-- -- -- -- -- -- -- --",
            "A3 A3 A3 A3 A3 A3 A3 A3",
        ];
        let source = map_text(8, 8, &rows);
        let map = Bit8Map::parse(&source, Path::new("world.b8map"), &sheets(&project)).unwrap();

        assert_eq!((map.width(), map.height(), map.cells().len()), (8, 8, 64));
        assert_eq!(map.cell_at(1, 1), Some(&MapCell::Empty));
        assert_eq!(
            map.cell_at(2, 2),
            Some(&MapCell::Asset(AssetId {
                group: "A".to_owned(),
                cell: 4
            }))
        );
        assert_eq!(
            map.cell_at(4, 2),
            Some(&MapCell::Asset(AssetId {
                group: "B".to_owned(),
                cell: 1
            }))
        );
        assert_eq!(map.cell_at(8, 0), None);
        assert_eq!(
            map.inspection_report().used_assets,
            ["A1", "A2", "A3", "A4", "B1"]
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn parses_rectangular_map_and_is_deterministic() {
        let project = project_with_assets();
        let source = map_text(3, 2, &["A1 -- B1", "A4 A2 --"]);
        let registered = sheets(&project);
        let first = Bit8Map::parse(&source, Path::new("rect.b8map"), &registered).unwrap();
        let second = Bit8Map::parse(&source, Path::new("rect.b8map"), &registered).unwrap();
        assert_eq!((first.width(), first.height()), (3, 2));
        assert_eq!(first, second);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn parses_and_round_trips_stable_nodes_and_reserves_retired_ids() {
        let project = project_with_assets();
        let source = concat!(
            "version = 1\nwidth = 2\nheight = 1\n\nA1 --\n\n",
            "[node_state]\nnext_id = 3\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 256\ny = 224\nenabled = true\nscript = \"player.b8\"\n\n",
            "[[node_state.nodes]]\nid = \"N2\"\nname = \"Camera\"\nx = 224\ny = 192\nenabled = false\n"
        );
        let registered = sheets(&project);
        let map = Bit8Map::parse(source, Path::new("world.b8map"), &registered).unwrap();
        assert_eq!(map.nodes().len(), 2);
        assert_eq!(map.nodes()[0].id, "N1");
        assert_eq!(map.nodes()[0].node_type, NodeType::Node);
        assert_eq!(map.nodes()[0].name, "Player");
        assert_eq!((map.nodes()[0].x, map.nodes()[0].y), (256, 224));
        assert!(map.nodes()[0].enabled);
        assert_eq!(map.nodes()[0].script.as_deref(), Some("player.b8"));
        assert_eq!(
            map.nodes()[0].collider,
            None,
            "old node records have no collider by default"
        );
        assert_eq!(map.nodes()[1].script, None);
        assert!(!map.nodes()[1].enabled);
        assert_eq!(
            map.nodes()[1].node_type,
            NodeType::Node,
            "a name of Camera does not imply a type"
        );

        let serialized = map.serialize().unwrap();
        let reloaded = Bit8Map::parse(&serialized, Path::new("world.b8map"), &registered).unwrap();
        assert_eq!(reloaded, map);
        assert!(serialized.contains("next_id = 3"));
        assert!(serialized.contains("type = \"Node\""));

        let mut allocation_map = Bit8Map::parse(
            "version = 1\nwidth = 1\nheight = 1\n--\n\n[node_state]\nnext_id = 8\n",
            Path::new("reserved.b8map"),
            &registered,
        )
        .unwrap();
        assert_eq!(allocation_map.allocate_node_id().unwrap(), "N8");
        let reloaded = Bit8Map::parse(
            &allocation_map.serialize().unwrap(),
            Path::new("reserved.b8map"),
            &registered,
        )
        .unwrap();
        assert_eq!(reloaded.next_node_id, 9);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_visual_metadata_round_trips_without_resolving_or_affecting_runtime_identity() {
        let project = project_with_assets();
        let registered = sheets(&project);
        let source = concat!(
            "version = 1\nwidth = 1\nheight = 1\n--\n\n[node_state]\nnext_id = 2\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 8\ny = 16\nenabled = true\nvisual = \"A99\"\n\n",
            "[node_state.nodes.collider]\nenabled = true\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n"
        );
        let map = Bit8Map::parse(source, Path::new("visual.b8map"), &registered).unwrap();
        assert_eq!(map.width(), 1);
        assert_eq!(map.height(), 1);
        assert_eq!(map.cells().len(), 1);
        assert_eq!(map.nodes()[0].visual.as_deref(), Some("A99"));
        assert_eq!(map.nodes()[0].collider.as_ref().unwrap().width, 8);

        let serialized = map.serialize().unwrap();
        assert!(serialized.contains("visual = \"A99\""));
        let reloaded = Bit8Map::parse(&serialized, Path::new("visual.b8map"), &registered).unwrap();
        assert_eq!(
            reloaded, map,
            "unresolved editor visuals remain round-trippable"
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn node_box_collider_parses_round_trips_and_reports_malformed_dimensions_with_context() {
        let project = project_with_assets();
        let registered = sheets(&project);
        let source = concat!(
            "version = 1\nwidth = 1\nheight = 1\n--\n\n[node_state]\nnext_id = 2\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 16\ny = 24\nenabled = true\nscript = \"player.b8\"\n\n",
            "[node_state.nodes.collider]\nenabled = true\noffset_x = -2\noffset_y = 3\nwidth = 16\nheight = 12\n"
        );
        let map = Bit8Map::parse(source, Path::new("collider.b8map"), &registered).unwrap();
        assert_eq!(
            map.nodes()[0].collider,
            Some(BoxCollider {
                enabled: true,
                offset_x: -2,
                offset_y: 3,
                width: 16,
                height: 12,
            })
        );
        let serialized = map.serialize().unwrap();
        assert!(serialized.contains("[node_state.nodes.collider]"));
        assert!(serialized.contains("offset_x = -2"));
        assert_eq!(
            Bit8Map::parse(&serialized, Path::new("collider.b8map"), &registered).unwrap(),
            map
        );

        for (field, value, expected) in [("width", "0", "positive"), ("height", "-1", "positive")] {
            let malformed = source.replace(
                &format!("{field} = {}", if field == "width" { "16" } else { "12" }),
                &format!("{field} = {value}"),
            );
            let error = Bit8Map::parse(&malformed, Path::new("broken-collider.b8map"), &registered)
                .unwrap_err()
                .to_string();
            assert!(error.contains("broken-collider.b8map"), "{error}");
            assert!(error.contains("N1") && error.contains("Player"), "{error}");
            assert!(error.contains(field) && error.contains(expected), "{error}");
        }
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn parses_and_round_trips_registered_node_types_without_inference() {
        let project = project_with_assets();
        let registered = sheets(&project);
        let source = concat!(
            "version = 1\nwidth = 2\nheight = 1\n\nA1 --\n\n",
            "[node_state]\nnext_id = 4\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\ntype = \"Node\"\nname = \"Camera\"\nx = 8\ny = 16\nenabled = true\nscript = \"player.b8\"\n\n",
            "[[node_state.nodes]]\nid = \"N3\"\ntype = \"Camera\"\nname = \"MainView\"\nx = 24\ny = 32\nenabled = false\nscript = \"camera.b8\"\n"
        );
        let map = Bit8Map::parse(source, Path::new("typed.b8map"), &registered).unwrap();
        assert_eq!(
            map.nodes()[0].node_type,
            NodeType::Node,
            "name never determines identity"
        );
        assert_eq!(map.nodes()[1].node_type, NodeType::Camera);
        assert_eq!(map.nodes()[1].name, "MainView");
        assert_eq!(map.nodes()[1].script.as_deref(), Some("camera.b8"));
        assert_eq!(map.nodes()[1].x, 24);
        assert!(!map.nodes()[1].enabled);
        assert_eq!(map.cells().len(), 2);
        assert_eq!(
            map.cell_at(0, 0),
            Some(&MapCell::Asset(AssetId {
                group: "A".into(),
                cell: 1
            }))
        );

        let serialized = map.serialize().unwrap();
        assert!(serialized.contains("type = \"Node\""));
        assert!(serialized.contains("type = \"Camera\""));
        assert!(serialized.contains("next_id = 4"));
        assert!(serialized.contains("script = \"camera.b8\""));
        let reloaded = Bit8Map::parse(&serialized, Path::new("typed.b8map"), &registered).unwrap();
        assert_eq!(reloaded, map);
        assert_eq!(BUILT_IN_NODE_TYPES, &[NodeType::Node, NodeType::Camera]);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn rejects_unknown_node_type_with_map_path_id_name_and_value() {
        let project = project_with_assets();
        let source = concat!(
            "version = 1\nwidth = 1\nheight = 1\n--\n\n[node_state]\nnext_id = 2\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\ntype = \"Banana\"\nname = \"Mystery\"\nx = 0\ny = 0\nenabled = true\n"
        );
        let error = Bit8Map::parse(source, Path::new("unknown-type.b8map"), &sheets(&project))
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown-type.b8map"));
        assert!(error.contains("N1"));
        assert!(error.contains("Mystery"));
        assert!(error.contains("Banana"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn rejects_duplicate_node_ids_names_and_malformed_node_records() {
        let project = project_with_assets();
        let registered = sheets(&project);
        let node = |id: &str, name: &str| {
            format!(
                "[[node_state.nodes]]\nid = \"{id}\"\nname = \"{name}\"\nx = 0\ny = 0\nenabled = true\n"
            )
        };
        let cases = [
            (format!("{}{}", node("N1", "Player"), node("N1", "Other")), "duplicate node ID 'N1'"),
            (format!("{}{}", node("N1", "Player"), node("N2", "Player")), "duplicate node name 'Player'"),
            ("[[node_state.nodes]]\nid = \"Player\"\nname = \"Player\"\nx = 0\ny = 0\nenabled = true\n".to_owned(), "invalid ID"),
            ("[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 0\nenabled = true\n".to_owned(), "invalid node section"),
        ];
        for (records, expected) in cases {
            let source = format!(
                "version = 1\nwidth = 1\nheight = 1\n--\n\n[node_state]\nnext_id = 2\n{records}"
            );
            let error = Bit8Map::parse(&source, Path::new("nodes.b8map"), &registered)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains(expected),
                "expected {expected:?}, got {error}"
            );
        }
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn rejects_missing_or_invalid_headers_and_dimensions() {
        let project = project_with_assets();
        let registered = sheets(&project);
        let cases = [
            ("version = 1\nheight = 1\nA1\n", "missing width"),
            ("version = 1\nwidth = 1\nA1\n", "missing height"),
            ("version = 1\nheight = 1\nA1\n", "missing width"),
            (
                "version = 1\nwidth = 0\nheight = 1\nA1\n",
                "greater than zero",
            ),
            (
                "version = 1\nwidth = -1\nheight = 1\nA1\n",
                "positive integer",
            ),
            (
                "version = 1\nwidth = nope\nheight = 1\nA1\n",
                "positive integer",
            ),
            (
                "version = 1\nwidth = 257\nheight = 1\nA1\n",
                "may not exceed",
            ),
            (
                "version = 2\nwidth = 1\nheight = 1\nA1\n",
                "unsupported map version",
            ),
        ];
        for (source, expected) in cases {
            let error = Bit8Map::parse(source, Path::new("invalid.b8map"), &registered)
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{source:?}: {error}");
        }
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn rejects_row_column_and_malformed_empty_token_errors_with_location() {
        let project = project_with_assets();
        let registered = sheets(&project);
        let cases = [
            (map_text(1, 2, &["A1"]), "expected 2 map rows"),
            (map_text(2, 1, &["A1"]), "row 1 has 1 cells; expected 2"),
            (map_text(1, 1, &["-"]), "row 1, column 1"),
        ];
        for (source, expected) in cases {
            let error = Bit8Map::parse(&source, Path::new("broken.b8map"), &registered)
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{error}");
        }
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn rejects_unknown_group_bad_reference_and_out_of_range_cell() {
        let project = project_with_assets();
        let registered = sheets(&project);
        for (symbol, expected) in [
            ("C1", "unknown tilesheet group"),
            ("A5", "out of range"),
            ("A-1", "malformed asset reference"),
        ] {
            let source = map_text(1, 1, &[symbol]);
            let error = Bit8Map::parse(&source, Path::new("bad-id.b8map"), &registered)
                .unwrap_err()
                .to_string();
            assert!(error.contains(symbol), "symbol missing from error: {error}");
            assert!(error.contains(expected), "expected '{expected}': {error}");
        }
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn load_reports_missing_files_and_uses_the_project_registry() {
        let project = project_with_assets();
        let map_path = project.join("world.b8map");
        fs::write(&map_path, map_text(1, 1, &["B1"])).unwrap();
        let first = Bit8Map::load(&map_path, &project).unwrap();
        let second = Bit8Map::load(&map_path, &project).unwrap();
        assert_eq!(first, second);
        fs::remove_file(&map_path).unwrap();
        let error = Bit8Map::load(&map_path, &project).unwrap_err().to_string();
        assert!(error.contains("world.b8map"));
        fs::remove_dir_all(project).unwrap();
    }
}
