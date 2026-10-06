use crate::framebuffer::{Framebuffer, HEIGHT, WIDTH};
use crate::input::{Button, InputState};
use crate::palette::PALETTE;
use crate::runtime_map::RuntimeMap;
use crate::runtime_sprite::NodeRuntimeSpriteState;
use crate::sprite_definitions::SpriteRegistry;
use crate::tilesheet::Tilesheet;
use mlua::{Function, Lua, Result as LuaResult, Table, UserData, UserDataFields, Value};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

pub struct LuaRuntime {
    lua: Lua,
    initialized: Cell<bool>,
    input: Rc<Cell<InputState>>,
    node_scripts: RefCell<Vec<NodeScriptInstance>>,
    runtime_map: Option<Rc<RefCell<RuntimeMap>>>,
    sprites: Rc<SpriteRegistry>,
    sprite_renderer: Function,
}

struct NodeScriptInstance {
    id: String,
    name: String,
    script: String,
    node: Rc<RefCell<crate::runtime_map::RuntimeNode>>,
    init: Option<Function>,
    update: Option<Function>,
    draw: Option<Function>,
    sprite_state: Rc<RefCell<NodeRuntimeSpriteState>>,
}

impl LuaRuntime {
    pub fn new(framebuffer: Rc<RefCell<Framebuffer>>, tilesheet: Rc<Tilesheet>) -> LuaResult<Self> {
        Self::new_with_registered_tilesheets(framebuffer, tilesheet, HashMap::new())
    }

    pub fn new_with_registered_tilesheets(
        framebuffer: Rc<RefCell<Framebuffer>>,
        tilesheet: Rc<Tilesheet>,
        registered_tilesheets: HashMap<String, Tilesheet>,
    ) -> LuaResult<Self> {
        Self::new_with_runtime_map(framebuffer, tilesheet, Rc::new(registered_tilesheets), None)
    }

    pub fn new_with_runtime_map(
        framebuffer: Rc<RefCell<Framebuffer>>,
        tilesheet: Rc<Tilesheet>,
        registered_tilesheets: Rc<HashMap<String, Tilesheet>>,
        runtime_map: Option<Rc<RefCell<RuntimeMap>>>,
    ) -> LuaResult<Self> {
        let lua = Lua::new();
        let globals = lua.globals();
        let input = Rc::new(Cell::new(InputState::default()));

        let clear = {
            let framebuffer = Rc::clone(&framebuffer);
            lua.create_function(move |_, color: Option<i64>| {
                let color = palette_color(color.unwrap_or(0))?;
                framebuffer.borrow_mut().clear(color);
                Ok(())
            })?
        };
        globals.set("clear", clear.clone())?;
        globals.set("cls", clear)?;

        let pixel_framebuffer = Rc::clone(&framebuffer);
        let pixel = lua.create_function(move |_, (x, y, color): (i64, i64, i64)| {
            let color = palette_color(color)?;
            if x >= 0 && y >= 0 && x < WIDTH as i64 && y < HEIGHT as i64 {
                pixel_framebuffer
                    .borrow_mut()
                    .set_pixel(x as usize, y as usize, color);
            }
            Ok(())
        })?;
        globals.set("pixel", pixel.clone())?;
        globals.set("pix", pixel)?;

        let line_framebuffer = Rc::clone(&framebuffer);
        let line = lua.create_function(
            move |_, (x0, y0, x1, y1, color): (i64, i64, i64, i64, i64)| {
                let color = palette_color(color)?;
                line_framebuffer.borrow_mut().line(x0, y0, x1, y1, color);
                Ok(())
            },
        )?;
        globals.set("line", line)?;

        let rect_framebuffer = Rc::clone(&framebuffer);
        let rect = lua.create_function(
            move |_, (x, y, width, height, color): (i64, i64, i64, i64, i64)| {
                let color = palette_color(color)?;
                rect_framebuffer
                    .borrow_mut()
                    .rect(x, y, width, height, color);
                Ok(())
            },
        )?;
        globals.set("rect", rect)?;

        let rectfill_framebuffer = Rc::clone(&framebuffer);
        let rectfill = lua.create_function(
            move |_, (x, y, width, height, color): (i64, i64, i64, i64, i64)| {
                let color = palette_color(color)?;
                rectfill_framebuffer
                    .borrow_mut()
                    .rectfill(x, y, width, height, color);
                Ok(())
            },
        )?;
        globals.set("rectfill", rectfill)?;

        let circ_framebuffer = Rc::clone(&framebuffer);
        let circ = lua.create_function(move |_, (x, y, radius, color): (i64, i64, i64, i64)| {
            let color = palette_color(color)?;
            circ_framebuffer.borrow_mut().circ(x, y, radius, color);
            Ok(())
        })?;
        globals.set("circ", circ)?;

        let circfill_framebuffer = Rc::clone(&framebuffer);
        let circfill =
            lua.create_function(move |_, (x, y, radius, color): (i64, i64, i64, i64)| {
                let color = palette_color(color)?;
                circfill_framebuffer
                    .borrow_mut()
                    .circfill(x, y, radius, color);
                Ok(())
            })?;
        globals.set("circfill", circfill)?;

        let sprite_framebuffer = Rc::clone(&framebuffer);
        let sprite_tilesheet = Rc::clone(&tilesheet);
        let sprite_runtime_map = runtime_map.clone();
        let sprite = lua.create_function(move |_, (id, x, y): (Value, i64, i64)| {
            let sprite = match id {
                Value::Integer(id) => sprite_tilesheet
                    .cell(id)
                    .ok_or_else(|| mlua::Error::runtime("unknown sprite id"))?,
                Value::String(symbol) => {
                    let symbol = symbol.to_str()?;
                    if let Ok(index) = symbol.parse::<i64>() {
                        sprite_tilesheet
                            .cell(index)
                            .ok_or_else(|| mlua::Error::runtime("unknown sprite id"))?
                    } else {
                        let reference = crate::project_assets::parse_asset_reference(&symbol)
                            .ok_or_else(|| {
                                mlua::Error::runtime(format!(
                                    "invalid Bit8 sprite reference '{symbol}'"
                                ))
                            })?;
                        crate::project_assets::resolve_asset_reference(
                            &reference,
                            &registered_tilesheets,
                        )
                        .map_err(|error| mlua::Error::runtime(error.to_string()))?
                        .sprite
                    }
                }
                Value::Number(number) if number.fract() == 0.0 => sprite_tilesheet
                    .cell(number as i64)
                    .ok_or_else(|| mlua::Error::runtime("unknown sprite id"))?,
                _ => {
                    return Err(mlua::Error::runtime(
                        "sprite id must be a numeric index or a registered Bit8 asset symbol",
                    ));
                }
            };
            let (x, y) = sprite_runtime_map
                .as_ref()
                .map(|runtime_map| {
                    runtime_map
                        .borrow()
                        .camera_transform()
                        .world_to_screen(x, y)
                })
                .unwrap_or((x, y));
            sprite_framebuffer.borrow_mut().draw_sprite(sprite, x, y);
            Ok(())
        })?;
        globals.set("sprite", sprite.clone())?;
        globals.set("spr", sprite.clone())?;

        let map_function = {
            let runtime_map = runtime_map.clone();
            let framebuffer = Rc::clone(&framebuffer);
            lua.create_function(move |_, ()| {
                let runtime_map = runtime_map.as_ref().ok_or_else(|| {
                    mlua::Error::runtime("map APIs are unavailable for this runtime")
                })?;
                runtime_map
                    .borrow()
                    .render(&mut framebuffer.borrow_mut())
                    .map_err(mlua::Error::runtime)
            })?
        };
        globals.set("map", map_function)?;

        let mget = {
            let runtime_map = runtime_map.clone();
            lua.create_function(move |_, (x, y): (i64, i64)| {
                let runtime_map = runtime_map.as_ref().ok_or_else(|| {
                    mlua::Error::runtime("map APIs are unavailable for this runtime")
                })?;
                runtime_map.borrow().get(x, y).map_err(mlua::Error::runtime)
            })?
        };
        globals.set("mget", mget)?;

        let mset = {
            let runtime_map = runtime_map.clone();
            lua.create_function(move |_, (x, y, tile): (i64, i64, Value)| {
                let runtime_map = runtime_map.as_ref().ok_or_else(|| {
                    mlua::Error::runtime("map APIs are unavailable for this runtime")
                })?;
                let symbol = match tile {
                    Value::Nil => None,
                    Value::String(symbol) => Some(symbol.to_str()?.to_owned()),
                    _ => {
                        return Err(mlua::Error::runtime(
                            "map tile must be a registered Bit8 asset symbol or nil",
                        ));
                    }
                };
                runtime_map
                    .borrow_mut()
                    .set(x, y, symbol.as_deref())
                    .map_err(mlua::Error::runtime)
            })?
        };
        globals.set("mset", mset)?;

        let node_function = {
            let runtime_map = runtime_map.clone();
            lua.create_function(move |lua, name: String| {
                let node = runtime_map
                    .as_ref()
                    .and_then(|runtime_map| runtime_map.borrow().node_by_name(&name));
                match node {
                    Some(node) => Ok(Value::UserData(lua.create_userdata(LuaNode { node })?)),
                    None => Ok(Value::Nil),
                }
            })?
        };
        globals.set("node", node_function)?;

        for (name, button) in [
            ("UP", Button::Up),
            ("DOWN", Button::Down),
            ("LEFT", Button::Left),
            ("RIGHT", Button::Right),
            ("A", Button::A),
            ("B", Button::B),
        ] {
            globals.set(name, button.mask())?;
        }

        let button = {
            let input = Rc::clone(&input);
            lua.create_function(move |_, value: Value| {
                let button = bit8_button(value)?;
                Ok(input.get().is_down(button))
            })?
        };
        globals.set("button", button.clone())?;
        globals.set("btn", button)?;

        let button_pressed = {
            let input = Rc::clone(&input);
            lua.create_function(move |_, value: Value| {
                let button = bit8_button(value)?;
                Ok(input.get().was_just_pressed(button))
            })?
        };
        globals.set("buttonPressed", button_pressed.clone())?;
        globals.set("btnp", button_pressed)?;

        Ok(Self {
            lua,
            initialized: Cell::new(false),
            input,
            node_scripts: RefCell::new(Vec::new()),
            runtime_map,
            sprites: Rc::new(SpriteRegistry::default()),
            sprite_renderer: sprite,
        })
    }

    pub(crate) fn set_sprite_registry(&mut self, registry: SpriteRegistry) {
        self.sprites = Rc::new(registry);
    }

    pub fn load_file(&self, path: impl AsRef<Path>) -> LuaResult<()> {
        self.load_file_deferred(path)?;
        self.initialize()
    }

    pub fn load_file_deferred(&self, path: impl AsRef<Path>) -> LuaResult<()> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(mlua::Error::external)?;
        let chunk_name = format!("@{}", path.display());
        self.load_named_source(&source, Some(&chunk_name))
    }

    /// Creates an isolated Lua environment and `self` proxy for every scripted
    /// runtime Node. All configured paths must resolve inside project_root.
    pub fn load_node_scripts(
        &self,
        project_root: &Path,
        nodes: Vec<Rc<RefCell<crate::runtime_map::RuntimeNode>>>,
    ) -> LuaResult<()> {
        let root = project_root.canonicalize().map_err(mlua::Error::external)?;
        let mut instances = Vec::new();
        for node in nodes {
            let (id, name, configured_script) = {
                let node = node.borrow();
                (
                    node.id().to_owned(),
                    node.name().to_owned(),
                    node.script().map(str::to_owned),
                )
            };
            let Some(script) = configured_script else {
                continue;
            };
            let path = resolve_node_script_path(&root, &script)
                .map_err(|message| contextual_node_error(&id, &name, &script, "load", message))?;
            let source = fs::read_to_string(&path).map_err(|error| {
                contextual_node_error(&id, &name, &script, "load", error.to_string())
            })?;
            let sprite_state = Rc::new(RefCell::new(NodeRuntimeSpriteState::default()));
            if node.borrow().node_type() == crate::node_type::NodeType::Node
                && let Some(sprite) = node.borrow().initial_sprite()
            {
                sprite_state
                    .borrow_mut()
                    .bind(&self.sprites, sprite)
                    .map_err(|error| contextual_node_error(&id, &name, &script, "load", error))?;
            }
            let (environment, backing) =
                self.create_node_environment(Rc::clone(&node), Rc::clone(&sprite_state))?;
            let source = translate_asset_references(&translate_func_syntax(&source));
            self.lua
                .load(&source)
                .set_name(format!("@{}", path.display()))
                .set_environment(environment)
                .exec()
                .map_err(|error| contextual_node_error(&id, &name, &script, "load", error))?;
            instances.push(NodeScriptInstance {
                id,
                name,
                script,
                node,
                init: backing.get("init")?,
                update: backing.get("update")?,
                draw: backing.get("draw")?,
                sprite_state,
            });
        }
        *self.node_scripts.borrow_mut() = instances;
        Ok(())
    }

    fn create_node_environment(
        &self,
        node: Rc<RefCell<crate::runtime_map::RuntimeNode>>,
        sprite_state: Rc<RefCell<NodeRuntimeSpriteState>>,
    ) -> LuaResult<(Table, Table)> {
        let environment = self.lua.create_table()?;
        let backing = self.lua.create_table()?;
        let globals = self.lua.globals();
        backing.raw_set(
            "self",
            create_node_self(
                &self.lua,
                node,
                self.runtime_map.clone(),
                Rc::clone(&self.sprites),
                sprite_state,
                self.sprite_renderer.clone(),
            )?,
        )?;
        backing.raw_set("_G", environment.clone())?;

        let read_backing = backing.clone();
        let index = self
            .lua
            .create_function(move |_, (_environment, key): (Table, Value)| {
                let value = read_backing.raw_get::<Value>(key.clone())?;
                if !matches!(value, Value::Nil) {
                    return Ok(value);
                }
                globals.raw_get::<Value>(key)
            })?;
        let write_backing = backing.clone();
        let new_index = self.lua.create_function(
            move |_, (_environment, key, value): (Table, Value, Value)| {
                if matches!(&key, Value::String(name) if name.as_bytes().as_ref() == b"self") {
                    return Err(mlua::Error::runtime("Node script cannot replace self"));
                }
                write_backing.raw_set(key, value)
            },
        )?;
        let metatable = self.lua.create_table()?;
        metatable.raw_set("__index", index)?;
        metatable.raw_set("__newindex", new_index)?;
        environment.set_metatable(Some(metatable))?;
        Ok((environment, backing))
    }

    /// Calls main init once, followed by Node init callbacks in stable ID order.
    pub fn initialize(&self) -> LuaResult<()> {
        if self.initialized.replace(true) {
            return Ok(());
        }
        self.call_optional("init")?;
        for instance in self.node_scripts.borrow().iter() {
            call_node_callback(instance, "init", instance.init.as_ref())?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn load_source(&self, source: &str) -> LuaResult<()> {
        self.load_named_source(source, None)?;
        self.initialize()
    }

    fn load_named_source(&self, source: &str, name: Option<&str>) -> LuaResult<()> {
        let source = translate_asset_references(&translate_func_syntax(source));
        let chunk = self.lua.load(&source);
        if let Some(name) = name {
            chunk.set_name(name).exec()?;
        } else {
            chunk.exec()?;
        }
        Ok(())
    }

    pub fn frame(&self) -> LuaResult<()> {
        self.update_tick()?;
        self.draw()
    }

    pub(crate) fn update_tick(&self) -> LuaResult<()> {
        let mut input = self.input.get();
        input.begin_tick();
        self.input.set(input);
        let result = self.update_game();
        let mut input = self.input.get();
        input.finish_tick();
        self.input.set(input);
        result
    }

    fn update_game(&self) -> LuaResult<()> {
        self.call_optional("update")?;
        for instance in self.node_scripts.borrow().iter() {
            if instance.node.borrow().enabled() {
                let revision = instance.sprite_state.borrow().revision;
                call_node_callback(instance, "update", instance.update.as_ref())?;
                let mut state = instance.sprite_state.borrow_mut();
                if state.revision == revision {
                    state.advance(&self.sprites);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn draw(&self) -> LuaResult<()> {
        self.call_optional("draw")?;
        for instance in self.node_scripts.borrow().iter() {
            if instance.node.borrow().enabled() {
                call_node_callback(instance, "draw", instance.draw.as_ref())?;
            }
        }
        Ok(())
    }

    pub fn set_button_mask(&self, current: u8) {
        let mut input = self.input.get();
        input.sample_buttons(current);
        self.input.set(input);
    }

    fn call_optional(&self, name: &str) -> LuaResult<()> {
        let function = self.lua.globals().get::<Option<Function>>(name)?;
        if let Some(function) = function {
            function.call(())
        } else {
            Ok(())
        }
    }
}

fn create_node_self(
    lua: &Lua,
    node: Rc<RefCell<crate::runtime_map::RuntimeNode>>,
    runtime_map: Option<Rc<RefCell<RuntimeMap>>>,
    sprites: Rc<SpriteRegistry>,
    sprite_state: Rc<RefCell<NodeRuntimeSpriteState>>,
    sprite_renderer: Function,
) -> LuaResult<Table> {
    let proxy = lua.create_table()?;
    let custom_fields = lua.create_table()?;

    let bind_state = Rc::clone(&sprite_state);
    let bind_registry = Rc::clone(&sprites);
    let bind_sprite = lua.create_function(move |_, (_self, name): (Table, String)| {
        bind_state
            .borrow_mut()
            .bind(&bind_registry, &name)
            .map_err(mlua::Error::runtime)
    })?;
    let play_state = Rc::clone(&sprite_state);
    let play_registry = Rc::clone(&sprites);
    let play_node = Rc::clone(&node);
    let play = lua.create_function(move |_, (_self, name): (Table, String)| {
        play_state
            .borrow_mut()
            .play(&play_registry, &name, play_node.borrow().name())
            .map_err(mlua::Error::runtime)
    })?;
    let draw_node = Rc::clone(&node);
    let draw_sprite = lua.create_function(move |_, _self: Table| {
        let tile = sprite_state.borrow().tile(&sprites).map(str::to_owned);
        if let Some(tile) = tile {
            let node = draw_node.borrow();
            let (x, y) = (node.x(), node.y());
            drop(node);
            sprite_renderer.call::<()>((tile, x, y))?;
        }
        Ok(())
    })?;

    let collide_node = Rc::clone(&node);
    let collide_map = runtime_map.clone();
    let collide = lua.create_function(move |_, (_self, dx, dy): (Table, i64, i64)| {
        let Some(runtime_map) = &collide_map else {
            return Ok(false);
        };
        let node = collide_node.borrow();
        Ok(runtime_map.borrow().node_collides(&node, dx, dy))
    })?;

    let move_node = Rc::clone(&node);
    let move_map = runtime_map.clone();
    let move_method = lua.create_function(move |_, (_self, dx, dy): (Table, i64, i64)| {
        if let Some(runtime_map) = &move_map {
            let mut node = move_node.borrow_mut();
            runtime_map
                .borrow_mut()
                .move_node(&mut node, dx, dy)
                .map_err(mlua::Error::runtime)?;
        } else {
            let mut node = move_node.borrow_mut();
            let x = node.x().checked_add(dx).ok_or_else(|| {
                mlua::Error::runtime("Node movement exceeds the integer game-pixel range")
            })?;
            let y = node.y().checked_add(dy).ok_or_else(|| {
                mlua::Error::runtime("Node movement exceeds the integer game-pixel range")
            })?;
            node.set_x(x);
            node.set_y(y);
        }
        Ok(())
    })?;

    let read_node = Rc::clone(&node);
    let read_custom = custom_fields.clone();
    let collide_method = collide.clone();
    let move_method_for_index = move_method.clone();
    let index = lua.create_function(move |lua, (_proxy, key): (Table, Value)| {
        let name = match &key {
            Value::String(value) => Some(value.to_str()?.to_owned()),
            _ => None,
        };
        let node = read_node.borrow();
        match name.as_deref() {
            Some("id") => Ok(Value::String(lua.create_string(node.id())?)),
            Some("type") => Ok(Value::String(
                lua.create_string(node.node_type().identifier())?,
            )),
            Some("name") => Ok(Value::String(lua.create_string(node.name())?)),
            Some("x") => Ok(Value::Integer(node.x())),
            Some("y") => Ok(Value::Integer(node.y())),
            Some("enabled") => Ok(Value::Boolean(node.enabled())),
            Some("collide") => Ok(Value::Function(collide_method.clone())),
            Some("move") => Ok(Value::Function(move_method_for_index.clone())),
            Some("sprite") => Ok(Value::Function(bind_sprite.clone())),
            Some("play") => Ok(Value::Function(play.clone())),
            Some("spr") => Ok(Value::Function(draw_sprite.clone())),
            _ => read_custom.raw_get::<Value>(key),
        }
    })?;

    let write_node = Rc::clone(&node);
    let write_custom = custom_fields;
    let new_index =
        lua.create_function(move |_, (_proxy, key, value): (Table, Value, Value)| {
            let name = match &key {
                Value::String(value) => Some(value.to_str()?.to_owned()),
                _ => None,
            };
            match name.as_deref() {
                Some("id") | Some("type") | Some("name") | Some("collide") | Some("move")
                | Some("sprite") | Some("play") | Some("spr") => Err(mlua::Error::runtime(
                    format!("self.{} is read-only", name.as_deref().unwrap_or_default()),
                )),
                Some("x") => {
                    let value = lua_integer(value, "self.x")?;
                    write_node.borrow_mut().set_x(value);
                    Ok(())
                }
                Some("y") => {
                    let value = lua_integer(value, "self.y")?;
                    write_node.borrow_mut().set_y(value);
                    Ok(())
                }
                Some("enabled") => {
                    let Value::Boolean(value) = value else {
                        return Err(mlua::Error::runtime("self.enabled must be a boolean"));
                    };
                    write_node.borrow_mut().set_enabled(value);
                    Ok(())
                }
                _ => write_custom.raw_set(key, value),
            }
        })?;

    let metatable = lua.create_table()?;
    metatable.raw_set("__index", index)?;
    metatable.raw_set("__newindex", new_index)?;
    proxy.set_metatable(Some(metatable))?;
    Ok(proxy)
}

fn lua_integer(value: Value, field: &str) -> LuaResult<i64> {
    match value {
        Value::Integer(value) => Ok(value),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => {
            if value >= i64::MIN as f64 && value < i64::MAX as f64 {
                Ok(value as i64)
            } else {
                Err(mlua::Error::runtime(format!(
                    "{field} is outside the integer range"
                )))
            }
        }
        _ => Err(mlua::Error::runtime(format!("{field} must be an integer"))),
    }
}

fn call_node_callback(
    instance: &NodeScriptInstance,
    callback: &str,
    function: Option<&Function>,
) -> LuaResult<()> {
    let Some(function) = function else {
        return Ok(());
    };
    function.call(()).map_err(|error| {
        contextual_node_error(
            &instance.id,
            &instance.name,
            &instance.script,
            callback,
            error,
        )
    })
}

fn contextual_node_error(
    id: &str,
    name: &str,
    script: &str,
    callback: &str,
    error: impl std::fmt::Display,
) -> mlua::Error {
    mlua::Error::runtime(format!(
        "Node script error: node={id} ({name}), script={script}, callback={callback}: {error}"
    ))
}

fn resolve_node_script_path(root: &Path, script: &str) -> Result<std::path::PathBuf, String> {
    let configured = Path::new(script);
    if configured.is_absolute() {
        return Err("script path must be relative to the Bit8 project".to_owned());
    }
    let path = root
        .join(configured)
        .canonicalize()
        .map_err(|error| format!("cannot resolve script '{}': {error}", configured.display()))?;
    if !path.starts_with(root) {
        return Err("script path escapes the Bit8 project root".to_owned());
    }
    if !path.is_file() {
        return Err("script path does not refer to a file".to_owned());
    }
    Ok(path)
}

struct LuaNode {
    node: Rc<RefCell<crate::runtime_map::RuntimeNode>>,
}

impl UserData for LuaNode {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| Ok(this.node.borrow().id().to_owned()));
        fields.add_field_method_get("type", |_, this| {
            Ok(this.node.borrow().node_type().identifier().to_owned())
        });
        fields.add_field_method_get("name", |_, this| Ok(this.node.borrow().name().to_owned()));
        fields.add_field_method_get("x", |_, this| Ok(this.node.borrow().x()));
        fields.add_field_method_set("x", |_, this, value: i64| {
            this.node.borrow_mut().set_x(value);
            Ok(())
        });
        fields.add_field_method_get("y", |_, this| Ok(this.node.borrow().y()));
        fields.add_field_method_set("y", |_, this, value: i64| {
            this.node.borrow_mut().set_y(value);
            Ok(())
        });
        fields.add_field_method_get("enabled", |_, this| Ok(this.node.borrow().enabled()));
        fields.add_field_method_set("enabled", |_, this, value: bool| {
            this.node.borrow_mut().set_enabled(value);
            Ok(())
        });
    }
}

fn bit8_button(value: Value) -> LuaResult<Button> {
    let button = match value {
        Value::Integer(value) => Button::from_value(value),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => {
            Button::from_value(value as i64)
        }
        Value::String(value) => match value.to_str()?.as_ref() {
            "UP" => Some(Button::Up),
            "DOWN" => Some(Button::Down),
            "LEFT" => Some(Button::Left),
            "RIGHT" => Some(Button::Right),
            "A" => Some(Button::A),
            "B" => Some(Button::B),
            _ => None,
        },
        _ => None,
    };
    button.ok_or_else(|| {
        mlua::Error::runtime(
            "button must be a Bit8 button constant or one of UP, DOWN, LEFT, RIGHT, A, B",
        )
    })
}

fn palette_color(color: i64) -> LuaResult<u32> {
    PALETTE
        .get(usize::try_from(color).unwrap_or(usize::MAX))
        .copied()
        .ok_or_else(|| mlua::Error::runtime("color must be between 0 and 15"))
}

/// Converts Bit8's small `func name(...)` syntax to Lua before execution or validation.
pub fn translate_func_syntax(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let indentation = line.len() - line.trim_start().len();
            let content = &line[indentation..];
            if let Some(declaration) = content.strip_prefix("func ") {
                format!("{}function {}", &line[..indentation], declaration)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Rewrites only bare asset symbols in sprite IDs and `mset` tile arguments
/// into string tokens consumed by Rust bindings. Other Lua globals are untouched.
pub fn translate_asset_references(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut copied_until = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'\'' || bytes[index] == b'"' {
            index = skip_quoted_string(bytes, index);
            continue;
        }
        if bytes[index] == b'-' && bytes.get(index + 1) == Some(&b'-') {
            index = long_bracket_end(bytes, index + 2)
                .unwrap_or_else(|| skip_line_comment(bytes, index + 2));
            continue;
        }
        if let Some(end) = long_bracket_end(bytes, index) {
            index = end;
            continue;
        }
        if !is_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }

        let identifier_start = index;
        index += 1;
        while index < bytes.len() && is_identifier_continue(bytes[index]) {
            index += 1;
        }
        let identifier = &source[identifier_start..index];
        if identifier != "spr" && identifier != "sprite" && identifier != "mset" {
            continue;
        }

        let mut argument_start = skip_ascii_whitespace(bytes, index);
        if bytes.get(argument_start) != Some(&b'(') {
            continue;
        }
        argument_start = if identifier == "mset" {
            let Some(start) = third_argument_start(bytes, argument_start + 1) else {
                continue;
            };
            start
        } else {
            skip_ascii_whitespace(bytes, argument_start + 1)
        };
        let Some(argument_end) = asset_symbol_end(bytes, argument_start) else {
            continue;
        };
        let symbol = &source[argument_start..argument_end];
        if crate::project_assets::parse_asset_reference(symbol).is_none() {
            continue;
        }

        output.push_str(&source[copied_until..argument_start]);
        output.push('"');
        output.push_str(symbol);
        output.push('"');
        copied_until = argument_end;
        index = argument_end;
    }

    output.push_str(&source[copied_until..]);
    output
}

fn third_argument_start(bytes: &[u8], mut index: usize) -> Option<usize> {
    let mut commas = 0;
    let mut parentheses = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' => index = skip_quoted_string(bytes, index),
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index = long_bracket_end(bytes, index + 2)
                    .unwrap_or_else(|| skip_line_comment(bytes, index + 2));
            }
            b'(' => {
                parentheses += 1;
                index += 1;
            }
            b')' if parentheses == 0 => return None,
            b')' => {
                parentheses -= 1;
                index += 1;
            }
            b',' if parentheses == 0 => {
                commas += 1;
                index += 1;
                if commas == 2 {
                    return Some(skip_ascii_whitespace(bytes, index));
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn skip_ascii_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    index
}

fn asset_symbol_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut end = start;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        end += 1;
    }
    (end > start && bytes[start..end].iter().all(u8::is_ascii_alphanumeric)).then_some(end)
}

fn skip_quoted_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += 1;
        }
    }
    index
}

fn skip_line_comment(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn long_bracket_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut opening_end = start + 1;
    while bytes.get(opening_end) == Some(&b'=') {
        opening_end += 1;
    }
    if bytes.get(opening_end) != Some(&b'[') {
        return None;
    }

    let equals = opening_end - start - 1;
    let mut index = opening_end + 1;
    while index < bytes.len() {
        if bytes[index] == b']'
            && bytes.get(index + 1..index + 1 + equals) == Some(&bytes[start + 1..opening_end])
            && bytes.get(index + 1 + equals) == Some(&b']')
        {
            return Some(index + equals + 2);
        }
        index += 1;
    }
    Some(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use std::io::Cursor;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

    fn runtime(framebuffer: Rc<RefCell<Framebuffer>>) -> LuaRuntime {
        let tilesheet = Tilesheet::from_reader(Cursor::new(include_bytes!(
            "../games/tilesheet_demo/tilesheet (A).png"
        )))
        .unwrap();
        LuaRuntime::new(framebuffer, Rc::new(tilesheet)).unwrap()
    }

    fn registered_runtime(
        framebuffer: Rc<RefCell<Framebuffer>>,
    ) -> (LuaRuntime, std::path::PathBuf) {
        let project = std::env::temp_dir().join(format!(
            "bit8-lua-assets-{}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&project).unwrap();

        let mut sheet_a = RgbaImage::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let index = match (x / 8, y / 8) {
                    (0, 0) => 1,
                    (1, 0) => 2,
                    (0, 1) => 3,
                    _ => 4,
                };
                sheet_a.put_pixel(x, y, palette_rgba(index));
            }
        }
        DynamicImage::ImageRgba8(sheet_a)
            .save_with_format(project.join("a.png"), ImageFormat::Png)
            .unwrap();

        let mut sheet_b = RgbaImage::new(8, 8);
        for pixel in sheet_b.pixels_mut() {
            *pixel = palette_rgba(5);
        }
        DynamicImage::ImageRgba8(sheet_b)
            .save_with_format(project.join("b.png"), ImageFormat::Png)
            .unwrap();
        fs::write(
            project.join(crate::project_assets::REGISTRY_FILE),
            "version = 1\n\n[[tilesheets]]\ngroup = \"A\"\nfile = \"a.png\"\n\n[[tilesheets]]\ngroup = \"B\"\nfile = \"b.png\"\n",
        )
        .unwrap();

        let sheets = crate::project_assets::load_registered_tilesheets(&project).unwrap();
        let legacy_sheet = Rc::new(Tilesheet::empty());
        let game =
            LuaRuntime::new_with_registered_tilesheets(framebuffer, legacy_sheet, sheets).unwrap();
        (game, project)
    }

    fn palette_rgba(index: usize) -> image::Rgba<u8> {
        let color = PALETTE[index];
        image::Rgba([(color >> 16) as u8, (color >> 8) as u8, color as u8, 255])
    }

    #[test]
    fn func_declarations_are_translated() {
        let source = "func draw()\npix(1, 2, 3)\nend";
        assert_eq!(
            translate_func_syntax(source),
            "function draw()\npix(1, 2, 3)\nend"
        );
    }

    #[test]
    fn drawing_aliases_share_the_framebuffer_implementation() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        game.lua
            .load("clear(0)\ncls(0)\npixel(31, 31, 1)\npix(32, 31, 2)")
            .exec()?;

        let framebuffer = framebuffer.borrow();
        assert_eq!(framebuffer.pixels()[31 * WIDTH + 31], PALETTE[1]);
        assert_eq!(framebuffer.pixels()[31 * WIDTH + 32], PALETTE[2]);
        assert_eq!(framebuffer.pixels().len(), WIDTH * HEIGHT);
        Ok(())
    }

    #[test]
    fn primitive_drawing_apis_write_palette_pixels_to_the_framebuffer() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        game.lua
            .load(
                "clear(0)\nline(0, 0, 2, 0, 1)\nrect(4, 4, 1, 1, 2)\nrectfill(6, 6, 2, 2, 3)\ncirc(10, 10, 0, 4)\ncircfill(12, 12, 1, 5)",
            )
            .exec()?;

        let framebuffer = framebuffer.borrow();
        assert_eq!(framebuffer.pixels()[0], PALETTE[1]);
        assert_eq!(framebuffer.pixels()[4 * WIDTH + 4], PALETTE[2]);
        assert_eq!(framebuffer.pixels()[6 * WIDTH + 7], PALETTE[3]);
        assert_eq!(framebuffer.pixels()[10 * WIDTH + 10], PALETTE[4]);
        assert_eq!(framebuffer.pixels()[12 * WIDTH + 12], PALETTE[5]);
        assert_eq!(framebuffer.pixels()[12 * WIDTH + 11], PALETTE[5]);
        Ok(())
    }

    #[test]
    fn primitive_drawing_uses_existing_palette_validation() {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(framebuffer);

        assert!(game.lua.load("line(0, 0, 1, 1, 16)").exec().is_err());
        assert!(game.lua.load("rect(0, 0, 1, 1, -1)").exec().is_err());
        assert!(game.lua.load("circfill(0, 0, 1, 99)").exec().is_err());
    }

    #[test]
    fn primitives_demo_draws_all_five_shapes_without_a_desktop_window() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        game.load_source(include_str!("../games/primitives_demo/main.b8"))?;

        game.frame()?;

        let framebuffer = framebuffer.borrow();
        for color in &PALETTE[1..=5] {
            assert!(framebuffer.pixels().contains(color));
        }
        Ok(())
    }

    #[test]
    fn main_b8_file_uses_the_existing_preprocessor_and_runtime() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/moving_pixel/main.b8");

        game.load_file(path)?;
        game.frame()?;

        assert_eq!(framebuffer.borrow().pixels()[32 * WIDTH + 32], PALETTE[1]);
        Ok(())
    }

    #[test]
    fn legacy_main_lua_file_still_loads() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("bit8-vscode/test/fixtures/main.lua");

        game.load_file(path)?;
        game.frame()?;

        assert_eq!(framebuffer.borrow().pixels().len(), WIDTH * HEIGHT);
        Ok(())
    }

    #[test]
    fn file_syntax_errors_keep_the_source_filename() {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(framebuffer);
        let directory =
            std::env::temp_dir().join(format!("bit8-syntax-error-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("main.b8");
        fs::write(&path, "func draw(\n").unwrap();

        let error = game.load_file(&path).unwrap_err();
        fs::remove_file(&path).unwrap();
        fs::remove_dir(directory).unwrap();

        assert!(
            error.to_string().contains("main.b8"),
            "unexpected Lua error: {error}"
        );
        assert!(error.to_string().contains(".b8"));
    }

    #[test]
    fn lifecycle_functions_run_in_order_and_init_runs_once() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        game.load_source(
            "calls = ''\nfunc init() calls = calls .. 'i' end\nfunc update() calls = calls .. 'u' end\nfunc draw() calls = calls .. 'd' end",
        )?;
        assert_eq!(game.lua.globals().get::<String>("calls")?, "i");

        game.load_source("func init() calls = calls .. 'i' end")?;
        assert_eq!(game.lua.globals().get::<String>("calls")?, "i");

        game.frame()?;
        game.frame()?;

        assert_eq!(game.lua.globals().get::<String>("calls")?, "iudud");
        assert_eq!(framebuffer.borrow().pixels().len(), WIDTH * HEIGHT);
        Ok(())
    }

    #[test]
    fn missing_lifecycle_functions_are_allowed() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        game.load_source("func draw() pix(1, 2, 1) end")?;

        game.frame()?;
        assert_eq!(framebuffer.borrow().pixels()[2 * WIDTH + 1], PALETTE[1]);
        Ok(())
    }

    #[test]
    fn moving_pixel_game_updates_before_drawing() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        game.load_source(include_str!("../games/moving_pixel/main.b8"))?;

        game.frame()?;
        assert_eq!(framebuffer.borrow().pixels()[32 * WIDTH + 32], PALETTE[1]);

        game.set_button_mask(Button::Right.mask());
        game.frame()?;
        let framebuffer = framebuffer.borrow();
        assert_eq!(framebuffer.pixels()[32 * WIDTH + 32], PALETTE[0]);
        assert_eq!(framebuffer.pixels()[32 * WIDTH + 33], PALETTE[1]);
        Ok(())
    }

    #[test]
    fn lua_input_aliases_report_held_and_pressed_once() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(framebuffer);
        game.load_source(
            "func update() held = button(RIGHT) == btn(RIGHT)\npressed = buttonPressed(A) == btnp(A)\npressed_result = btnp(A) end",
        )?;

        game.set_button_mask(Button::Right.mask() | Button::A.mask());
        game.frame()?;
        assert!(game.lua.globals().get::<bool>("held")?);
        assert!(game.lua.globals().get::<bool>("pressed")?);
        assert!(game.lua.globals().get::<bool>("pressed_result")?);

        game.set_button_mask(Button::Right.mask() | Button::A.mask());
        game.frame()?;
        assert!(game.lua.globals().get::<bool>("held")?);
        assert!(game.lua.globals().get::<bool>("pressed")?);
        assert!(!game.lua.globals().get::<bool>("pressed_result")?);
        Ok(())
    }

    #[test]
    fn sprite_aliases_draw_cells_from_the_loaded_tilesheet() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        framebuffer.borrow_mut().clear(PALETTE[4]);
        let game = runtime(Rc::clone(&framebuffer));

        game.lua.load("sprite(0, 10, 10)\nspr(1, 20, 20)").exec()?;

        let framebuffer = framebuffer.borrow();
        assert_eq!(framebuffer.pixels()[10 * WIDTH + 10], PALETTE[4]);
        assert_eq!(framebuffer.pixels()[20 * WIDTH + 20], PALETTE[4]);
        let first_cell_has_pixels =
            (10..18).any(|y| (10..18).any(|x| framebuffer.pixels()[y * WIDTH + x] != PALETTE[4]));
        let second_cell_has_pixels =
            (20..28).any(|y| (20..28).any(|x| framebuffer.pixels()[y * WIDTH + x] != PALETTE[4]));
        assert!(first_cell_has_pixels);
        assert!(second_cell_has_pixels);
        Ok(())
    }

    #[test]
    fn b8_asset_symbols_preserve_group_and_one_based_cell_identity() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let (game, project) = registered_runtime(Rc::clone(&framebuffer));
        let source = "func draw()\n    spr(A1, 0, 0)\n    spr(A4, 8, 0)\n    sprite(A4, 16, 0)\n    spr(B1, 24, 0)\nend\n";
        let path = project.join("main.b8");
        fs::write(&path, source).unwrap();
        game.load_file(&path)?;
        game.frame()?;

        let framebuffer = framebuffer.borrow();
        assert_eq!(framebuffer.pixels()[0], PALETTE[1]);
        assert_eq!(framebuffer.pixels()[8], PALETTE[4]);
        assert_eq!(framebuffer.pixels()[16], PALETTE[4]);
        assert_eq!(framebuffer.pixels()[24], PALETTE[5]);
        drop(framebuffer);
        fs::remove_dir_all(project).unwrap();
        Ok(())
    }

    #[test]
    fn node_api_resolves_and_mutates_runtime_state_without_changing_identity_or_map_file()
    -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let (_, project) = registered_runtime(Rc::clone(&framebuffer));
        let map_source = concat!(
            "version = 1\nwidth = 1\nheight = 1\n--\n\n",
            "[node_state]\nnext_id = 2\n\n",
            "[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 256\ny = 224\nenabled = true\nscript = \"player.b8\"\n"
        );
        let map_path = project.join("world.b8map");
        fs::write(&map_path, map_source).unwrap();
        let registered =
            Rc::new(crate::project_assets::load_registered_tilesheets(&project).unwrap());
        let runtime_map = Rc::new(RefCell::new(
            RuntimeMap::load_default(&project, Rc::clone(&registered)).unwrap(),
        ));
        let game = LuaRuntime::new_with_runtime_map(
            Rc::clone(&framebuffer),
            Rc::new(Tilesheet::empty()),
            registered,
            Some(runtime_map),
        )?;
        let source = concat!(
            "player = node('Player')\n",
            "assert(player ~= nil and node('Banana') == nil)\n",
            "assert(player.id == 'N1' and player.type == 'Node' and player.name == 'Player')\n",
            "assert(player.x == 256 and player.y == 224 and player.enabled == true)\n",
            "player.x = 120\nplayer.y = 80\nplayer.enabled = false\n",
            "assert(player.x == 120 and player.y == 80 and player.enabled == false)\n",
            "local changed_id = pcall(function() player.id = 'N99' end)\n",
            "assert(not changed_id and player.id == 'N1')\n",
            "local changed_type = pcall(function() player.type = 'Camera' end)\n",
            "assert(not changed_type and player.type == 'Node')\n"
        );
        let source_path = project.join("main.b8");
        fs::write(&source_path, source).unwrap();
        game.load_file(&source_path)?;
        assert_eq!(fs::read_to_string(map_path).unwrap(), map_source);
        fs::remove_dir_all(project).unwrap();
        Ok(())
    }

    #[test]
    fn tilesheet_demo_executes_its_real_registered_a4_reference() -> LuaResult<()> {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("games/tilesheet_demo");
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let registered =
            Rc::new(crate::project_assets::load_registered_tilesheets(&project).unwrap());
        let sprites = SpriteRegistry::load_project(&project).unwrap().unwrap();
        let map = Rc::new(RefCell::new(
            RuntimeMap::load_default_with_sprites(&project, Rc::clone(&registered), &sprites)
                .unwrap(),
        ));
        let mut expected = Framebuffer::new();
        let a4 = registered["A"].cell(3).unwrap();
        assert!(a4.iter().any(|index| *index != 0));
        expected.draw_sprite(a4, 32, 32);
        let legacy = Rc::new(Tilesheet::load_png(project.join("tilesheet (A).png")).unwrap());
        let mut game = LuaRuntime::new_with_runtime_map(
            Rc::clone(&framebuffer),
            legacy,
            registered,
            Some(Rc::clone(&map)),
        )?;

        game.set_sprite_registry(sprites);
        game.load_file_deferred(project.join("main.b8"))?;
        game.load_node_scripts(&project, map.borrow().nodes_in_stable_id_order())?;
        game.initialize()?;
        game.frame()?;

        let framebuffer = framebuffer.borrow();
        for y in 32..40 {
            for x in 32..40 {
                assert_eq!(
                    framebuffer.pixels()[y * WIDTH + x],
                    expected.pixels()[y * WIDTH + x]
                );
            }
        }
        Ok(())
    }

    #[test]
    fn unknown_asset_group_in_b8_source_reports_the_original_symbol() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let (game, project) = registered_runtime(framebuffer);
        let path = project.join("main.b8");
        fs::write(&path, "func draw()\n    spr(Z1, 0, 0)\nend\n").unwrap();
        game.load_file(&path)?;

        let error = game.frame().unwrap_err().to_string();
        assert!(error.contains("Z1"), "unexpected error: {error}");
        assert!(error.contains("unknown tilesheet group"));
        assert!(error.contains("main.b8"), "missing source path: {error}");
        fs::remove_dir_all(project).unwrap();
        Ok(())
    }

    #[test]
    fn out_of_range_asset_cell_in_b8_source_reports_the_original_symbol() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let (game, project) = registered_runtime(framebuffer);
        let path = project.join("main.b8");
        fs::write(&path, "func draw()\n    sprite(A99, 0, 0)\nend\n").unwrap();
        game.load_file(&path)?;

        let error = game.frame().unwrap_err().to_string();
        assert!(error.contains("A99"), "unexpected error: {error}");
        assert!(error.contains("out of range"));
        fs::remove_dir_all(project).unwrap();
        Ok(())
    }

    #[test]
    fn asset_transform_is_limited_to_sprite_and_map_tile_arguments() {
        let source = "A4 = 7\nspr(A4, 1, 2)\nmset(0, nested(1, 2), B3)\nx = A4\ntext = 'spr(B1, 0, 0)' -- spr(C1, 0, 0)\n--[[ spr(D1, 0, 0) ]]\nraw = [=[sprite(E1, 0, 0)]=]\n";
        assert_eq!(
            translate_asset_references(source),
            "A4 = 7\nspr(\"A4\", 1, 2)\nmset(0, nested(1, 2), \"B3\")\nx = A4\ntext = 'spr(B1, 0, 0)' -- spr(C1, 0, 0)\n--[[ spr(D1, 0, 0) ]]\nraw = [=[sprite(E1, 0, 0)]=]\n"
        );
    }

    #[test]
    fn moving_sprite_example_uses_input_and_clips_at_edges() -> LuaResult<()> {
        let framebuffer = Rc::new(RefCell::new(Framebuffer::new()));
        let game = runtime(Rc::clone(&framebuffer));
        game.load_source(include_str!("../games/moving_sprite/main.b8"))?;

        game.frame()?;
        game.set_button_mask(Button::Right.mask());
        game.frame()?;
        assert_eq!(game.lua.globals().get::<i64>("x")?, 29);
        assert_eq!(game.lua.globals().get::<i64>("y")?, 28);
        let framebuffer = framebuffer.borrow();
        assert!(
            (28..36)
                .any(|y| { (29..37).any(|x| framebuffer.pixels()[y * WIDTH + x] != PALETTE[0]) })
        );
        drop(framebuffer);

        for _ in 0..64 {
            game.set_button_mask(Button::Left.mask());
            game.frame()?;
        }
        assert_eq!(game.lua.globals().get::<i64>("x")?, 0);

        for _ in 0..64 {
            game.set_button_mask(Button::Up.mask());
            game.frame()?;
        }
        assert_eq!(game.lua.globals().get::<i64>("y")?, 0);
        Ok(())
    }
}
