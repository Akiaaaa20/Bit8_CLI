use bit8::framebuffer::{HEIGHT, WIDTH};
use bit8::input::Button;
use bit8::runtime_session::RuntimeSession;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostCommand {
    Step {
        buttons: Vec<String>,
        elapsed_ns: Option<u64>,
    },
    Stop,
}

/// Runs the line-oriented host protocol. Returns `true` after a clean stop or
/// EOF, and `false` after project/runtime failure. Invalid command lines are
/// recoverable: they emit an error and the host continues without stepping.
pub fn serve(
    project: impl AsRef<Path>,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<bool> {
    let mut session = match RuntimeSession::start(project) {
        Ok(session) => session,
        Err(error) => {
            write_message(
                &mut output,
                &json!({"type": "error", "message": error.to_string()}),
            )?;
            return Ok(false);
        }
    };

    write_message(
        &mut output,
        &json!({"type": "ready", "width": WIDTH, "height": HEIGHT}),
    )?;

    let mut sequence = 0_u64;
    let mut previous_step = Instant::now();
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            write_message(&mut output, &json!({"type": "exit"}))?;
            return Ok(true);
        }

        let command = match serde_json::from_str::<HostCommand>(&line) {
            Ok(command) => command,
            Err(error) => {
                write_message(
                    &mut output,
                    &json!({"type": "error", "message": format!("invalid command: {error}")}),
                )?;
                continue;
            }
        };

        match command {
            HostCommand::Step {
                buttons,
                elapsed_ns,
            } => {
                let button_mask = match parse_buttons(&buttons) {
                    Ok(button_mask) => button_mask,
                    Err(message) => {
                        write_message(&mut output, &json!({"type": "error", "message": message}))?;
                        continue;
                    }
                };
                let now = Instant::now();
                let elapsed = elapsed_ns
                    .map(Duration::from_nanos)
                    .unwrap_or_else(|| now.duration_since(previous_step));
                previous_step = now;
                session.set_buttons(button_mask);
                let frame = match session.advance(elapsed) {
                    Ok(frame) => frame,
                    Err(error) => {
                        write_message(
                            &mut output,
                            &json!({"type": "error", "message": error.to_string()}),
                        )?;
                        return Ok(false);
                    }
                };
                let next_sequence = sequence + 1;
                write_message(
                    &mut output,
                    &json!({
                        "type": "frame",
                        "seq": next_sequence,
                        "width": WIDTH,
                        "height": HEIGHT,
                        "pixels": frame.pixels(),
                    }),
                )?;
                sequence = next_sequence;
            }
            HostCommand::Stop => {
                write_message(&mut output, &json!({"type": "exit"}))?;
                return Ok(true);
            }
        }
    }
}

fn parse_buttons(buttons: &[String]) -> Result<u8, String> {
    buttons.iter().try_fold(0, |mask, name| {
        let button = match name.as_str() {
            "UP" => Button::Up,
            "DOWN" => Button::Down,
            "LEFT" => Button::Left,
            "RIGHT" => Button::Right,
            "A" => Button::A,
            "B" => Button::B,
            _ => return Err(format!("unknown Bit8 button '{name}'")),
        };
        Ok(mask | button.mask())
    })
}

fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, message).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}
