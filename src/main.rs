mod desktop;
mod host;

use bit8::map::{Bit8Map, MapInspectionReport};
use bit8::project_assets::{self, InspectionReport};
use bit8::runtime_session::RuntimeSession;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = concat!(
    "Bit8 ",
    env!("CARGO_PKG_VERSION"),
    "\nA tiny Lua game framework for a 64x64 game.\n\n",
    "Usage:\n    bit8 run <project>\n    bit8 host <project>\n    bit8 inspect tilesheets <project> [--json]\n    bit8 inspect map <project> <map-file> [--json]\n    bit8 tilesheet add <project> <png> [--json]\n    bit8 tilesheet name <project> <group> [--json]\n\n",
    "Commands:\n    run        Run a Bit8 project in a desktop window\n    host       Run a Bit8 project over stdin/stdout NDJSON\n    inspect    Inspect registered project data\n    tilesheet  Register, name, and set tile metadata\n\n",
    "    bit8 tilesheet solid <project> <tile-id> <true|false>\n",
    "Options:\n    -h, --help       Show help\n    -V, --version    Show version\n"
);

#[derive(Debug)]
enum Command {
    Help,
    Version,
    Run(PathBuf),
    Host(PathBuf),
    InspectTilesheets {
        project: PathBuf,
        json: bool,
    },
    InspectMap {
        project: PathBuf,
        map_file: PathBuf,
        json: bool,
    },
    AddTilesheet {
        project: PathBuf,
        png: PathBuf,
        json: bool,
    },
    NameTilesheet {
        project: PathBuf,
        group: String,
        json: bool,
    },
    SetTileSolid {
        project: PathBuf,
        id: String,
        solid: bool,
    },
}

fn main() -> ExitCode {
    match parse_command(env::args_os().skip(1)) {
        Ok(Command::Help) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("Bit8 {VERSION}");
            ExitCode::SUCCESS
        }
        Ok(Command::Run(project_path)) => match run(project_path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("bit8: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(Command::Host(project_path)) => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let output = std::io::BufWriter::new(stdout.lock());
            match host::serve(project_path, stdin.lock(), output) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(error) => {
                    eprintln!("bit8 host: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Ok(Command::InspectTilesheets { project, json }) => {
            match inspect_tilesheets(&project, json) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("bit8: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Ok(Command::InspectMap {
            project,
            map_file,
            json,
        }) => match inspect_map(&project, &map_file, json) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("bit8: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(Command::AddTilesheet { project, png, json }) => {
            match project_assets::add_tilesheet(&project, &png) {
                Ok(change) => print_tilesheet_change(&change, json),
                Err(error) => {
                    eprintln!("bit8: {error}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Ok(Command::NameTilesheet {
            project,
            group,
            json,
        }) => {
            match project_assets::apply_tilesheet_name(&project, &group) {
                Ok(change) => print_tilesheet_change(&change, json),
                Err(error) => {
                    eprintln!("bit8: {error}");
                    return ExitCode::FAILURE;
                }
            }
            ExitCode::SUCCESS
        }
        Ok(Command::SetTileSolid { project, id, solid }) => {
            match project_assets::set_tile_solid(&project, &id, solid) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("bit8: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(error) => {
            eprintln!("bit8: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(project_path: PathBuf) -> Result<(), Box<dyn Error>> {
    let mut session = RuntimeSession::start(project_path)?;
    desktop::run(&mut session)?;
    Ok(())
}

fn inspect_tilesheets(project: &std::path::Path, json: bool) -> Result<(), Box<dyn Error>> {
    let report = project_assets::inspect_tilesheets(project)?;
    if json {
        let sprites =
            bit8::sprite_definitions::SpriteRegistry::load_project(project)?.unwrap_or_default();
        let mut value = serde_json::to_value(&report)?;
        value["sprites"] = serde_json::to_value(sprites.inspection())?;
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        print_tilesheets(&report);
    }
    Ok(())
}

fn print_tilesheets(report: &InspectionReport) {
    if report.tilesheets.is_empty() {
        println!(
            "No registered tilesheets. Add entries to {}.",
            project_assets::REGISTRY_FILE
        );
        return;
    }

    println!("GROUP  FILE  SIZE  CELLS");
    for sheet in &report.tilesheets {
        let ids = sheet
            .cells
            .iter()
            .map(|cell| cell.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "{}  {}  {}x{}  {}",
            sheet.group, sheet.file, sheet.width, sheet.height, ids
        );
    }
}

fn inspect_map(
    project: &std::path::Path,
    map_file: &std::path::Path,
    json: bool,
) -> Result<(), Box<dyn Error>> {
    let map = Bit8Map::load(map_file, project)?;
    let report = map.inspection_report();
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_map(&report);
    }
    Ok(())
}

fn print_map(report: &MapInspectionReport) {
    println!("Bit8 Map");
    println!("Size: {} × {} tiles", report.width, report.height);
    println!("Cells: {}", report.cells);
    println!("Used assets:");
    if report.used_assets.is_empty() {
        println!("  (none)");
    } else {
        for asset in &report.used_assets {
            println!("  {asset}");
        }
    }
}

fn print_tilesheet_change(change: &project_assets::TilesheetChange, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({"group": change.group, "file": change.file})
        );
    } else {
        println!(
            "Registered tilesheet {} as group {}",
            change.file, change.group
        );
    }
}

fn parse_command(args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let mut args = args;
    let Some(command) = args.next() else {
        return Err(cli_error("no command provided"));
    };

    if command == "--help" || command == "-h" {
        return if args.next().is_none() {
            Ok(Command::Help)
        } else {
            Err(cli_error("help does not accept arguments"))
        };
    }
    if command == "--version" || command == "-V" {
        return if args.next().is_none() {
            Ok(Command::Version)
        } else {
            Err(cli_error("version does not accept arguments"))
        };
    }
    if command == "inspect" {
        return parse_inspect_command(args);
    }
    if command == "tilesheet" {
        return parse_tilesheet_command(args);
    }
    if command == "host" {
        let Some(project_path) = args.next() else {
            return Err(cli_error("missing project path for 'host'"));
        };
        if args.next().is_some() {
            return Err(cli_error("too many arguments for 'host'"));
        }
        return Ok(Command::Host(PathBuf::from(project_path)));
    }
    if command != "run" {
        let command = command.to_string_lossy();
        let message = if command.starts_with('-') {
            format!("unknown option '{command}'")
        } else {
            format!("unknown command '{command}'")
        };
        return Err(cli_error(&message));
    }

    let Some(project_path) = args.next() else {
        return Err(cli_error("missing project path"));
    };
    if args.next().is_some() {
        return Err(cli_error("too many arguments for 'run'"));
    }

    Ok(Command::Run(PathBuf::from(project_path)))
}

fn parse_tilesheet_command(mut args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let Some(action) = args.next() else {
        return Err(cli_error(
            "missing tilesheet action; expected 'add', 'name', or 'solid'",
        ));
    };
    let Some(project) = args.next() else {
        return Err(cli_error("missing project path for 'tilesheet'"));
    };
    let command = match action.to_str() {
        Some("add") => {
            let Some(png) = args.next() else {
                return Err(cli_error("missing PNG path for 'tilesheet add'"));
            };
            Command::AddTilesheet {
                project: PathBuf::from(project),
                png: PathBuf::from(png),
                json: false,
            }
        }
        Some("name") => {
            let Some(group) = args.next() else {
                return Err(cli_error("missing group ID for 'tilesheet name'"));
            };
            Command::NameTilesheet {
                project: PathBuf::from(project),
                group: group.to_string_lossy().into_owned(),
                json: false,
            }
        }
        Some("solid") => {
            let Some(id) = args.next() else {
                return Err(cli_error("missing tile ID for 'tilesheet solid'"));
            };
            let Some(value) = args.next() else {
                return Err(cli_error("missing true/false value for 'tilesheet solid'"));
            };
            let solid = match value.to_str() {
                Some("true") => true,
                Some("false") => false,
                _ => return Err(cli_error("Solid value must be 'true' or 'false'")),
            };
            Command::SetTileSolid {
                project: PathBuf::from(project),
                id: id.to_string_lossy().into_owned(),
                solid,
            }
        }
        _ => {
            return Err(cli_error(
                "unknown tilesheet action; expected 'add', 'name', or 'solid'",
            ));
        }
    };
    let mut json = false;
    for argument in args {
        if argument == "--json" && !json {
            json = true;
        } else {
            return Err(cli_error("unexpected argument for 'tilesheet'"));
        }
    }
    Ok(match command {
        Command::AddTilesheet { project, png, .. } => Command::AddTilesheet { project, png, json },
        Command::NameTilesheet { project, group, .. } => Command::NameTilesheet {
            project,
            group,
            json,
        },
        Command::SetTileSolid { project, id, solid } => {
            Command::SetTileSolid { project, id, solid }
        }
        _ => unreachable!(),
    })
}

fn parse_inspect_command(mut args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let Some(target) = args.next() else {
        return Err(cli_error(
            "missing inspection target; expected 'tilesheets' or 'map'",
        ));
    };
    match target.to_str() {
        Some("tilesheets") => parse_inspect_tilesheets(args),
        Some("map") => parse_inspect_map(args),
        _ => Err(cli_error(
            "unknown inspection target; expected 'tilesheets' or 'map'",
        )),
    }
}

fn parse_inspect_tilesheets(args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let mut project = None;
    let mut json = false;
    for argument in args {
        if argument == "--json" {
            if json {
                return Err(cli_error("duplicate '--json' option"));
            }
            json = true;
        } else if project.replace(PathBuf::from(argument)).is_some() {
            return Err(cli_error("too many arguments for 'inspect tilesheets'"));
        }
    }

    let Some(project) = project else {
        return Err(cli_error("missing project path for 'inspect tilesheets'"));
    };
    Ok(Command::InspectTilesheets { project, json })
}

fn parse_inspect_map(mut args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let Some(project) = args.next() else {
        return Err(cli_error("missing project path for 'inspect map'"));
    };
    let Some(map_file) = args.next() else {
        return Err(cli_error("missing .b8map file path for 'inspect map'"));
    };
    let mut json = false;
    for argument in args {
        if argument == "--json" && !json {
            json = true;
        } else {
            return Err(cli_error("unexpected argument for 'inspect map'"));
        }
    }
    Ok(Command::InspectMap {
        project: PathBuf::from(project),
        map_file: PathBuf::from(map_file),
        json,
    })
}

fn cli_error(message: &str) -> String {
    format!("{message}\nTry 'bit8 --help' for usage.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_and_version_flags_are_supported() {
        assert!(matches!(
            parse_command([OsString::from("--help")].into_iter()).unwrap(),
            Command::Help
        ));
        assert!(matches!(
            parse_command([OsString::from("-h")].into_iter()).unwrap(),
            Command::Help
        ));
        assert!(matches!(
            parse_command([OsString::from("--version")].into_iter()).unwrap(),
            Command::Version
        ));
        assert!(matches!(
            parse_command([OsString::from("-V")].into_iter()).unwrap(),
            Command::Version
        ));
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
        assert!(HELP.starts_with(&format!("Bit8 {VERSION}\n")));
        assert!(HELP.contains("tilesheet solid <project> <tile-id> <true|false>"));
    }

    #[test]
    fn parses_stable_tile_solid_metadata_command() {
        let command = parse_command(
            ["tilesheet", "solid", "games/demo", "B6", "true"]
                .into_iter()
                .map(OsString::from),
        )
        .unwrap();
        assert!(matches!(
            command,
            Command::SetTileSolid { project, id, solid: true }
                if project.as_path() == std::path::Path::new("games/demo") && id == "B6"
        ));
    }

    #[test]
    fn run_command_accepts_a_project_with_main_b8() {
        let project =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/primitives_demo");
        let Command::Run(path) =
            parse_command([OsString::from("run"), project.into_os_string()].into_iter()).unwrap()
        else {
            panic!("expected run command");
        };
        assert!(RuntimeSession::start(path).is_ok());
    }

    #[test]
    fn missing_run_path_is_a_concise_cli_error() {
        let error = parse_command([OsString::from("run")].into_iter()).unwrap_err();
        assert!(error.starts_with("missing project path\n"));
        assert!(error.contains("Try 'bit8 --help'"));
    }

    #[test]
    fn parses_host_project_argument() {
        assert!(matches!(
            parse_command([OsString::from("host"), OsString::from("game")].into_iter()),
            Ok(Command::Host(path)) if path.as_os_str() == "game"
        ));
        assert!(
            parse_command([OsString::from("host")].into_iter())
                .unwrap_err()
                .contains("missing project path")
        );
    }

    #[test]
    fn parses_read_only_tilesheet_inspection_with_optional_json_output() {
        let project = OsString::from("games/tilesheet_demo");
        assert!(matches!(
            parse_command(
                [
                    OsString::from("inspect"),
                    OsString::from("tilesheets"),
                    project.clone()
                ]
                .into_iter()
            )
            .unwrap(),
            Command::InspectTilesheets { json: false, .. }
        ));
        assert!(matches!(
            parse_command(
                [
                    OsString::from("inspect"),
                    OsString::from("tilesheets"),
                    OsString::from("--json"),
                    project,
                ]
                .into_iter()
            )
            .unwrap(),
            Command::InspectTilesheets { json: true, .. }
        ));
    }

    #[test]
    fn parses_read_only_map_inspection_with_project_and_map_paths() {
        assert!(matches!(
            parse_command([
                OsString::from("inspect"),
                OsString::from("map"),
                OsString::from("games/demo"),
                OsString::from("world.b8map"),
                OsString::from("--json"),
            ].into_iter()).unwrap(),
            Command::InspectMap { project, map_file, json: true }
                if project.as_os_str() == "games/demo" && map_file.as_os_str() == "world.b8map"
        ));
        assert!(
            parse_command(
                [
                    OsString::from("inspect"),
                    OsString::from("map"),
                    OsString::from("games/demo")
                ]
                .into_iter()
            )
            .unwrap_err()
            .contains("missing .b8map file path")
        );
    }

    #[test]
    fn world_inspection_command_is_retired() {
        let error = parse_command(
            [
                OsString::from("inspect"),
                OsString::from("world"),
                OsString::from("games/demo"),
                OsString::from("layout.b8world"),
            ]
            .into_iter(),
        )
        .unwrap_err();
        assert!(error.contains("unknown inspection target"));
    }

    #[test]
    fn parses_explicit_tilesheet_registration_and_naming_commands() {
        let project = OsString::from("game");
        let png = OsString::from("art.png");
        assert!(matches!(
            parse_command([
                OsString::from("tilesheet"), OsString::from("add"), project.clone(), png,
                OsString::from("--json")
            ].into_iter()).unwrap(),
            Command::AddTilesheet { project: parsed_project, png: parsed_png, json: true }
                if parsed_project.as_os_str() == "game" && parsed_png.as_os_str() == "art.png"
        ));
        assert!(matches!(
            parse_command([
                OsString::from("tilesheet"), OsString::from("name"), project,
                OsString::from("A"), OsString::from("--json")
            ].into_iter()).unwrap(),
            Command::NameTilesheet { project: parsed_project, group, json: true }
                if parsed_project.as_os_str() == "game" && group == "A"
        ));
    }

    #[test]
    fn inspection_cli_lists_the_demo_public_cell_ids() {
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo");
        let report = project_assets::inspect_tilesheets(&project).unwrap();
        let group_a = report
            .tilesheets
            .iter()
            .find(|sheet| sheet.group == "A")
            .expect("the demo retains registered Group A");
        assert_eq!(
            group_a
                .cells
                .iter()
                .map(|cell| cell.id.as_str())
                .collect::<Vec<_>>(),
            ["A1", "A2", "A3", "A4"]
        );
    }

    #[test]
    fn unknown_command_is_reported_clearly() {
        let error = parse_command([OsString::from("banana")].into_iter()).unwrap_err();
        assert!(error.starts_with("unknown command 'banana'\n"));
        assert!(error.contains("Try 'bit8 --help'"));
    }
}
