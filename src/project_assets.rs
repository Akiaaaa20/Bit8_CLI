use crate::palette::PALETTE;
use crate::tilesheet::Tilesheet;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{Error as IoError, ErrorKind, Write};
use std::path::{Component, Path, PathBuf};

pub const REGISTRY_FILE: &str = "bit8.assets.toml";

/// Stable public identity of one 1-based cell in a registered tilesheet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId {
    pub group: String,
    pub cell: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedAssetReference {
    pub symbol: String,
    pub group: String,
    pub cell_number: Option<usize>,
}

pub(crate) struct ResolvedAssetReference<'a> {
    pub id: AssetId,
    pub sprite: &'a [u8; crate::framebuffer::SPRITE_SIZE * crate::framebuffer::SPRITE_SIZE],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AssetReferenceError {
    UnknownGroup { symbol: String },
    InvalidCell { symbol: String },
    OutOfRange { symbol: String, group: String },
}

impl std::fmt::Display for AssetReferenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownGroup { symbol } => write!(
                formatter,
                "unknown tilesheet group in Bit8 asset reference '{symbol}'"
            ),
            Self::InvalidCell { symbol } => {
                write!(
                    formatter,
                    "Bit8 asset reference '{symbol}' has an invalid cell number"
                )
            }
            Self::OutOfRange { symbol, group } => write!(
                formatter,
                "Bit8 asset reference '{symbol}' is out of range for group {group}"
            ),
        }
    }
}

impl std::error::Error for AssetReferenceError {}

pub(crate) fn parse_asset_reference(symbol: &str) -> Option<ParsedAssetReference> {
    let digit_start = symbol.find(|character: char| character.is_ascii_digit())?;
    let (group, number) = symbol.split_at(digit_start);
    if group.is_empty()
        || !group.bytes().all(|byte| byte.is_ascii_uppercase())
        || number.is_empty()
        || !number.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    Some(ParsedAssetReference {
        symbol: symbol.to_owned(),
        group: group.to_owned(),
        cell_number: number.parse().ok(),
    })
}

pub(crate) fn resolve_asset_reference<'a>(
    reference: &ParsedAssetReference,
    tilesheets: &'a HashMap<String, Tilesheet>,
) -> Result<ResolvedAssetReference<'a>, AssetReferenceError> {
    resolve_asset_id_with_symbol(
        &reference.group,
        reference.cell_number,
        &reference.symbol,
        tilesheets,
    )
}

pub(crate) fn resolve_asset_id<'a>(
    id: &AssetId,
    tilesheets: &'a HashMap<String, Tilesheet>,
) -> Result<ResolvedAssetReference<'a>, AssetReferenceError> {
    resolve_asset_id_with_symbol(
        &id.group,
        Some(id.cell),
        &format!("{}{}", id.group, id.cell),
        tilesheets,
    )
}

fn resolve_asset_id_with_symbol<'a>(
    group: &str,
    cell_number: Option<usize>,
    symbol: &str,
    tilesheets: &'a HashMap<String, Tilesheet>,
) -> Result<ResolvedAssetReference<'a>, AssetReferenceError> {
    let Some(sheet) = tilesheets.get(group) else {
        return Err(AssetReferenceError::UnknownGroup {
            symbol: symbol.to_owned(),
        });
    };
    let Some(cell) = cell_number.filter(|cell| *cell > 0) else {
        return Err(AssetReferenceError::InvalidCell {
            symbol: symbol.to_owned(),
        });
    };
    let sprite_index = cell
        .checked_sub(1)
        .and_then(|index| i64::try_from(index).ok());
    let Some(sprite) = sprite_index.and_then(|index| sheet.cell(index)) else {
        return Err(AssetReferenceError::OutOfRange {
            symbol: symbol.to_owned(),
            group: group.to_owned(),
        });
    };
    Ok(ResolvedAssetReference {
        id: AssetId {
            group: group.to_owned(),
            cell,
        },
        sprite,
    })
}

#[derive(Debug, Deserialize, Serialize)]
struct Registry {
    version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    retired_groups: Vec<String>,
    #[serde(default)]
    tilesheets: Vec<TilesheetEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TilesheetEntry {
    group: String,
    file: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    solid: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct InspectionReport {
    pub tilesheets: Vec<InspectedTilesheet>,
}

#[derive(Debug, Serialize)]
pub struct InspectedTilesheet {
    pub group: String,
    pub file: String,
    pub width: u32,
    pub height: u32,
    pub columns: usize,
    pub rows: usize,
    /// The exact Runtime palette values used to render the mapped pixel indices below.
    pub palette: Vec<u32>,
    pub cells: Vec<InspectedCell>,
}

#[derive(Debug, Serialize)]
pub struct InspectedCell {
    pub id: String,
    pub x: usize,
    pub y: usize,
    #[serde(default)]
    pub solid: bool,
    /// Row-major 8x8 Runtime palette indices; index 0 is transparent.
    pub pixels: Vec<u8>,
}

#[derive(Debug, Serialize)]
pub struct TilesheetChange {
    pub group: String,
    pub file: String,
}

pub fn load_registered_tilesheets(
    project: &Path,
) -> Result<HashMap<String, Tilesheet>, Box<dyn Error>> {
    let project = resolve_project_dir(project)?;
    let registry = load_registry(&project)?;
    let mut paths = HashSet::new();
    let mut sheets = HashMap::with_capacity(registry.tilesheets.len());
    for entry in registry.tilesheets {
        let path = resolve_registered_file(&project, &entry.file)?;
        if !paths.insert(path.clone()) {
            return Err(invalid_registry(format!(
                "tilesheet file '{}' is registered more than once",
                entry.file
            ))
            .into());
        }
        let sheet = Tilesheet::load_png(&path).map_err(|error| {
            IoError::new(
                ErrorKind::InvalidData,
                format!("cannot load registered tilesheet '{}': {error}", entry.file),
            )
        })?;
        validate_solid_cells(&entry.group, &entry.file, &entry.solid, sheet.cell_count())?;
        sheets.insert(entry.group, sheet);
    }
    Ok(sheets)
}

/// Returns stable IDs for the cells explicitly marked Solid in the project registry.
/// The caller loads tilesheets separately, which validates each cell number against
/// the corresponding image dimensions.
pub fn load_registered_solid_cells(project: &Path) -> Result<HashSet<AssetId>, Box<dyn Error>> {
    let project = resolve_project_dir(project)?;
    let registry = load_registry(&project)?;
    validate_registry(&registry)?;

    Ok(registry
        .tilesheets
        .into_iter()
        .flat_map(|entry| {
            entry.solid.into_iter().map(move |cell| AssetId {
                group: entry.group.clone(),
                cell,
            })
        })
        .collect())
}

pub fn add_tilesheet(project: &Path, png: &Path) -> Result<TilesheetChange, Box<dyn Error>> {
    let project = resolve_project_dir(project)?;
    let png_metadata = fs::symlink_metadata(png)?;
    if png_metadata.file_type().is_symlink() {
        return Err(
            invalid_registry("symbolic-link PNG imports are not supported".to_owned()).into(),
        );
    }
    let source = png.canonicalize()?;
    if !source.starts_with(&project) || !source.is_file() {
        return Err(invalid_registry(
            "the selected PNG must be a file inside the project".to_owned(),
        )
        .into());
    }
    if !source
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err(invalid_registry("the selected file must be a PNG".to_owned()).into());
    }
    Tilesheet::load_png(&source)?;

    let mut registry = load_registry(&project)?;
    let existing = inspect_tilesheets(&project)?;
    if existing.tilesheets.iter().any(|sheet| {
        project
            .join(&sheet.file)
            .canonicalize()
            .is_ok_and(|path| path == source)
    }) {
        return Err(invalid_registry("the selected PNG is already registered".to_owned()).into());
    }

    let group = next_group_id(&registry);
    let new_path = canonical_filename(&source, &group);
    let file = relative_registry_path(&project, &new_path)?;
    registry.tilesheets.push(TilesheetEntry {
        group: group.clone(),
        file: file.clone(),
        solid: Vec::new(),
    });
    move_and_write_registry(&project, &source, &new_path, &registry)?;
    Ok(TilesheetChange { group, file })
}

pub fn apply_tilesheet_name(
    project: &Path,
    group: &str,
) -> Result<TilesheetChange, Box<dyn Error>> {
    let project = resolve_project_dir(project)?;
    let mut registry = load_registry(&project)?;
    inspect_tilesheets(&project)?;
    let entry = registry
        .tilesheets
        .iter_mut()
        .find(|entry| entry.group == group)
        .ok_or_else(|| invalid_registry(format!("tilesheet group '{group}' is not registered")))?;
    let old_file = entry.file.clone();
    let source = resolve_registered_file(&project, &old_file)?;
    let new_path = canonical_filename(&source, group);
    let file = relative_registry_path(&project, &new_path)?;
    if file == old_file {
        return Ok(TilesheetChange {
            group: group.to_owned(),
            file,
        });
    }
    entry.file = file.clone();
    move_and_write_registry(&project, &source, &new_path, &registry)?;
    Ok(TilesheetChange {
        group: group.to_owned(),
        file,
    })
}

/// Persist the Solid flag for one stable, 1-based registered tile ID.
pub fn set_tile_solid(project: &Path, symbol: &str, solid: bool) -> Result<(), Box<dyn Error>> {
    let project = resolve_project_dir(project)?;
    let reference = parse_asset_reference(symbol)
        .ok_or_else(|| invalid_registry(format!("invalid Bit8 tile ID '{symbol}'")))?;
    let mut registry = load_registry(&project)?;
    let report = inspect_tilesheets(&project)?;
    let sheet = report
        .tilesheets
        .iter()
        .find(|sheet| sheet.group == reference.group)
        .ok_or_else(|| {
            invalid_registry(format!("unknown tilesheet group in tile ID '{symbol}'"))
        })?;
    let cell_number = reference
        .cell_number
        .filter(|number| *number > 0 && *number <= sheet.cells.len())
        .ok_or_else(|| {
            invalid_registry(format!(
                "tile ID '{symbol}' is out of range for group {}",
                reference.group
            ))
        })?;
    let entry = registry
        .tilesheets
        .iter_mut()
        .find(|entry| entry.group == reference.group)
        .ok_or_else(|| {
            invalid_registry(format!("unknown tilesheet group in tile ID '{symbol}'"))
        })?;
    if solid {
        if !entry.solid.contains(&cell_number) {
            entry.solid.push(cell_number);
            entry.solid.sort_unstable();
        }
    } else {
        entry.solid.retain(|cell| *cell != cell_number);
    }
    let registry_path = project.join(REGISTRY_FILE);
    move_and_write_registry(&project, &registry_path, &registry_path, &registry)?;
    Ok(())
}

fn validate_solid_cells(
    group: &str,
    file: &str,
    solid: &[usize],
    cell_count: usize,
) -> Result<(), IoError> {
    let mut seen = HashSet::new();
    for cell in solid {
        if *cell == 0 || *cell > cell_count || !seen.insert(cell) {
            return Err(invalid_registry(format!(
                "tilesheet '{file}' group {group} has invalid or duplicate Solid cell {group}{cell}; valid cells are 1 through {cell_count}"
            )));
        }
    }
    Ok(())
}

fn resolve_project_dir(project: &Path) -> Result<PathBuf, IoError> {
    let project = project.canonicalize()?;
    if !project.is_dir() {
        return Err(IoError::new(
            ErrorKind::NotFound,
            "project path is not a directory",
        ));
    }
    Ok(project)
}

fn load_registry(project: &Path) -> Result<Registry, Box<dyn Error>> {
    let path = project.join(REGISTRY_FILE);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(Registry {
                version: 1,
                retired_groups: vec![],
                tilesheets: vec![],
            });
        }
        Err(error) => return Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(
                invalid_registry("registry file must not be a symbolic link".to_owned()).into(),
            );
        }
        Ok(_) => {}
    }
    let registry: Registry = toml::from_str(&fs::read_to_string(path)?)?;
    validate_registry(&registry)?;
    Ok(registry)
}

fn next_group_id(registry: &Registry) -> String {
    let used: HashSet<&str> = registry
        .tilesheets
        .iter()
        .map(|entry| entry.group.as_str())
        .chain(registry.retired_groups.iter().map(String::as_str))
        .collect();
    (0usize..)
        .map(group_id)
        .find(|group| !used.contains(group.as_str()))
        .unwrap()
}

fn group_id(mut index: usize) -> String {
    let mut group = String::new();
    loop {
        let letter = (index % 26) as u8;
        group.insert(0, char::from(b'A' + letter));
        index /= 26;
        if index == 0 {
            return group;
        }
        index -= 1;
    }
}

fn canonical_filename(source: &Path, group: &str) -> PathBuf {
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let suffix = format!(" ({group})");
    let stem = stem.strip_suffix(&suffix).unwrap_or(&stem);
    source.with_file_name(format!("{stem}{suffix}.png"))
}

fn relative_registry_path(project: &Path, file: &Path) -> Result<String, IoError> {
    let relative = file.strip_prefix(project).map_err(|_| {
        invalid_registry("the resulting tilesheet path must stay inside the project".to_owned())
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn move_and_write_registry(
    project: &Path,
    source: &Path,
    target: &Path,
    registry: &Registry,
) -> Result<(), Box<dyn Error>> {
    let registry_path = project.join(REGISTRY_FILE);
    let source = source.to_path_buf();
    let target = target.to_path_buf();
    if target != source && fs::symlink_metadata(&target).is_ok() {
        return Err(IoError::new(
            ErrorKind::AlreadyExists,
            format!("refusing to overwrite '{}'", target.display()),
        )
        .into());
    }

    let serialized = toml::to_string_pretty(registry)?;
    let mut temp_path = project.join(format!(".{REGISTRY_FILE}.{}.tmp", std::process::id()));
    let mut suffix = 0u32;
    let mut temp = loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => break file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                suffix += 1;
                temp_path = project.join(format!(
                    ".{REGISTRY_FILE}.{}-{suffix}.tmp",
                    std::process::id()
                ));
            }
            Err(error) => return Err(error.into()),
        }
    };
    if let Err(error) = temp
        .write_all(serialized.as_bytes())
        .and_then(|()| temp.sync_all())
    {
        let _ = fs::remove_file(&temp_path);
        return Err(error.into());
    }
    drop(temp);

    let moved = target != source;
    if moved {
        if let Err(error) = fs::hard_link(&source, &target) {
            let _ = fs::remove_file(&temp_path);
            return Err(error.into());
        }
        if let Err(error) = fs::remove_file(&source) {
            let _ = fs::remove_file(&target);
            let _ = fs::remove_file(&temp_path);
            return Err(error.into());
        }
    }

    if let Err(error) = fs::rename(&temp_path, &registry_path) {
        let rollback = if moved {
            fs::hard_link(&target, &source).and_then(|()| fs::remove_file(&target))
        } else {
            Ok(())
        };
        let _ = fs::remove_file(&temp_path);
        if let Err(rollback_error) = rollback {
            return Err(format!(
                "registry update failed ({error}); file rollback also failed ({rollback_error})"
            )
            .into());
        }
        return Err(error.into());
    }
    Ok(())
}

pub fn inspect_tilesheets(project: &Path) -> Result<InspectionReport, Box<dyn Error>> {
    let project = project.canonicalize().map_err(|error| {
        IoError::new(
            error.kind(),
            format!(
                "cannot resolve project directory '{}': {error}",
                project.display()
            ),
        )
    })?;
    if !project.is_dir() {
        return Err(IoError::new(
            ErrorKind::NotFound,
            format!("project directory not found: {}", project.display()),
        )
        .into());
    }

    let registry_path = project.join(REGISTRY_FILE);
    if !registry_path.exists() {
        return Ok(InspectionReport { tilesheets: vec![] });
    }
    let source = fs::read_to_string(&registry_path)?;
    let registry: Registry = toml::from_str(&source)?;
    validate_registry(&registry)?;

    let mut paths = HashSet::new();
    let mut sheets = Vec::with_capacity(registry.tilesheets.len());
    for entry in registry.tilesheets {
        let path = resolve_registered_file(&project, &entry.file)?;
        if !paths.insert(path.clone()) {
            return Err(invalid_registry(format!(
                "tilesheet file '{}' is registered more than once",
                entry.file
            ))
            .into());
        }

        let sheet = Tilesheet::load_png(&path).map_err(|error| {
            IoError::new(
                ErrorKind::InvalidData,
                format!("cannot load registered tilesheet '{}': {error}", entry.file),
            )
        })?;
        let (width, height) = sheet.dimensions();
        let (columns, rows) = sheet.cell_grid();
        validate_solid_cells(&entry.group, &entry.file, &entry.solid, columns * rows)?;
        let mut cells = Vec::with_capacity(columns * rows);
        for y in 0..rows {
            for x in 0..columns {
                let index = y * columns + x;
                cells.push(InspectedCell {
                    id: public_cell_id(&entry.group, index),
                    x,
                    y,
                    solid: entry.solid.contains(&(index + 1)),
                    pixels: sheet.cell(index as i64).unwrap().to_vec(),
                });
            }
        }
        sheets.push(InspectedTilesheet {
            group: entry.group,
            file: entry.file,
            width,
            height,
            columns,
            rows,
            palette: PALETTE.to_vec(),
            cells,
        });
    }

    Ok(InspectionReport { tilesheets: sheets })
}

pub fn public_cell_id(group: &str, zero_based_index: usize) -> String {
    format!("{group}{}", zero_based_index + 1)
}

fn validate_registry(registry: &Registry) -> Result<(), IoError> {
    if registry.version != 1 {
        return Err(invalid_registry(format!(
            "unsupported registry version {}; expected 1",
            registry.version
        )));
    }

    let mut retired = HashSet::new();
    for group in &registry.retired_groups {
        if !valid_group_id(group) || !retired.insert(group) {
            return Err(invalid_registry(format!(
                "invalid or duplicate retired tilesheet group '{group}'"
            )));
        }
    }

    let mut active = HashSet::new();
    for entry in &registry.tilesheets {
        if !valid_group_id(&entry.group) {
            return Err(invalid_registry(format!(
                "invalid tilesheet group '{}'; use uppercase letters such as A or B",
                entry.group
            )));
        }
        if !active.insert(&entry.group) {
            return Err(invalid_registry(format!(
                "tilesheet group '{}' is registered more than once",
                entry.group
            )));
        }
        if retired.contains(&entry.group) {
            return Err(invalid_registry(format!(
                "tilesheet group '{}' is retired and cannot be reused",
                entry.group
            )));
        }
    }

    Ok(())
}

fn valid_group_id(group: &str) -> bool {
    !group.is_empty() && group.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn resolve_registered_file(project: &Path, file: &str) -> Result<PathBuf, IoError> {
    let relative = Path::new(file);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || !relative
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err(invalid_registry(format!(
            "registered tilesheet path '{file}' must be a relative PNG path inside the project"
        )));
    }

    let path = project.join(relative).canonicalize().map_err(|error| {
        IoError::new(
            error.kind(),
            format!("registered tilesheet '{file}' cannot be opened: {error}"),
        )
    })?;
    if !path.starts_with(project) || !path.is_file() {
        return Err(invalid_registry(format!(
            "registered tilesheet '{file}' must resolve to a file inside the project"
        )));
    }
    Ok(path)
}

fn invalid_registry(message: String) -> IoError {
    IoError::new(
        ErrorKind::InvalidData,
        format!("{REGISTRY_FILE}: {message}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    fn project() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bit8-assets-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo/tilesheet (A).png"),
            path.join("sheet.png"),
        )
        .unwrap();
        path
    }

    fn write_registry(project: &Path, body: &str) {
        fs::write(project.join(REGISTRY_FILE), body).unwrap();
    }

    #[test]
    fn registered_sheet_ids_follow_row_major_order() {
        let project = project();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            32,
            32,
            image::Rgba([80, 120, 160, 255]),
        ))
        .save_with_format(project.join("sheet.png"), image::ImageFormat::Png)
        .unwrap();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        );
        let report = inspect_tilesheets(&project).unwrap();

        assert_eq!(report.tilesheets.len(), 1);
        let sheet = &report.tilesheets[0];
        assert_eq!(sheet.width, 32);
        assert_eq!(sheet.height, 32);
        assert_eq!((sheet.columns, sheet.rows), (4, 4));
        assert_eq!(
            sheet
                .cells
                .iter()
                .map(|cell| cell.id.as_str())
                .collect::<Vec<_>>(),
            [
                "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11", "A12", "A13",
                "A14", "A15", "A16"
            ]
        );
        assert_eq!(
            sheet
                .cells
                .iter()
                .map(|cell| (cell.x, cell.y))
                .collect::<Vec<_>>(),
            [
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2),
                (0, 3),
                (1, 3),
                (2, 3),
                (3, 3)
            ]
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn solid_metadata_defaults_false_and_round_trips_by_stable_cell_identity() {
        let project = project();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        );
        let before = inspect_tilesheets(&project).unwrap();
        assert!(before.tilesheets[0].cells.iter().all(|cell| !cell.solid));

        set_tile_solid(&project, "A2", true).unwrap();
        let solid = inspect_tilesheets(&project).unwrap();
        assert_eq!(
            solid.tilesheets[0]
                .cells
                .iter()
                .map(|cell| (cell.id.as_str(), cell.solid))
                .collect::<Vec<_>>(),
            [("A1", false), ("A2", true), ("A3", false), ("A4", false)]
        );
        assert!(
            fs::read_to_string(project.join(REGISTRY_FILE))
                .unwrap()
                .contains("solid =")
        );

        let change = apply_tilesheet_name(&project, "A").unwrap();
        assert_eq!(change.group, "A");
        let renamed = inspect_tilesheets(&project).unwrap();
        assert_eq!(renamed.tilesheets[0].file, "sheet (A).png");
        assert!(
            renamed.tilesheets[0].cells[1].solid,
            "canonical rename retains A2 metadata"
        );
        set_tile_solid(&project, "A2", false).unwrap();
        assert!(!inspect_tilesheets(&project).unwrap().tilesheets[0].cells[1].solid);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn solid_metadata_rejects_cell_ids_outside_the_registered_sheet() {
        let project = project();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        );
        let error = set_tile_solid(&project, "A99", true)
            .unwrap_err()
            .to_string();
        assert!(error.contains("A99"));
        assert!(error.contains("out of range"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn explicit_add_skips_active_and_retired_groups_and_names_the_png() {
        let project = project();
        fs::copy(project.join("sheet.png"), project.join("art.png")).unwrap();
        write_registry(
            &project,
            "version = 1\nretired_groups = [\"B\"]\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        );

        let change = add_tilesheet(&project, &project.join("art.png")).unwrap();
        assert_eq!(change.group, "C");
        assert_eq!(change.file, "art (C).png");
        assert!(!project.join("art.png").exists());
        assert!(project.join("art (C).png").is_file());
        let report = inspect_tilesheets(&project).unwrap();
        assert_eq!(
            report
                .tilesheets
                .iter()
                .map(|sheet| sheet.group.as_str())
                .collect::<Vec<_>>(),
            ["A", "C"]
        );
        assert_eq!(report.tilesheets[1].cells[0].id, "C1");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn explicit_add_collision_leaves_png_and_registry_unchanged() {
        let project = project();
        fs::copy(project.join("sheet.png"), project.join("art.png")).unwrap();
        fs::copy(project.join("sheet.png"), project.join("art (A).png")).unwrap();
        write_registry(&project, "version = 1\n");

        let error = add_tilesheet(&project, &project.join("art.png"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("refusing to overwrite"));
        assert!(project.join("art.png").is_file());
        assert!(project.join("art (A).png").is_file());
        assert_eq!(
            fs::read_to_string(project.join(REGISTRY_FILE)).unwrap(),
            "version = 1\n"
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn canonical_name_migration_preserves_existing_group_and_cell_ids() {
        let project = project();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        );
        let before = inspect_tilesheets(&project).unwrap();

        let change = apply_tilesheet_name(&project, "A").unwrap();
        assert_eq!(change.group, "A");
        assert_eq!(change.file, "sheet (A).png");
        assert!(!project.join("sheet.png").exists());
        let after = inspect_tilesheets(&project).unwrap();
        assert_eq!(after.tilesheets[0].group, before.tilesheets[0].group);
        assert_eq!(
            after.tilesheets[0].cells[0].id,
            before.tilesheets[0].cells[0].id
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn canonical_name_collision_preserves_registered_path_and_group() {
        let project = project();
        fs::copy(project.join("sheet.png"), project.join("sheet (A).png")).unwrap();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        );

        let error = apply_tilesheet_name(&project, "A").unwrap_err().to_string();
        assert!(error.contains("refusing to overwrite"));
        assert!(project.join("sheet.png").is_file());
        assert!(project.join("sheet (A).png").is_file());
        assert_eq!(
            inspect_tilesheets(&project).unwrap().tilesheets[0].file,
            "sheet.png"
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn group_ids_continue_after_z_without_filename_sorting() {
        assert_eq!(group_id(0), "A");
        assert_eq!(group_id(25), "Z");
        assert_eq!(group_id(26), "AA");
        assert_eq!(group_id(27), "AB");
    }

    #[test]
    fn later_groups_keep_their_ids_when_an_earlier_group_is_retired() {
        let project = project();
        fs::copy(project.join("sheet.png"), project.join("a.png")).unwrap();
        fs::copy(project.join("sheet.png"), project.join("c.png")).unwrap();
        write_registry(
            &project,
            "version = 1\nretired_groups = [\"B\"]\n\n[[tilesheets]]\ngroup = \"C\"\nfile = \"a.png\"\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"c.png\"\n",
        );
        let report = inspect_tilesheets(&project).unwrap();

        assert_eq!(report.tilesheets[0].group, "C");
        assert_eq!(report.tilesheets[0].cells[0].id, "C1");
        assert_eq!(report.tilesheets[1].group, "A");
        assert_eq!(report.tilesheets[1].cells[0].id, "A1");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn only_explicitly_registered_png_files_are_inspected() {
        let project = project();
        fs::copy(project.join("sheet.png"), project.join("unregistered.png")).unwrap();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"C\"\nfile = \"sheet.png\"\n",
        );
        let report = inspect_tilesheets(&project).unwrap();

        assert_eq!(report.tilesheets.len(), 1);
        assert_eq!(report.tilesheets[0].group, "C");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn renaming_a_sheet_keeps_the_group_and_public_cell_ids() {
        let project = project();
        let manifest = |file: &str| {
            format!("version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"{file}\"\n")
        };
        write_registry(&project, &manifest("sheet.png"));
        let before = inspect_tilesheets(&project).unwrap();
        fs::rename(project.join("sheet.png"), project.join("renamed.png")).unwrap();
        write_registry(&project, &manifest("renamed.png"));
        let after = inspect_tilesheets(&project).unwrap();

        assert_eq!(before.tilesheets[0].group, after.tilesheets[0].group);
        assert_eq!(
            before.tilesheets[0].cells[0].id,
            after.tilesheets[0].cells[0].id
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn retired_groups_cannot_be_reused() {
        let project = project();
        write_registry(
            &project,
            "version = 1\nretired_groups = [\"B\"]\n\n[[tilesheets]]\ngroup = \"B\"\nfile = \"sheet.png\"\n",
        );

        let error = inspect_tilesheets(&project).unwrap_err().to_string();
        assert!(error.contains("group 'B' is retired"));
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn no_registry_means_no_registered_assets() {
        let project = project();
        let report = inspect_tilesheets(&project).unwrap();
        assert!(report.tilesheets.is_empty());
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn registry_rejects_duplicate_groups_and_paths_outside_project() {
        let project = project();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"sheet.png\"\n",
        );
        assert!(
            inspect_tilesheets(&project)
                .unwrap_err()
                .to_string()
                .contains("registered more than once")
        );

        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"../outside.png\"\n",
        );
        assert!(
            inspect_tilesheets(&project)
                .unwrap_err()
                .to_string()
                .contains("relative PNG path")
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn registered_tilesheets_reject_png_dimensions_not_divisible_by_eight() {
        let project = project();
        let image = image::RgbaImage::new(9, 8);
        image::DynamicImage::ImageRgba8(image)
            .save_with_format(project.join("invalid.png"), image::ImageFormat::Png)
            .unwrap();
        write_registry(
            &project,
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"invalid.png\"\n",
        );

        let error = inspect_tilesheets(&project).unwrap_err().to_string();
        assert!(error.contains("invalid.png"), "{error}");
        assert!(error.contains("9x8"), "{error}");
        assert!(error.contains("nonzero multiples of 8"), "{error}");
        fs::remove_dir_all(project).unwrap();
    }
}
