<p align="center">
  <img src="BIT_8.jpg" width="45%">
</p>

# BIT8_CLI 0.1.0

[English](README.md) | **繁體中文** | [日本語](README.ja.md) | [한국어](README.ko.md)

BIT8_CLI 是一個以 Rust 開發、由 Lua 驅動的小型 2D 遊戲 Runtime 與開發工具組。

遊戲運行於固定的 64×64 framebuffer，並使用 Lua 5.4 編寫遊戲邏輯。BIT8_CLI 提供 CLI Runtime、Tilemap、Sprite 與 Animation、Node、碰撞、Camera、Language Service，以及整合 VS Code 的開發流程。

## 功能

- 使用 `.b8` 的 Lua 5.4 腳本
- 固定 64×64 framebuffer
- 固定 30 Hz 遊戲模擬
- 8×8 Tile 與穩定的 Asset ID
- 最大 256×256 Tile 的 Map
- Sprite 與 Animation 定義
- 可編寫 Script 的 Map Node
- Solid Tile 與 Box Collider
- Camera 世界座標渲染
- Language Service
- VS Code Map Workspace
- 整合式 BIT8 GAME View
- 與 Editor 無關的 Host Protocol

## 快速開始

### 從原始碼安裝

在 repository 根目錄執行：

```bash
cargo install --path . --locked
```

執行 BIT8_CLI 專案：

```bash
bit8 run <project-path>
```

BIT8_CLI 專案通常使用 `main.b8` 作為主要 Script。

舊版 `main.lua` 專案仍然受到支援。

### 控制方式

桌面 Runtime 的預設控制：

| 按鍵 | 功能 |
| --- | --- |
| 方向鍵 | UP / DOWN / LEFT / RIGHT |
| Z | A |
| X | B |
| Escape | 離開 |

Project path 可以是相對於目前目錄的路徑，也可以使用絕對路徑。

## 一個小小的 BIT8_CLI 程式

```lua
func init()
    x = 28
    y = 28
end

func update()
    if btn(LEFT) then x = x - 1 end
    if btn(RIGHT) then x = x + 1 end
    if btn(UP) then y = y - 1 end
    if btn(DOWN) then y = y + 1 end
end

func draw()
    cls()
    rectfill(x, y, x + 7, y + 7, 7)
end
```

BIT8_CLI 使用 Lua 5.4，並支援 `.b8` Script。`func` 是 BIT8_CLI 提供的語法形式，讓 Script 可以用較簡潔的方式定義函式。

## 專案結構

一個 BIT8_CLI 專案可以包含：

```text
my-game/
├── main.b8
├── world.b8map
├── bit8.assets.toml
├── bit8.sprites.toml
├── player.b8
└── tilesheet (A).png
```

並不是每個專案都必須包含以上所有檔案。

`main.b8` 負責組裝遊戲，而 Map、Node、Sprite 與其他資源則依照專案需求加入。

## Tilesheet 與 Asset ID

BIT8_CLI 專案透過 `bit8.assets.toml` 明確註冊 PNG Tilesheet。

每個 PNG 的尺寸必須可以被 8 整除。Tile 固定為 8×8 game pixels，並依照由左至右、由上至下的順序取得 Cell 編號。

註冊後的 Tile 使用穩定的 Asset ID，例如：

```text
A1
A2
A3
A4
B1
```

Asset Group 的身分是明確指定的，不會從檔名自動推測，因此 `A1` 與 `B1` 分別保留自己的 Group 與 Cell 身分。

已註冊的 Tile 可以直接繪製：

```lua
spr(A4, x, y)
```

直接使用 `spr()` / `sprite()` 與舊有 Numeric Sprite 的低階 API 仍然有效。

在 VS Code 中，PNG 仍然使用一般的 Image Preview / Text Editor 流程。BIT8_CLI 提供明確的 Asset 註冊功能，而不是接管一般 PNG 編輯行為。

## Map

BIT8_CLI 使用 `.b8map` 文字格式儲存 Map。

例如：

```text
version = 1
width = 4
height = 2

A1 A1 A1 A1
A1 -- A4 A1
```

`--` 代表空白 Cell，其餘 Cell 必須使用已註冊的 Asset ID。

Map 最大支援 **256×256 Tiles**。

Runtime 提供：

```lua
map()
mget(x, y)
mset(x, y, tile)
```

`map()` 繪製專案的 `world.b8map`。

`mget(x, y)` 使用從 0 開始的 Tile 座標讀取 Map Cell，並回傳 Asset ID 或 `nil`。

`mset(x, y, tile)` 修改目前 RuntimeSession 記憶體中的 Map。傳入 `nil` 可以清除 Cell。

Runtime 對 Map 的修改不會改寫 `.b8map` 原始檔案，Session 結束後便會消失。

每個 Tile 為 8×8 game pixels，而 framebuffer 始終維持 **64×64 game pixels**。

因此：

> **Map 是遊戲世界，不是 framebuffer。**

64×64 framebuffer 一次只顯示世界中的一部分。

VS Code 的 BIT8_CLI Map Workspace 可以直接編輯 `.b8map`，並提供 Tile 選擇、Pencil、Eraser、Pan、Zoom、Node 與相關編輯功能。

## Node

BIT8_CLI Map 可以包含 Node。

一般 Map Node 擁有穩定 ID，並可以包含位置、Script、Sprite、Collider 與 Runtime State。

BIT8_CLI 的 Node 設計遵循一個簡單原則：

> **Main 組裝遊戲，Node 活在遊戲裡。**

Node Script 可以實作：

```lua
func init()
end

func update()
end

func draw()
end
```

不同 Node 即使使用相同 Script，也擁有各自獨立的 Runtime State。

Node 可以透過 `self` 存取自己的狀態，例如：

```lua
self.x
self.y
self.id
self.name
self.enabled
```

這讓遊戲物件的行為可以留在自己的 Script 中，而 `main.b8` 不需要承擔整個遊戲的所有邏輯。

## Sprite 與 Animation

BIT8_CLI 的 Sprite workflow 為：

```text
PNG
 ↓
穩定的 Tile ID
 ↓
Sprite / Animation 定義
 ↓
Node
 ↓
Script 控制 Animation
```

Sprite 定義儲存在可選的 `bit8.sprites.toml`。

例如，一個 `Player` Sprite 可以使用 `A4` 作為 Preview，並擁有 `idle` 與 `walk` Animation。

當 Map Node 綁定 `Player` Sprite 後，Sprite 會在 Node 的 `init()` 執行前完成初始綁定。

Node Script 可以寫成：

```lua
func init()
    self:play("idle")
end

func update()
    if btn(RIGHT) then
        self:move(1, 0)
        self:play("walk")
    else
        self:play("idle")
    end
end

func draw()
    self:spr()
end
```

`self:play()` 控制目前 Animation。

`self:spr()` 繪製 Node 目前 Sprite / Animation 的 Frame。

每個 Node 擁有自己的 Animation playback state，因此多個 Node 使用相同 Sprite 定義時，Animation 狀態仍然彼此獨立。

Map Workspace 顯示的是穩定的靜態 Sprite Preview，而不是 Runtime 當下正在播放的 Animation Frame。

## 碰撞

註冊的 Map Tile 可以標記為 Solid。

一般 Node 也可以具有 Box Collider，其 Offset、Width 與 Height 都以整數 game pixels 表示。

Node 提供：

```lua
self:collide(dx, dy)
self:move(dx, dy)
```

`self:collide(dx, dy)` 是非破壞性的碰撞查詢，不會修改 Node 的位置。

`self:move(dx, dy)` 則進行具有碰撞判定的移動，以 1 game pixel 為單位掃描，先處理 X，再處理 Y。

直接修改：

```lua
self.x
self.y
```

仍然是不受限制的低階操作。

碰撞使用世界座標，與 Camera 的畫面轉換彼此獨立。

## Camera

Camera Node 控制世界在 64×64 framebuffer 中顯示的位置。

Camera 的 `x` / `y` 表示 viewport 的中心。

如果 Map 中存在多個 Camera，Runtime 使用已啟用且 Numeric Node ID 最小的 Camera。

如果沒有有效的 Camera，世界原點則為 `(0, 0)`。

世界座標內容，例如 Map、Sprite 與 Node Sprite，會經過 Camera 的 world-to-screen transform。

Screen-space drawing API 則保持在螢幕座標中。

因此 Camera 可以移動遊戲世界的視野，而不需要實際改變 Map 或 Node 的世界座標。

## VS Code

`bit8-vscode` extension 提供 BIT8_CLI 的整合開發功能，包括：

- BIT8_CLI 語法支援
- Completion
- Hover
- Diagnostics
- Map Workspace
- Sprite 資訊
- Node 編輯
- Asset 註冊
- BIT8 GAME View
- Run / Stop Commands

Map Workspace 可以直接編輯 `.b8map` 文件，同時保留 VS Code 原生的 Save、Undo / Redo、Revert 與 Dirty State 行為。

BIT8 GAME View 顯示 Runtime 的 64×64 framebuffer，並將鍵盤輸入傳送給 BIT8_CLI。

需要特別注意：

> **VS Code 是 BIT8_CLI 的 frontend / client，而不是 BIT8_CLI Runtime 本身。**

BIT8_CLI 的核心 Runtime 並不依賴 VS Code。

## Host Protocol

想建立自己的 Editor、Frontend 或其他 Tool 時，可以在不開啟原生遊戲視窗的情況下啟動 BIT8_CLI：

```bash
bit8 host <project-path>
```

Host 透過 stdin / stdout 使用 NDJSON 與 Frontend 通訊。

Frontend 可以提供輸入並取得 64×64 framebuffer Frame，而遊戲模擬仍然由 RuntimeSession 管理。

BIT8_CLI 的遊戲模擬固定為 **30 Hz**，與 Frontend 要求畫面的頻率分離。

Host Protocol 刻意設計成與 VS Code 無關，因此其他 Editor 或 Tool 未來也可以整合 BIT8_CLI，而不需要依賴 `bit8-vscode`。

更底層的 Protocol 細節請參閱技術文件。

## Architecture

BIT8_CLI 將遊戲 Runtime 與 Editor Integration 分開：

```text
       Game Project
            │
            ▼
      RuntimeSession
            │
       ┌────┴────┐
       ▼         ▼
    bit8 run   bit8 host
       │         │
       ▼         ▼
    Desktop    Editors /
    Window     Tooling
```

`bit8 run` 提供原生桌面執行方式。

`bit8 host` 則提供給 Editor 與其他 Frontend 使用的介面。

兩者共用 BIT8_CLI 的 Runtime，而不是各自實作另一套遊戲邏輯。

這讓 BIT8_CLI 可以保持 Editor-independent，同時仍然提供完整的 VS Code 開發體驗。

## 文件

更完整的 BIT8_CLI 文件可以在 repository 的 `docs/` 中找到。

你也可以參閱：

- `CHANGELOG.md`：0.1.0 功能與版本變更摘要
- `BIT8_SPEC.md`：更深入的技術規格
- `docs/`：Runtime、Sprite、Node 與開發流程等詳細文件

README 的目標是讓你快速了解 BIT8_CLI，而不是取代完整的技術規格。

## 專案狀態

目前版本：

**BIT8_CLI 0.1.0**

0.1.0 是 BIT8_CLI 的第一個公開版本。

BIT8_CLI 目前仍處於早期開發階段，因此未來版本可能會調整 API、檔案格式與部分開發流程。

0.1.0 的重點是建立一套小型但完整的基礎：Runtime、Lua Script、Asset、Map、Node、Sprite、Animation、Collision、Camera，以及 Editor Integration。

## License

BIT8_CLI 採用 MIT License。

Copyright (c) 2026 AKI
