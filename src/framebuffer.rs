use crate::palette::PALETTE;

pub const WIDTH: usize = 64;
pub const HEIGHT: usize = 64;
pub const SPRITE_SIZE: usize = 8;
pub const TRANSPARENT_COLOR: u8 = 0;
const PIXEL_COUNT: usize = WIDTH * HEIGHT;

/// Bit8's fixed-size internal display.
pub struct Framebuffer {
    pixels: [u32; PIXEL_COUNT],
}

impl Framebuffer {
    pub fn new() -> Self {
        Self {
            pixels: [0; PIXEL_COUNT],
        }
    }

    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < WIDTH && y < HEIGHT {
            self.pixels[y * WIDTH + x] = color;
        }
    }

    pub fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, color: u32) {
        let Some((mut x0, mut y0, x1, y1)) =
            clip_line(x0 as i128, y0 as i128, x1 as i128, y1 as i128)
        else {
            return;
        };

        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            self.set_pixel_clipped(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }

            let twice_error = 2 * error;
            if twice_error >= dy {
                error += dy;
                x0 += sx;
            }
            if twice_error <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    /// Draws an outline whose width and height are pixel counts.
    pub fn rect(&mut self, x: i64, y: i64, width: i64, height: i64, color: u32) {
        if width <= 0 || height <= 0 {
            return;
        }

        let left = x as i128;
        let top = y as i128;
        let right = left + width as i128 - 1;
        let bottom = top + height as i128 - 1;
        self.draw_horizontal_span(top, left, right, color);
        if bottom != top {
            self.draw_horizontal_span(bottom, left, right, color);
        }
        self.draw_vertical_span(left, top + 1, bottom - 1, color);
        if right != left {
            self.draw_vertical_span(right, top + 1, bottom - 1, color);
        }
    }

    /// Draws a filled rectangle whose width and height are pixel counts.
    pub fn rectfill(&mut self, x: i64, y: i64, width: i64, height: i64, color: u32) {
        if width <= 0 || height <= 0 {
            return;
        }

        let left = (x as i128).max(0);
        let top = (y as i128).max(0);
        let right = (x as i128 + width as i128).min(WIDTH as i128);
        let bottom = (y as i128 + height as i128).min(HEIGHT as i128);
        for screen_y in top..bottom {
            for screen_x in left..right {
                self.set_pixel_clipped(screen_x, screen_y, color);
            }
        }
    }

    /// Draws an integer-radius circle centered at (x, y).
    pub fn circ(&mut self, x: i64, y: i64, radius: i64, color: u32) {
        if radius < 0 {
            return;
        }

        self.for_each_circle_row(y, radius, |framebuffer, screen_y, x_radius| {
            framebuffer.set_pixel_clipped(x as i128 - x_radius, screen_y, color);
            framebuffer.set_pixel_clipped(x as i128 + x_radius, screen_y, color);
        });
    }

    /// Draws a filled integer-radius circle centered at (x, y).
    pub fn circfill(&mut self, x: i64, y: i64, radius: i64, color: u32) {
        if radius < 0 {
            return;
        }

        self.for_each_circle_row(y, radius, |framebuffer, screen_y, x_radius| {
            framebuffer.draw_horizontal_span(
                screen_y,
                x as i128 - x_radius,
                x as i128 + x_radius,
                color,
            );
        });
    }

    fn for_each_circle_row(
        &mut self,
        y: i64,
        radius: i64,
        mut draw_row: impl FnMut(&mut Self, i128, i128),
    ) {
        let radius = radius as u128;
        let radius_squared = radius * radius;
        let first_y = (y as i128 - radius as i128).max(0);
        let last_y = (y as i128 + radius as i128).min(HEIGHT as i128 - 1);

        for screen_y in first_y..=last_y {
            let y_distance = (screen_y - y as i128).unsigned_abs();
            let x_radius = integer_sqrt(radius_squared - y_distance * y_distance) as i128;
            draw_row(self, screen_y, x_radius);
        }
    }

    fn draw_horizontal_span(&mut self, y: i128, left: i128, right: i128, color: u32) {
        if !(0..HEIGHT as i128).contains(&y) {
            return;
        }
        let left = left.max(0);
        let right = right.min(WIDTH as i128 - 1);
        for x in left..=right {
            self.set_pixel_clipped(x, y, color);
        }
    }

    fn draw_vertical_span(&mut self, x: i128, top: i128, bottom: i128, color: u32) {
        if !(0..WIDTH as i128).contains(&x) {
            return;
        }
        let top = top.max(0);
        let bottom = bottom.min(HEIGHT as i128 - 1);
        for y in top..=bottom {
            self.set_pixel_clipped(x, y, color);
        }
    }

    fn set_pixel_clipped(&mut self, x: i128, y: i128, color: u32) {
        if (0..WIDTH as i128).contains(&x) && (0..HEIGHT as i128).contains(&y) {
            self.set_pixel(x as usize, y as usize, color);
        }
    }

    pub fn draw_sprite(&mut self, sprite: &[u8; SPRITE_SIZE * SPRITE_SIZE], x: i64, y: i64) {
        for sprite_y in 0..SPRITE_SIZE {
            for sprite_x in 0..SPRITE_SIZE {
                let color = sprite[sprite_y * SPRITE_SIZE + sprite_x];
                if color == TRANSPARENT_COLOR {
                    continue;
                }

                let screen_x = x.saturating_add(sprite_x as i64);
                let screen_y = y.saturating_add(sprite_y as i64);
                if screen_x >= 0
                    && screen_y >= 0
                    && screen_x < WIDTH as i64
                    && screen_y < HEIGHT as i64
                    && let Some(color) = PALETTE.get(color as usize)
                {
                    self.set_pixel(screen_x as usize, screen_y as usize, *color);
                }
            }
        }
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }
}

fn clip_line(
    mut x0: i128,
    mut y0: i128,
    mut x1: i128,
    mut y1: i128,
) -> Option<(i128, i128, i128, i128)> {
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const TOP: u8 = 4;
    const BOTTOM: u8 = 8;

    fn out_code(x: i128, y: i128) -> u8 {
        let mut code = 0;
        if x < 0 {
            code |= LEFT;
        } else if x >= WIDTH as i128 {
            code |= RIGHT;
        }
        if y < 0 {
            code |= TOP;
        } else if y >= HEIGHT as i128 {
            code |= BOTTOM;
        }
        code
    }

    loop {
        let code0 = out_code(x0, y0);
        let code1 = out_code(x1, y1);
        if code0 | code1 == 0 {
            return Some((x0, y0, x1, y1));
        }
        if code0 & code1 != 0 {
            return None;
        }

        let code = if code0 != 0 { code0 } else { code1 };
        let (x, y) = if code & TOP != 0 {
            if y1 == y0 {
                return None;
            }
            (x0 + (x1 - x0) * (-(y0)) / (y1 - y0), 0)
        } else if code & BOTTOM != 0 {
            if y1 == y0 {
                return None;
            }
            let y = HEIGHT as i128 - 1;
            (x0 + (x1 - x0) * (y - y0) / (y1 - y0), y)
        } else if code & RIGHT != 0 {
            if x1 == x0 {
                return None;
            }
            let x = WIDTH as i128 - 1;
            (x, y0 + (y1 - y0) * (x - x0) / (x1 - x0))
        } else {
            if x1 == x0 {
                return None;
            }
            (0, y0 + (y1 - y0) * (-(x0)) / (x1 - x0))
        };

        if code == code0 {
            x0 = x;
            y0 = y;
        } else {
            x1 = x;
            y1 = y;
        }
    }
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }

    let mut estimate = value;
    let mut next = estimate.div_ceil(2);
    while next < estimate {
        estimate = next;
        next = (estimate + value / estimate) / 2;
    }
    estimate
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_is_always_64_by_64() {
        let framebuffer = Framebuffer::new();

        assert_eq!(framebuffer.pixels().len(), 64 * 64);
    }

    #[test]
    fn clear_sets_every_pixel() {
        let mut framebuffer = Framebuffer::new();

        framebuffer.clear(0x12_34_56);

        assert!(
            framebuffer
                .pixels()
                .iter()
                .all(|pixel| *pixel == 0x12_34_56)
        );
    }

    #[test]
    fn sprite_transparency_preserves_existing_pixels() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.clear(PALETTE[4]);
        let mut sprite = [TRANSPARENT_COLOR; SPRITE_SIZE * SPRITE_SIZE];
        sprite[1] = 12;

        framebuffer.draw_sprite(&sprite, 10, 20);

        assert_eq!(framebuffer.pixels()[20 * WIDTH + 10], PALETTE[4]);
        assert_eq!(framebuffer.pixels()[20 * WIDTH + 11], PALETTE[12]);
    }

    #[test]
    fn sprite_drawing_clips_at_every_framebuffer_edge() {
        let sprite = [1; SPRITE_SIZE * SPRITE_SIZE];
        let mut framebuffer = Framebuffer::new();
        framebuffer.draw_sprite(&sprite, 63, 63);
        framebuffer.draw_sprite(&sprite, -7, -7);
        framebuffer.draw_sprite(&sprite, i64::MAX, i64::MAX);

        assert_eq!(framebuffer.pixels()[63 * WIDTH + 63], PALETTE[1]);
        assert_eq!(framebuffer.pixels()[0], PALETTE[1]);
        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel != 0)
                .count(),
            2
        );
    }

    #[test]
    fn line_draws_horizontal_vertical_diagonal_and_reversed_segments() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.line(2, 4, 5, 4, PALETTE[1]);
        framebuffer.line(8, 2, 8, 5, PALETTE[2]);
        framebuffer.line(12, 2, 15, 5, PALETTE[3]);
        framebuffer.line(19, 5, 16, 2, PALETTE[4]);
        framebuffer.line(30, 1, 32, 7, PALETTE[5]);

        for x in 2..=5 {
            assert_eq!(framebuffer.pixels()[4 * WIDTH + x], PALETTE[1]);
        }
        for y in 2..=5 {
            assert_eq!(framebuffer.pixels()[y * WIDTH + 8], PALETTE[2]);
            assert_eq!(framebuffer.pixels()[y * WIDTH + (10 + y)], PALETTE[3]);
            assert_eq!(framebuffer.pixels()[y * WIDTH + (14 + y)], PALETTE[4]);
        }
        assert_eq!(framebuffer.pixels()[WIDTH + 30], PALETTE[5]);
        assert_eq!(framebuffer.pixels()[7 * WIDTH + 32], PALETTE[5]);
        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel == PALETTE[5])
                .count(),
            7
        );
    }

    #[test]
    fn line_clips_offscreen_endpoints_and_handles_extreme_coordinates() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.line(-100, 0, 100, 0, PALETTE[1]);
        for x in 0..WIDTH {
            assert_eq!(framebuffer.pixels()[x], PALETTE[1]);
        }

        framebuffer.clear(0);
        framebuffer.line(i64::MIN, i64::MIN, i64::MAX, i64::MAX, PALETTE[2]);
        for x in 0..WIDTH {
            assert_eq!(framebuffer.pixels()[x * WIDTH + x], PALETTE[2]);
        }
        framebuffer.clear(0);
        framebuffer.line(-100, -1, 100, -1, PALETTE[3]);
        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel == PALETTE[3])
                .count(),
            0
        );
    }

    #[test]
    fn rect_draws_outline_with_pixel_count_dimensions_and_clips() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.rect(2, 3, 4, 3, PALETTE[1]);
        framebuffer.rect(10, 10, 1, 1, PALETTE[2]);
        framebuffer.rect(-1, -1, 3, 3, PALETTE[3]);

        for y in 3..6 {
            for x in 2..6 {
                let border = y == 3 || y == 5 || x == 2 || x == 5;
                assert_eq!(framebuffer.pixels()[y * WIDTH + x] == PALETTE[1], border);
            }
        }
        assert_eq!(framebuffer.pixels()[10 * WIDTH + 10], PALETTE[2]);
        assert_eq!(framebuffer.pixels()[0], 0);
        assert_eq!(framebuffer.pixels()[WIDTH], PALETTE[3]);
        assert_eq!(framebuffer.pixels()[WIDTH + 1], PALETTE[3]);
    }

    #[test]
    fn rect_ignores_zero_or_negative_dimensions() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.rect(5, 5, 0, 4, PALETTE[1]);
        framebuffer.rect(5, 5, 4, 0, PALETTE[1]);
        framebuffer.rect(5, 5, -1, 4, PALETTE[1]);
        assert!(framebuffer.pixels().iter().all(|pixel| *pixel == 0));
    }

    #[test]
    fn rectfill_draws_the_requested_area_and_clips() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.rectfill(2, 3, 4, 3, PALETTE[1]);
        framebuffer.rectfill(10, 10, 1, 1, PALETTE[2]);
        framebuffer.rectfill(-1, 20, 3, 2, PALETTE[3]);

        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel == PALETTE[1])
                .count(),
            12
        );
        assert_eq!(framebuffer.pixels()[10 * WIDTH + 10], PALETTE[2]);
        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel == PALETTE[3])
                .count(),
            4
        );
    }

    #[test]
    fn rectfill_ignores_zero_or_negative_dimensions() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.rectfill(5, 5, 0, 4, PALETTE[1]);
        framebuffer.rectfill(5, 5, 4, 0, PALETTE[1]);
        framebuffer.rectfill(5, 5, 4, -1, PALETTE[1]);
        assert!(framebuffer.pixels().iter().all(|pixel| *pixel == 0));
    }

    #[test]
    fn circ_draws_a_center_pixel_and_integer_circle_outline() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.circ(4, 4, 0, PALETTE[1]);
        framebuffer.circ(10, 10, 2, PALETTE[2]);

        assert_eq!(framebuffer.pixels()[4 * WIDTH + 4], PALETTE[1]);
        for (x, y) in [
            (10, 8),
            (9, 9),
            (11, 9),
            (8, 10),
            (12, 10),
            (9, 11),
            (11, 11),
            (10, 12),
        ] {
            assert_eq!(framebuffer.pixels()[y * WIDTH + x], PALETTE[2]);
        }
        assert_eq!(framebuffer.pixels()[10 * WIDTH + 10], 0);
    }

    #[test]
    fn circ_clips_at_edges_and_ignores_negative_radius() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.circ(0, 0, 2, PALETTE[1]);
        framebuffer.circ(20, 20, -1, PALETTE[2]);
        framebuffer.circfill(30, 30, -1, PALETTE[3]);

        assert_eq!(framebuffer.pixels()[2], PALETTE[1]);
        assert_eq!(framebuffer.pixels()[WIDTH + 1], PALETTE[1]);
        assert_eq!(framebuffer.pixels()[2 * WIDTH], PALETTE[1]);
        assert!(
            framebuffer
                .pixels()
                .iter()
                .all(|pixel| *pixel != PALETTE[2])
        );
        assert!(
            framebuffer
                .pixels()
                .iter()
                .all(|pixel| *pixel != PALETTE[3])
        );
    }

    #[test]
    fn circfill_fills_circle_interior_and_clips() {
        let mut framebuffer = Framebuffer::new();
        framebuffer.circfill(10, 10, 0, PALETTE[1]);
        framebuffer.circfill(20, 20, 2, PALETTE[2]);
        framebuffer.circfill(0, 40, 2, PALETTE[3]);

        assert_eq!(framebuffer.pixels()[10 * WIDTH + 10], PALETTE[1]);
        assert_eq!(framebuffer.pixels()[20 * WIDTH + 20], PALETTE[2]);
        assert_eq!(framebuffer.pixels()[20 * WIDTH + 22], PALETTE[2]);
        assert_eq!(framebuffer.pixels()[19 * WIDTH + 20], PALETTE[2]);
        assert_eq!(framebuffer.pixels()[40 * WIDTH], PALETTE[3]);
        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel == PALETTE[1])
                .count(),
            1
        );
        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel == PALETTE[2])
                .count(),
            13
        );
        assert_eq!(
            framebuffer
                .pixels()
                .iter()
                .filter(|pixel| **pixel == PALETTE[3])
                .count(),
            9
        );
    }
}
