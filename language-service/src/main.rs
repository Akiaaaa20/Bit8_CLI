use bit8::lua::translate_func_syntax;
use lsp_server::{Connection, Message, Notification, Request, Response};
use mlua::Lua;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let (connection, io_threads) = Connection::stdio();
    connection.initialize(server_capabilities())?;

    let mut documents = HashMap::<String, OpenDocument>::new();
    let mut shutdown_requested = false;

    for message in &connection.receiver {
        match message {
            Message::Request(request) if request.method == "shutdown" => {
                shutdown_requested = true;
                connection
                    .sender
                    .send(Message::Response(Response::new_ok(request.id, Value::Null)))?;
            }
            Message::Request(request) if request.method == "textDocument/completion" => {
                let result = completion_response(&request, &documents);
                connection
                    .sender
                    .send(Message::Response(Response::new_ok(request.id, result)))?;
            }
            Message::Request(request) if request.method == "textDocument/hover" => {
                let result = hover_response(&request, &documents);
                connection
                    .sender
                    .send(Message::Response(Response::new_ok(request.id, result)))?;
            }
            Message::Request(request) => {
                connection.sender.send(Message::Response(Response::new_err(
                    request.id,
                    -32601,
                    format!("method not found: {}", request.method),
                )))?;
            }
            Message::Notification(notification) if notification.method == "exit" => break,
            Message::Notification(notification) => {
                handle_notification(notification, &mut documents, &connection)?;
            }
            Message::Response(_) => {}
        }
    }

    drop(connection);
    io_threads.join()?;
    if !shutdown_requested {
        std::process::exit(1);
    }
    Ok(())
}

fn server_capabilities() -> Value {
    json!({
        "completionProvider": {
            "triggerCharacters": [
                "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m",
                "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M",
                "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "_"
            ]
        },
        "hoverProvider": true,
        "textDocumentSync": {
            "openClose": true,
            "change": 1
        }
    })
}

#[derive(Clone)]
struct OpenDocument {
    language_id: String,
    version: i64,
    text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ApiKind {
    Function,
    Constant,
    Property,
}

#[derive(Clone, Copy)]
struct ApiEntry {
    name: &'static str,
    signature: &'static str,
    documentation: &'static str,
    kind: ApiKind,
}

const API: &[ApiEntry] = &[
    ApiEntry {
        name: "init",
        signature: "init()",
        documentation: "Optional lifecycle function called once after the game file loads.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "update",
        signature: "update()",
        documentation: "Optional lifecycle function called once per game frame before draw().",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "draw",
        signature: "draw()",
        documentation: "Optional lifecycle function called once per game frame after update().",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "map",
        signature: "map()",
        documentation: "Draw the current map in world space. An enabled Camera translates the visible 64×64 game-pixel region; areas outside the map leave the existing framebuffer background visible.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "node",
        signature: "node(name) -> Node | nil",
        documentation: "Return the map's runtime node with this exact name, or nil if it does not exist.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "self",
        signature: "self: Node instance",
        documentation: "In a Node script, self refers to that Node. Built-in identity and transform fields are documented on self.id/type/name/x/y/enabled; arbitrary fields such as self.hp and self.speed are allowed, instance-local, and not inferred or persisted.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "name",
        signature: "Node.name: string",
        documentation: "Read the node's user-facing name. Node names are read-only in this phase.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "type",
        signature: "Node.type: string",
        documentation: "Read-only type returned by node(name).type: 'Node' or 'Camera'. For Camera, x/y are the center of a fixed 64×64 game-pixel viewport.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "x",
        signature: "Node.x: integer",
        documentation: "Read or change the node's game-pixel X coordinate.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "y",
        signature: "Node.y: integer",
        documentation: "Read or change the node's game-pixel Y coordinate.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "enabled",
        signature: "Node.enabled: boolean",
        documentation: "Read or change the node's enabled state. Disabled scripted nodes skip update() and draw(); their instance and local fields remain alive.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "clear",
        signature: "clear(color)",
        documentation: "Clear the 64×64 framebuffer in screen space; color is a palette index from 0 to 15 and defaults to 0.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "cls",
        signature: "cls(color)",
        documentation: "Alias of clear(color): clear the 64×64 framebuffer in screen space; color is a palette index from 0 to 15 and defaults to 0.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "pixel",
        signature: "pixel(x, y, color)",
        documentation: "Set one screen-space framebuffer pixel; Camera translation does not apply. Coordinates outside 0–63 are ignored; color must be a palette index from 0 to 15.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "pix",
        signature: "pix(x, y, color)",
        documentation: "Alias of pixel(x, y, color); coordinates stay in screen space and are not Camera-translated.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "sprite",
        signature: "sprite(id, x, y)",
        documentation: "Draw an 8×8 sprite from the project's PNG tilesheet. With an enabled Camera, x/y are world coordinates translated by the Camera; otherwise they are framebuffer coordinates. Palette index 0 is transparent; drawing is clipped to the framebuffer.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "spr",
        signature: "spr(id, x, y)",
        documentation: "Alias of sprite(id, x, y); an enabled Camera translates its world-space coordinates before framebuffer drawing.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "line",
        signature: "line(x0, y0, x1, y1, color)",
        documentation: "Draw a screen-space line between two points; Camera translation does not apply. It is clipped to the 64×64 framebuffer. Color must be a palette index from 0 to 15.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "rect",
        signature: "rect(x, y, width, height, color)",
        documentation: "Draw a screen-space rectangle outline; Camera translation does not apply. Width and height are pixel counts; non-positive dimensions draw nothing. Drawing is clipped to the framebuffer.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "rectfill",
        signature: "rectfill(x, y, width, height, color)",
        documentation: "Draw a filled screen-space rectangle; Camera translation does not apply. Width and height are pixel counts; non-positive dimensions draw nothing. Drawing is clipped to the framebuffer.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "circ",
        signature: "circ(x, y, radius, color)",
        documentation: "Draw an integer-radius screen-space circle centered at (x, y); Camera translation does not apply. Radius 0 draws the center pixel; a negative radius draws nothing. Drawing is clipped to the framebuffer.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "circfill",
        signature: "circfill(x, y, radius, color)",
        documentation: "Draw a filled integer-radius screen-space circle centered at (x, y); Camera translation does not apply. Radius 0 draws the center pixel; a negative radius draws nothing. Drawing is clipped to the framebuffer.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "button",
        signature: "button(button)",
        documentation: "Return true while the specified Bit8 button is held. Accepts UP/DOWN/LEFT/RIGHT/A/B constants or their uppercase name strings.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "btn",
        signature: "btn(button)",
        documentation: "Alias of button(button): return true while held. Accepts a Bit8 constant or uppercase name string such as \"RIGHT\".",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "buttonPressed",
        signature: "buttonPressed(button)",
        documentation: "Return true only on the frame when the button changes from released to pressed. Accepts Bit8 constants or uppercase name strings.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "btnp",
        signature: "btnp(button)",
        documentation: "Alias of buttonPressed(button): true only on the press-transition frame, not while held. Accepts a constant or uppercase name string.",
        kind: ApiKind::Function,
    },
    ApiEntry {
        name: "UP",
        signature: "UP",
        documentation: "Bit8 input constant for the up direction.",
        kind: ApiKind::Constant,
    },
    ApiEntry {
        name: "DOWN",
        signature: "DOWN",
        documentation: "Bit8 input constant for the down direction.",
        kind: ApiKind::Constant,
    },
    ApiEntry {
        name: "LEFT",
        signature: "LEFT",
        documentation: "Bit8 input constant for the left direction.",
        kind: ApiKind::Constant,
    },
    ApiEntry {
        name: "RIGHT",
        signature: "RIGHT",
        documentation: "Bit8 input constant for the right direction.",
        kind: ApiKind::Constant,
    },
    ApiEntry {
        name: "A",
        signature: "A",
        documentation: "Bit8 A input constant.",
        kind: ApiKind::Constant,
    },
    ApiEntry {
        name: "B",
        signature: "B",
        documentation: "Bit8 B input constant.",
        kind: ApiKind::Constant,
    },
];

const SELF_API: &[ApiEntry] = &[
    ApiEntry {
        name: "self.id",
        signature: "self.id: string",
        documentation: "Read-only stable ID of this Node instance.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "self.name",
        signature: "self.name: string",
        documentation: "Read-only name of this Node instance.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "self.type",
        signature: "self.type: string",
        documentation: "Read-only type of this Node: 'Node' or 'Camera'. For Camera, x/y are the center of a fixed 64×64 game-pixel viewport.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "self.x",
        signature: "self.x: integer",
        documentation: "Read or change this Node's game-pixel X coordinate. It shares state with node(name).x.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "self.y",
        signature: "self.y: integer",
        documentation: "Read or change this Node's game-pixel Y coordinate. It shares state with node(name).y.",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "self.enabled",
        signature: "self.enabled: boolean",
        documentation: "Read or change this Node's enabled state. Disabled instances skip update() and draw() but retain local fields and do not rerun init().",
        kind: ApiKind::Property,
    },
    ApiEntry {
        name: "self.move",
        signature: "self:move(dx, dy)",
        documentation: "Moves this Node by integer game-pixel deltas, resolving its Box Collider against Solid map tiles. X is resolved before Y.",
        kind: ApiKind::Function,
    },
];

fn completion_response(request: &Request, documents: &HashMap<String, OpenDocument>) -> Value {
    let Some(uri) = request_uri(request) else {
        return Value::Null;
    };
    let Some(document) = documents.get(uri) else {
        return Value::Null;
    };
    let Some(prefix) = request_prefix(request, &document.text) else {
        return Value::Null;
    };
    if document.language_id != "bit8" {
        return Value::Null;
    }

    if let Some(prefix) = self_member_prefix(request, &document.text) {
        return Value::Array(
            SELF_API
                .iter()
                .filter_map(|entry| {
                    let member = entry.name.strip_prefix("self.")?;
                    member.starts_with(&prefix).then(|| {
                        json!({
                            "label": member,
                            "kind": 5,
                            "detail": entry.signature,
                            "documentation": entry.documentation
                        })
                    })
                })
                .collect(),
        );
    }

    Value::Array(
        API.iter()
            .filter(|entry| entry.name.starts_with(&prefix))
            .map(|entry| {
                json!({
                    "label": entry.name,
                    "kind": match entry.kind { ApiKind::Function => 3, ApiKind::Constant => 21, ApiKind::Property => 5 },
                    "detail": entry.signature,
                    "documentation": entry.documentation
                })
            })
            .collect(),
    )
}

fn hover_response(request: &Request, documents: &HashMap<String, OpenDocument>) -> Value {
    let Some(uri) = request_uri(request) else {
        return Value::Null;
    };
    let Some(document) = documents.get(uri) else {
        return Value::Null;
    };
    let Some(word) = request_word(request, &document.text) else {
        return Value::Null;
    };
    if document.language_id != "bit8" {
        return Value::Null;
    }
    if let Some(prefix) = self_member_prefix(request, &document.text) {
        let Some(member) = request_word(request, &document.text) else {
            return Value::Null;
        };
        let Some(entry) = SELF_API.iter().find(|entry| {
            entry.name.strip_prefix("self.") == Some(member.as_str()) && member.starts_with(&prefix)
        }) else {
            return Value::Null;
        };
        return json!({ "contents": { "kind": "markdown", "value": format!("`{}`\n\n{}", entry.signature, entry.documentation) } });
    }
    let Some(entry) = API.iter().find(|entry| entry.name == word) else {
        return Value::Null;
    };
    json!({ "contents": { "kind": "markdown", "value": format!("`{}`\n\n{}", entry.signature, entry.documentation) } })
}

fn request_uri(request: &Request) -> Option<&str> {
    request.params.get("textDocument")?.get("uri")?.as_str()
}

fn request_prefix(request: &Request, text: &str) -> Option<String> {
    let params = &request.params;
    let position = params.get("position")?;
    let line = position.get("line")?.as_u64()? as usize;
    let character = position.get("character")?.as_u64()? as usize;
    Some(prefix_at(text, line, character))
}

fn self_member_prefix(request: &Request, text: &str) -> Option<String> {
    let position = request.params.get("position")?;
    let line_index = position.get("line")?.as_u64()? as usize;
    let character = position.get("character")?.as_u64()? as usize;
    let line = text.lines().nth(line_index)?;
    let mut byte_end = 0;
    let mut utf16_offset = 0;
    for (index, value) in line.char_indices() {
        if utf16_offset >= character {
            break;
        }
        byte_end = index + value.len_utf8();
        utf16_offset += value.len_utf16();
    }
    let line_prefix = &line[..byte_end];
    let (member_start, delimiter_len) =
        match (line_prefix.rfind("self."), line_prefix.rfind("self:")) {
            (Some(dot), Some(colon)) if colon > dot => (colon, "self:".len()),
            (Some(dot), _) => (dot, "self.".len()),
            (None, Some(colon)) => (colon, "self:".len()),
            (None, None) => return None,
        };
    let before = &line_prefix[..member_start];
    if before
        .chars()
        .last()
        .is_some_and(|value| value.is_ascii_alphanumeric() || value == '_' || value == '.')
    {
        return None;
    }
    let prefix = &line_prefix[member_start + delimiter_len..];
    prefix
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || value == '_')
        .then(|| prefix.to_owned())
}

fn request_word(request: &Request, text: &str) -> Option<String> {
    let params = &request.params;
    let position = params.get("position")?;
    let line = position.get("line")?.as_u64()? as usize;
    let character = position.get("character")?.as_u64()? as usize;
    Some(word_at(text, line, character))
}

fn prefix_at(text: &str, line_index: usize, utf16_character: usize) -> String {
    let Some(line) = text.lines().nth(line_index) else {
        return String::new();
    };
    let mut end = 0;
    let mut offset = 0;
    for (index, character) in line.char_indices() {
        if offset >= utf16_character {
            break;
        }
        end = index + character.len_utf8();
        offset += character.len_utf16();
    }
    let start = line[..end]
        .rfind(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .map_or(0, |index| index + 1);
    line[start..end].to_owned()
}

fn word_at(text: &str, line_index: usize, utf16_character: usize) -> String {
    let Some(line) = text.lines().nth(line_index) else {
        return String::new();
    };
    let mut utf16_offset = 0;
    let mut byte_offset = line.len();
    for (index, character) in line.char_indices() {
        let width = character.len_utf16();
        if utf16_offset + width > utf16_character {
            byte_offset = index;
            break;
        }
        utf16_offset += width;
    }
    let bytes = line.as_bytes();
    if byte_offset == line.len() && utf16_character < utf16_offset {
        return String::new();
    }
    let mut start = byte_offset;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    let mut end = byte_offset;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    line[start..end].to_owned()
}

fn update_document_text(
    documents: &mut HashMap<String, OpenDocument>,
    uri: &str,
    version: i64,
    text: &str,
) {
    if let Some(document) = documents.get_mut(uri) {
        document.version = version;
        document.text = text.to_owned();
    } else {
        documents.insert(
            uri.to_owned(),
            OpenDocument {
                language_id: String::new(),
                version,
                text: text.to_owned(),
            },
        );
    }
}

fn handle_notification(
    notification: Notification,
    documents: &mut HashMap<String, OpenDocument>,
    connection: &Connection,
) -> Result<(), Box<dyn Error>> {
    match notification.method.as_str() {
        "textDocument/didOpen" => {
            let Some(document) = notification.params.get("textDocument") else {
                return Ok(());
            };
            let (Some(uri), Some(text)) = (
                document.get("uri").and_then(Value::as_str),
                document.get("text").and_then(Value::as_str),
            ) else {
                return Ok(());
            };
            let language_id = document
                .get("languageId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let version = document
                .get("version")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            documents.insert(
                uri.to_owned(),
                OpenDocument {
                    language_id: language_id.to_owned(),
                    version,
                    text: text.to_owned(),
                },
            );
            publish_diagnostics(uri, text, connection)?;
        }
        "textDocument/didChange" => {
            let Some(params) = notification.params.as_object() else {
                return Ok(());
            };
            let Some(uri) = params
                .get("textDocument")
                .and_then(|document| document.get("uri"))
                .and_then(Value::as_str)
            else {
                return Ok(());
            };
            let Some(text) = params
                .get("contentChanges")
                .and_then(Value::as_array)
                .and_then(|changes| changes.last())
                .and_then(|change| change.get("text"))
                .and_then(Value::as_str)
            else {
                return Ok(());
            };
            let version = params
                .get("textDocument")
                .and_then(|document| document.get("version"))
                .and_then(Value::as_i64)
                .unwrap_or_default();
            update_document_text(documents, uri, version, text);
            publish_diagnostics(uri, text, connection)?;
        }
        "textDocument/didClose" => {
            let Some(uri) = notification
                .params
                .get("textDocument")
                .and_then(|document| document.get("uri"))
                .and_then(Value::as_str)
            else {
                return Ok(());
            };
            documents.remove(uri);
            connection
                .sender
                .send(Message::Notification(Notification::new(
                    "textDocument/publishDiagnostics".to_owned(),
                    json!({ "uri": uri, "diagnostics": [] }),
                )))?;
        }
        _ => {}
    }
    Ok(())
}

fn publish_diagnostics(
    uri: &str,
    source: &str,
    connection: &Connection,
) -> Result<(), Box<dyn Error>> {
    let source_name = document_name(uri);
    let diagnostics = match validate_named_source(source, &source_name) {
        Ok(()) => Vec::new(),
        Err(message) => vec![syntax_diagnostic(source, &message)],
    };
    connection
        .sender
        .send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".to_owned(),
            json!({ "uri": uri, "diagnostics": diagnostics }),
        )))?;
    Ok(())
}

#[cfg(test)]
fn validate_source(source: &str) -> Result<(), String> {
    validate_named_source(source, "Bit8")
}

fn validate_named_source(source: &str, source_name: &str) -> Result<(), String> {
    let translated = translate_func_syntax(source);
    Lua::new()
        .load(&translated)
        .set_name(format!("@{source_name}"))
        .into_function()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn document_name(uri: &str) -> String {
    uri.split(['?', '#'])
        .next()
        .unwrap_or(uri)
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("Bit8")
        .to_owned()
}

fn syntax_diagnostic(source: &str, message: &str) -> Value {
    let line = error_line(message).min(source.lines().count().saturating_sub(1));
    let end_character = source
        .lines()
        .nth(line)
        .map(|text| text.encode_utf16().count())
        .unwrap_or_default();
    json!({
        "range": {
            "start": { "line": line, "character": 0 },
            "end": { "line": line, "character": end_character }
        },
        "severity": 1,
        "source": "bit8",
        "message": message
    })
}

fn error_line(message: &str) -> usize {
    message
        .find("]:")
        .and_then(|offset| {
            message[offset + 2..]
                .split_once(':')
                .and_then(|(line, _)| line.parse::<usize>().ok())
        })
        .or_else(|| {
            message
                .rsplit_once(": ")
                .and_then(|(location, _)| location.rsplit_once(':'))
                .and_then(|(_, line)| line.parse::<usize>().ok())
        })
        .unwrap_or(1)
        .saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lsp_request(method: &str, uri: &str, character: usize) -> Request {
        Request::new(
            1.into(),
            method.to_owned(),
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": character }
            }),
        )
    }

    fn open_document(language_id: &str, text: &str) -> HashMap<String, OpenDocument> {
        HashMap::from([(
            "file:///main.b8".to_owned(),
            OpenDocument {
                language_id: language_id.to_owned(),
                version: 1,
                text: text.to_owned(),
            },
        )])
    }

    const MOVING_PIXEL_GAME: &str = r#"
x = 32

func init()
    playerX = 32
end

func update()
    if btn(LEFT) and playerX > 0 then
        playerX = playerX - 1
    end

    if btn(RIGHT) and playerX < 63 then
        playerX = playerX + 1
    end
end

func draw()
    cls(0)
    pix(playerX, 32, 1)
end
"#;

    #[test]
    fn accepts_current_bit8_syntax_and_api_names() {
        assert!(validate_source(MOVING_PIXEL_GAME).is_ok());
    }

    #[test]
    fn reports_missing_end() {
        let error = validate_source("func draw()\n    pix(1, 1, 1)").unwrap_err();
        assert!(error.contains("end"), "unexpected parser message: {error}");
    }

    #[test]
    fn reports_malformed_func_declaration() {
        assert!(validate_source("func ()\nend").is_err());
    }

    #[test]
    fn does_not_validate_unknown_user_globals_or_functions() {
        assert!(validate_source("playerX = 32\ncustom_game_call(playerX)").is_ok());
    }

    #[test]
    fn accepts_registered_asset_reference_syntax_without_declarations() {
        assert!(
            validate_source("func draw()\n    spr(A4, playerX, 32)\n    sprite(B1, 8, 8)\nend")
                .is_ok()
        );
    }

    #[test]
    fn diagnostics_point_to_lua_parser_line() {
        assert_eq!(error_line("[string \"Bit8\"]:3: syntax error"), 2);
    }

    #[test]
    fn diagnostics_keep_the_b8_document_filename() {
        let error = validate_named_source("\nfunc draw(\nend", "main.b8").unwrap_err();
        assert!(
            error.contains("main.b8"),
            "diagnostic omitted the file name: {error}"
        );
        assert_eq!(error_line(&error), 2);
        assert_eq!(document_name("file:///games/demo/main.b8"), "main.b8");
    }

    #[test]
    fn completion_request_returns_the_public_runtime_api_for_bit8_documents() {
        let documents = open_document("bit8", "");
        let request = lsp_request("textDocument/completion", "file:///main.b8", 0);
        let response = completion_response(&request, &documents);
        let labels = response
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item.get("label").and_then(Value::as_str))
            .collect::<Vec<_>>();

        for name in [
            "cls",
            "clear",
            "pix",
            "pixel",
            "spr",
            "sprite",
            "line",
            "rect",
            "rectfill",
            "circ",
            "circfill",
            "btn",
            "button",
            "btnp",
            "buttonPressed",
            "UP",
            "DOWN",
            "LEFT",
            "RIGHT",
            "A",
            "B",
            "init",
            "update",
            "draw",
            "node",
            "name",
            "x",
            "y",
            "enabled",
        ] {
            assert!(labels.contains(&name), "missing completion {name}");
        }
    }

    #[test]
    fn completion_request_filters_by_typed_prefix_and_not_lua_documents() {
        let bit8 = open_document("bit8", "rec");
        let request = lsp_request("textDocument/completion", "file:///main.b8", 3);
        let labels = completion_response(&request, &bit8);
        assert_eq!(
            labels
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["label"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["rect", "rectfill"]
        );

        let lua = open_document("lua", "rec");
        assert_eq!(completion_response(&request, &lua), Value::Null);

        let buttons = open_document("bit8", "btn");
        let request = lsp_request("textDocument/completion", "file:///main.b8", 3);
        let labels = completion_response(&request, &buttons);
        assert_eq!(
            labels
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["label"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["btn", "btnp"]
        );
    }

    #[test]
    fn advertises_full_document_sync_for_full_text_change_handling() {
        // LSP TextDocumentSyncKind::Full is 1; 2 means Incremental.
        assert_eq!(server_capabilities()["textDocumentSync"]["change"], 1);
    }

    #[test]
    fn completion_uses_the_current_multiline_document_position_and_version() {
        let uri = "file:///games/demo/main.b8";
        let text = format!("{}\nrec", "line\n".repeat(17));
        let documents = HashMap::from([(
            uri.to_owned(),
            OpenDocument {
                language_id: "bit8".to_owned(),
                version: 42,
                text,
            },
        )]);
        let request = Request::new(
            7.into(),
            "textDocument/completion".to_owned(),
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": 18, "character": 3 }
            }),
        );

        let response = completion_response(&request, &documents);
        let labels = response
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["label"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["rect", "rectfill"]);
    }

    #[test]
    fn full_text_change_replaces_the_document_and_advances_its_version() {
        let uri = "file:///games/demo/main.b8";
        let mut documents = HashMap::from([(
            uri.to_owned(),
            OpenDocument {
                language_id: "bit8".to_owned(),
                version: 8,
                text: "old text".to_owned(),
            },
        )]);
        update_document_text(
            &mut documents,
            uri,
            9,
            &format!("{}\nrec", "line\n".repeat(17)),
        );
        let document = documents.get(uri).unwrap();
        assert_eq!(document.version, 9);
        assert!(document.text.ends_with("\nrec"));

        let request = Request::new(
            8.into(),
            "textDocument/completion".to_owned(),
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": 18, "character": 3 }
            }),
        );
        let response = completion_response(&request, &documents);
        let labels = response
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["label"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["rect", "rectfill"]);
    }

    #[test]
    fn hover_request_documents_known_apis_and_ignores_unknown_globals() {
        let documents = open_document("bit8", "btnp(A)");
        let request = lsp_request("textDocument/hover", "file:///main.b8", 1);
        let hover = hover_response(&request, &documents);
        let text = hover["contents"]["value"].as_str().unwrap();
        assert!(text.contains("btnp(button)"));
        assert!(text.contains("press-transition frame"));

        let unknown = open_document("bit8", "playerX = 32");
        let request = lsp_request("textDocument/hover", "file:///main.b8", 2);
        assert_eq!(hover_response(&request, &unknown), Value::Null);

        let ordinary_lua = open_document("lua", "btnp(A)");
        let request = lsp_request("textDocument/hover", "file:///main.b8", 1);
        assert_eq!(hover_response(&request, &ordinary_lua), Value::Null);
    }

    #[test]
    fn hover_docs_cover_runtime_primitive_and_sprite_semantics() {
        for (word, expected) in [
            ("rectfill", "Width and height are pixel counts"),
            ("circ", "centered at (x, y)"),
            (
                "spr",
                "an enabled Camera translates its world-space coordinates",
            ),
            (
                "map",
                "An enabled Camera translates the visible 64×64 game-pixel region",
            ),
            ("pix", "coordinates stay in screen space"),
            ("LEFT", "Bit8 input constant"),
        ] {
            let source = word.to_string();
            let documents = open_document("bit8", &source);
            let request = lsp_request("textDocument/hover", "file:///main.b8", 0);
            assert!(
                hover_response(&request, &documents)["contents"]["value"]
                    .as_str()
                    .unwrap()
                    .contains(expected)
            );
        }
    }

    #[test]
    fn node_completion_and_hover_document_only_the_phase_one_node_api() {
        let documents = open_document("bit8", "nod");
        let request = lsp_request("textDocument/completion", "file:///main.b8", 3);
        let response = completion_response(&request, &documents);
        let labels = response
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["label"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["node"]);

        for (word, signature, description) in [
            ("node", "node(name) -> Node | nil", "exact name"),
            ("name", "Node.name: string", "read-only"),
            ("type", "Node.type: string", "node(name).type"),
            ("x", "Node.x: integer", "game-pixel X"),
            ("y", "Node.y: integer", "game-pixel Y"),
            (
                "enabled",
                "Node.enabled: boolean",
                "skip update() and draw()",
            ),
        ] {
            let document = open_document("bit8", word);
            let request = lsp_request("textDocument/hover", "file:///main.b8", 0);
            let hover = hover_response(&request, &document);
            let text = hover["contents"]["value"].as_str().unwrap();
            assert!(text.contains(signature), "missing {signature}: {text}");
            assert!(text.contains(description), "missing {description}: {text}");
            if word == "type" {
                assert!(
                    text.contains("center of a fixed 64×64 game-pixel viewport"),
                    "{text}"
                );
            }
        }
    }

    #[test]
    fn node_script_self_completion_and_hover_document_only_builtin_instance_fields() {
        let document = open_document("bit8", "self");
        let request = lsp_request("textDocument/hover", "file:///main.b8", 4);
        let response = hover_response(&request, &document);
        let text = response["contents"]["value"].as_str().unwrap();
        assert!(text.contains("self.hp and self.speed"));
        assert!(text.contains("not inferred or persisted"));

        let documents = open_document("bit8", "self.i");
        let request = lsp_request("textDocument/completion", "file:///main.b8", 6);
        let response = completion_response(&request, &documents);
        assert_eq!(response[0]["label"], "id");
        assert_eq!(response.as_array().unwrap().len(), 1);
        assert_eq!(response[0]["detail"], "self.id: string");

        let all_fields = open_document("bit8", "self.");
        let request = lsp_request("textDocument/completion", "file:///main.b8", 5);
        let response = completion_response(&request, &all_fields);
        let labels = response
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["label"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["id", "name", "type", "x", "y", "enabled", "move"]);

        let move_prefix = open_document("bit8", "self:m");
        let request = lsp_request("textDocument/completion", "file:///main.b8", 6);
        let completions = completion_response(&request, &move_prefix);
        assert_eq!(completions.as_array().unwrap().len(), 1);
        assert_eq!(completions[0]["label"], "move");
        assert_eq!(completions[0]["detail"], "self:move(dx, dy)");

        for (source, position, expected) in [
            ("self.id", 7, "Read-only stable ID"),
            ("self.name", 9, "Read-only name"),
            ("self.type", 9, "Read-only type of this Node"),
            ("self.x", 6, "game-pixel X coordinate"),
            ("self.y", 6, "game-pixel Y coordinate"),
            ("self.enabled", 12, "skip update() and draw()"),
        ] {
            let document = open_document("bit8", source);
            let request = lsp_request("textDocument/hover", "file:///main.b8", position);
            let response = hover_response(&request, &document);
            let text = response["contents"]["value"].as_str().unwrap();
            assert!(text.contains(expected), "{source}: {text}");
            if source == "self.type" {
                assert!(
                    text.contains("center of a fixed 64×64 game-pixel viewport"),
                    "{text}"
                );
            }
        }

        let move_document = open_document("bit8", "self:move");
        let request = lsp_request("textDocument/hover", "file:///main.b8", 9);
        let hover = hover_response(&request, &move_document);
        let documentation = hover["contents"]["value"].as_str().unwrap();
        assert!(documentation.contains("self:move(dx, dy)"));
        assert!(documentation.contains("integer game-pixel deltas"));
        assert!(documentation.contains("X is resolved before Y"));

        let custom = open_document("bit8", "self.speed");
        let request = lsp_request("textDocument/hover", "file:///main.b8", 10);
        assert_eq!(hover_response(&request, &custom), Value::Null);
    }
}
