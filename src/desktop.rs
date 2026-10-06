use bit8::framebuffer::{HEIGHT, WIDTH};
use bit8::input::Button;
use bit8::runtime_session::RuntimeSession;
use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use std::time::Instant;

const WINDOW_SCALE: Scale = Scale::X8;

pub fn run(session: &mut RuntimeSession) -> Result<(), Box<dyn std::error::Error>> {
    let mut window = Window::new(
        concat!("Bit8 ", env!("CARGO_PKG_VERSION")),
        WIDTH,
        HEIGHT,
        WindowOptions {
            resize: false,
            scale: WINDOW_SCALE,
            scale_mode: ScaleMode::Stretch,
            ..WindowOptions::default()
        },
    )?;

    window.set_target_fps(60);
    let mut previous_frame = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mut button_mask = 0;
        if window.is_key_down(Key::Up) {
            button_mask |= Button::Up.mask();
        }
        if window.is_key_down(Key::Down) {
            button_mask |= Button::Down.mask();
        }
        if window.is_key_down(Key::Left) {
            button_mask |= Button::Left.mask();
        }
        if window.is_key_down(Key::Right) {
            button_mask |= Button::Right.mask();
        }
        if window.is_key_down(Key::Z) {
            button_mask |= Button::A.mask();
        }
        if window.is_key_down(Key::X) {
            button_mask |= Button::B.mask();
        }
        let now = Instant::now();
        let elapsed = now.duration_since(previous_frame);
        previous_frame = now;
        session.set_buttons(button_mask);
        let framebuffer = session.advance(elapsed)?;
        window.update_with_buffer(framebuffer.pixels(), WIDTH, HEIGHT)?;
    }

    Ok(())
}
