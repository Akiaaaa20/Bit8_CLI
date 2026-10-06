use bit8::input::Button;
use bit8::runtime_session::RuntimeSession;
use bit8::runtime_timing::{FIXED_DT, FIXED_HZ, MAX_CATCH_UP_TICKS};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = "ticks=0 presses=0 held_ticks=0 held=false\nfunc update() ticks=ticks+1 held=btn(A) if held then held_ticks=held_ticks+1 end if btnp(A) then presses=presses+1 end end\nfunc draw() cls(0) pix(ticks,0,1) pix(presses,1,2) pix(held_ticks,2,3) pix(held and 1 or 0,3,4) end";

struct Project(PathBuf);
impl Project {
    fn new(source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bit8-fixed-tick-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("main.b8"), source).unwrap();
        Self(path)
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

fn counts(session: &RuntimeSession) -> [usize; 4] {
    let frame = session.framebuffer();
    std::array::from_fn(|row| {
        frame.pixels()[row * 64..(row + 1) * 64]
            .iter()
            .position(|color| *color != 0)
            .unwrap()
    })
}

#[test]
fn less_exact_and_two_fixed_intervals_are_wired_into_lua_updates() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    session.advance(FIXED_DT - Duration::from_nanos(1)).unwrap();
    assert_eq!(counts(&session)[0], 0);
    session.advance(Duration::from_nanos(1)).unwrap();
    assert_eq!(counts(&session)[0], 1);
    session.advance(FIXED_DT * 2).unwrap();
    assert_eq!(counts(&session)[0], 3);
}

#[test]
fn fractional_remainder_survives_irregular_advances() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    session.advance(Duration::from_millis(70)).unwrap();
    assert_eq!(counts(&session)[0], 2);
    let remainder = Duration::from_millis(70) - FIXED_DT * 2;
    session
        .advance(FIXED_DT - remainder - Duration::from_nanos(1))
        .unwrap();
    assert_eq!(counts(&session)[0], 2);
    session.advance(Duration::from_nanos(1)).unwrap();
    assert_eq!(counts(&session)[0], 3);
}

fn cadence(hz: u64) -> Vec<Duration> {
    // Explicit durations sum to exactly one second, avoiding rounding in tests.
    (1..=hz)
        .map(|i| Duration::from_nanos(i * 1_000_000_000 / hz - (i - 1) * 1_000_000_000 / hz))
        .collect()
}

#[test]
fn thirty_sixty_one_twenty_and_irregular_frontends_produce_same_second() {
    let irregular = [7, 40, 23, 80, 50]
        .into_iter()
        .cycle()
        .take(25)
        .map(Duration::from_millis)
        .collect();
    for chunks in [cadence(30), cadence(60), cadence(120), irregular] {
        let project = Project::new(SOURCE);
        let mut session = project.start();
        for elapsed in chunks {
            session.advance(elapsed).unwrap();
        }
        assert_eq!(counts(&session)[0], FIXED_HZ as usize);
    }
}

#[test]
fn long_stall_caps_ticks_discards_old_debt_and_preserves_only_fraction() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    session
        .advance(FIXED_DT * 150 + Duration::from_nanos(7))
        .unwrap();
    assert_eq!(counts(&session)[0], MAX_CATCH_UP_TICKS);
    session.advance(Duration::ZERO).unwrap();
    assert_eq!(counts(&session)[0], 5);
    session.advance(FIXED_DT - Duration::from_nanos(7)).unwrap();
    assert_eq!(counts(&session)[0], 6);
}

#[test]
fn duration_max_does_not_overflow_or_spiral() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    session.advance(Duration::MAX).unwrap();
    assert_eq!(counts(&session)[0], 5);
    session.advance(Duration::ZERO).unwrap();
    assert_eq!(counts(&session)[0], 5);
}

#[test]
fn init_and_repeated_draws_consume_no_ticks() {
    let project = Project::new(&format!("{SOURCE}\nfunc init() assert(ticks==0) end"));
    let mut session = project.start();
    session.advance(Duration::ZERO).unwrap();
    for _ in 0..10 {
        session.draw().unwrap();
    }
    assert_eq!(counts(&session)[0], 0);
    session.advance(FIXED_DT).unwrap();
    for _ in 0..10 {
        session.draw().unwrap();
    }
    assert_eq!(counts(&session)[0], 1);
}

#[test]
fn press_between_ticks_survives_draw_and_is_consumed_only_once() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    session.set_buttons(Button::A.mask());
    session.advance(FIXED_DT / 2).unwrap();
    session.draw().unwrap();
    assert_eq!(counts(&session), [0, 0, 0, 0]);
    session.advance(FIXED_DT - FIXED_DT / 2).unwrap();
    assert_eq!(counts(&session), [1, 1, 1, 1]);
    session.advance(FIXED_DT).unwrap();
    assert_eq!(counts(&session), [2, 1, 2, 1]);
    session.set_buttons(0);
    session.advance(FIXED_DT).unwrap();
    assert_eq!(counts(&session), [3, 1, 2, 0]);
}

#[test]
fn short_tap_survives_release_and_drawing_but_held_is_false() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    session.set_buttons(Button::A.mask());
    session.advance(Duration::ZERO).unwrap();
    session.set_buttons(0);
    session.draw().unwrap();
    session.advance(FIXED_DT).unwrap();
    assert_eq!(counts(&session), [1, 1, 0, 0]);
    session.advance(FIXED_DT).unwrap();
    assert_eq!(counts(&session), [2, 1, 0, 0]);
}

#[test]
fn catchup_consumes_press_on_first_tick_only_and_retains_held_state() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    session.set_buttons(Button::A.mask());
    session.advance(Duration::from_millis(70)).unwrap();
    assert_eq!(counts(&session), [2, 1, 2, 1]);
    session.set_buttons(0);
    session.set_buttons(Button::A.mask());
    session.advance(FIXED_DT * 3).unwrap();
    assert_eq!(counts(&session), [5, 2, 5, 1]);
}

#[test]
fn separate_presses_and_normal_cadence_keep_existing_input_semantics() {
    let project = Project::new(SOURCE);
    let mut session = project.start();
    for buttons in [Button::A.mask(), Button::A.mask(), 0, Button::A.mask()] {
        session.step(buttons).unwrap();
    }
    assert_eq!(counts(&session), [4, 2, 3, 1]);
}

fn host(project: &Project, commands: &str) -> Vec<serde_json::Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bit8"))
        .arg("host")
        .arg(&project.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
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
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn spawned_host_thirty_sixty_and_irregular_cadences_match_session_progression() {
    let irregular = [7, 40, 23, 80, 50]
        .into_iter()
        .cycle()
        .take(25)
        .map(Duration::from_millis)
        .collect();
    for chunks in [cadence(30), cadence(60), irregular] {
        let project = Project::new(SOURCE);
        let mut commands = String::new();
        let mut session = project.start();
        for elapsed in &chunks {
            commands.push_str(&format!(
                "{{\"type\":\"step\",\"buttons\":[],\"elapsed_ns\":{}}}\n",
                elapsed.as_nanos()
            ));
            session.advance(*elapsed).unwrap();
        }
        commands.push_str("{\"type\":\"stop\"}\n");
        let messages = host(&project, &commands);
        assert_eq!(messages.len(), chunks.len() + 2);
        assert_eq!(messages[0]["type"], "ready");
        for (index, message) in messages[1..=chunks.len()].iter().enumerate() {
            assert_eq!(message["seq"], index + 1);
        }
        let pixels: Vec<u32> = messages[chunks.len()]["pixels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pixel| pixel.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(pixels, session.framebuffer().pixels());
        assert_eq!(counts(&session)[0], 30);
        assert_eq!(messages.last().unwrap()["type"], "exit");
    }
}

#[test]
fn spawned_host_short_tap_zero_time_and_catchup_preserve_input() {
    let project = Project::new(SOURCE);
    let commands = format!(
        "{{\"type\":\"step\",\"buttons\":[\"A\"],\"elapsed_ns\":0}}\n{{\"type\":\"step\",\"buttons\":[],\"elapsed_ns\":0}}\n{{\"type\":\"step\",\"buttons\":[],\"elapsed_ns\":{}}}\n{{\"type\":\"stop\"}}\n",
        (FIXED_DT * 2).as_nanos()
    );
    let messages = host(&project, &commands);
    let pixels = messages[3]["pixels"].as_array().unwrap();
    assert_eq!(pixels[2], 0x1d2b53);
    assert_eq!(pixels[65], 0x7e2553);
    assert_eq!(pixels[128], 0x008751);
    assert_eq!(pixels[192], 0xab5236);
}

#[test]
fn spawned_host_accepts_original_protocol_and_explicit_timing_validation() {
    let project = Project::new("func draw() pix(0,0,1) end");
    let messages = host(
        &project,
        "{\"type\":\"step\",\"buttons\":[]}\n{\"type\":\"step\",\"buttons\":[],\"elapsed_ns\":-1}\n{\"type\":\"step\",\"buttons\":[],\"elapsed_ns\":0}\n{\"type\":\"stop\"}\n",
    );
    assert_eq!(messages[1]["pixels"][0], 0x1d2b53);
    assert_eq!(messages[2]["type"], "error");
    assert_eq!(messages[3]["seq"], 2);
    assert_eq!(messages[4]["type"], "exit");
}
