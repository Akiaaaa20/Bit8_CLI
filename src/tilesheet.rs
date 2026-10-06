use crate::framebuffer::SPRITE_SIZE;
use crate::palette::PALETTE;
use image::ImageReader;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Error as IoError, ErrorKind, Read, Seek};
use std::path::Path;

pub struct Tilesheet {
    cells: Vec<[u8; SPRITE_SIZE * SPRITE_SIZE]>,
    columns: usize,
    rows: usize,
}

impl Tilesheet {
    pub fn empty() -> Self {
        Self {
            cells: Vec::new(),
            columns: 0,
            rows: 0,
        }
    }

    pub fn load_png(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|error| {
            IoError::new(
                error.kind(),
                format!("cannot open tilesheet '{}': {error}", path.display()),
            )
        })?;
        Self::from_reader(file).map_err(|error| {
            IoError::new(
                ErrorKind::InvalidData,
                format!("tilesheet '{}': {error}", path.display()),
            )
            .into()
        })
    }

    pub fn from_reader(reader: impl Read + Seek) -> Result<Self, Box<dyn Error>> {
        let reader = ImageReader::new(BufReader::new(reader)).with_guessed_format()?;
        let image = reader.decode()?.into_rgba8();
        let (width, height) = image.dimensions();

        if width == 0
            || height == 0
            || width % SPRITE_SIZE as u32 != 0
            || height % SPRITE_SIZE as u32 != 0
        {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                format!(
                    "PNG dimensions are {width}x{height}; both dimensions must be nonzero multiples of 8"
                ),
            )
            .into());
        }

        let cells_wide = width as usize / SPRITE_SIZE;
        let cells_high = height as usize / SPRITE_SIZE;
        let mut cells = Vec::with_capacity(cells_wide * cells_high);

        for cell_y in 0..cells_high {
            for cell_x in 0..cells_wide {
                let mut cell = [0; SPRITE_SIZE * SPRITE_SIZE];
                for y in 0..SPRITE_SIZE {
                    for x in 0..SPRITE_SIZE {
                        let pixel = image.get_pixel(
                            (cell_x * SPRITE_SIZE + x) as u32,
                            (cell_y * SPRITE_SIZE + y) as u32,
                        );
                        cell[y * SPRITE_SIZE + x] = palette_index(pixel.0);
                    }
                }
                cells.push(cell);
            }
        }

        Ok(Self {
            cells,
            columns: cells_wide,
            rows: cells_high,
        })
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (
            (self.columns * SPRITE_SIZE) as u32,
            (self.rows * SPRITE_SIZE) as u32,
        )
    }

    pub fn cell_grid(&self) -> (usize, usize) {
        (self.columns, self.rows)
    }

    pub fn cell(&self, id: i64) -> Option<&[u8; SPRITE_SIZE * SPRITE_SIZE]> {
        usize::try_from(id)
            .ok()
            .and_then(|index| self.cells.get(index))
    }
}

/// Maps opaque pixels to the nearest non-transparent palette entry by squared
/// Euclidean RGB distance. Iteration order makes ties choose the lower index.
/// Source alpha below 128 maps to transparent index 0.
fn palette_index([red, green, blue, alpha]: [u8; 4]) -> u8 {
    if alpha < 128 {
        return 0;
    }

    PALETTE
        .iter()
        .enumerate()
        .skip(1)
        .min_by_key(|(_, color)| {
            let red_distance = red as i32 - (**color >> 16) as i32;
            let green_distance = green as i32 - ((**color >> 8) & 0xff) as i32;
            let blue_distance = blue as i32 - (**color & 0xff) as i32;
            red_distance * red_distance
                + green_distance * green_distance
                + blue_distance * blue_distance
        })
        .map(|(index, _)| index as u8)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbaImage};
    use std::io::Cursor;

    fn png(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut image = RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                image.put_pixel(x, y, image::Rgba(pixel(x, y)));
            }
        }

        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    fn palette_rgba(index: usize) -> [u8; 4] {
        let color = PALETTE[index];
        [(color >> 16) as u8, (color >> 8) as u8, color as u8, 255]
    }

    fn load(bytes: &[u8]) -> Result<Tilesheet, Box<dyn Error>> {
        Tilesheet::from_reader(Cursor::new(bytes))
    }

    #[test]
    fn cell_count_matches_tilesheet_grid() {
        let bytes = png(24, 16, |_, _| palette_rgba(1));
        let sheet = load(&bytes).unwrap();

        assert_eq!(sheet.cell_count(), 6);
    }

    #[test]
    fn required_sheet_sizes_slice_to_one_two_and_sixteen_cells() {
        for (width, height, expected_count, expected_grid) in
            [(8, 8, 1, (1, 1)), (16, 8, 2, (2, 1)), (32, 32, 16, (4, 4))]
        {
            let sheet = load(&png(width, height, |_, _| palette_rgba(1))).unwrap();
            assert_eq!(sheet.cell_count(), expected_count, "{width}x{height}");
            assert_eq!(sheet.cell_grid(), expected_grid, "{width}x{height}");
            assert_eq!(sheet.dimensions(), (width, height));
        }
    }

    #[test]
    fn cells_are_numbered_left_to_right_then_top_to_bottom() {
        let bytes = png(16, 16, |x, y| {
            palette_rgba(match (x / 8, y / 8) {
                (0, 0) => 8,
                (1, 0) => 9,
                (0, 1) => 10,
                _ => 11,
            })
        });
        let sheet = load(&bytes).unwrap();

        assert_eq!(sheet.cell(0).unwrap()[0], 8);
        assert_eq!(sheet.cell(1).unwrap()[0], 9);
        assert_eq!(sheet.cell(2).unwrap()[0], 10);
        assert_eq!(sheet.cell(3).unwrap()[0], 11);
    }

    #[test]
    fn thirty_two_pixel_sheet_cells_preserve_row_major_source_rectangles() {
        let bytes = png(32, 32, |x, y| {
            let source_cell = (y / 8) * 4 + x / 8;
            palette_rgba(1 + (source_cell % 15) as usize)
        });
        let sheet = load(&bytes).unwrap();

        assert_eq!(sheet.cell_grid(), (4, 4));
        assert_eq!(sheet.cell_count(), 16);
        for source_cell in 0..16_u32 {
            let expected = (1 + (source_cell % 15)) as u8;
            let cell = sheet.cell(source_cell as i64).unwrap();
            assert!(
                cell.iter().all(|pixel| *pixel == expected),
                "cell {}",
                source_cell + 1
            );
        }
    }

    #[test]
    fn transparent_png_pixels_become_palette_index_zero() {
        let bytes = png(8, 8, |x, y| {
            if x == 3 && y == 4 {
                palette_rgba(12)
            } else {
                [0, 0, 0, 0]
            }
        });
        let sheet = load(&bytes).unwrap();
        let cell = sheet.cell(0).unwrap();

        assert_eq!(cell[0], 0);
        assert_eq!(cell[4 * SPRITE_SIZE + 3], 12);
    }

    #[test]
    fn runtime_palette_mapping_handles_exact_sky_tint_and_deterministic_ties() {
        assert_eq!(
            palette_index(palette_rgba(12)),
            12,
            "exact palette colors stay exact"
        );
        assert_eq!(
            palette_index([70, 130, 215, 255]),
            12,
            "off-palette sky-blue is mapped to Runtime cyan-blue"
        );
        assert_eq!(
            palette_index([0, 0, 0, 127]),
            0,
            "transparent source pixels use index 0"
        );
        assert_eq!(
            palette_index([62, 65, 81, 255]),
            1,
            "equal-distance ties choose the first palette entry"
        );
        assert_eq!(
            palette_index([255, 120, 160, 255]),
            14,
            "distinct off-palette pink maps independently"
        );
    }

    #[test]
    fn rejects_png_dimensions_that_are_not_multiples_of_eight() {
        for (width, height) in [(9, 8), (8, 9), (31, 32), (32, 31)] {
            let error = match load(&png(width, height, |_, _| palette_rgba(1))) {
                Ok(_) => panic!("invalid {width}x{height} PNG should be rejected"),
                Err(error) => error.to_string(),
            };
            assert!(error.contains(&format!("{width}x{height}")), "{error}");
            assert!(error.contains("nonzero multiples of 8"), "{error}");
        }
    }

    #[test]
    fn sprite_lookup_rejects_ids_outside_the_sheet() {
        let bytes = png(16, 8, |x, _| palette_rgba(1 + (x / 8) as usize));
        let sheet = load(&bytes).unwrap();

        assert!(sheet.cell(0).is_some());
        assert!(sheet.cell(1).is_some());
        assert_ne!(sheet.cell(0), sheet.cell(1));
        assert!(sheet.cell(2).is_none());
        assert!(sheet.cell(-1).is_none());
    }

    #[test]
    fn example_tilesheet_has_four_cells() {
        let sheet = load(include_bytes!("../games/tilesheet_demo/tilesheet (A).png")).unwrap();

        assert_eq!(sheet.cell_count(), 4);
    }
}
