# Bit8 0.1.0

[English](README.md) | **繁體中文** | [日本語](README.ja.md)

Bit8 是一個以 Rust 開發、由 Lua 驅動的小型 2D 遊戲 Runtime 與開發工具組。

遊戲運行於固定的 64×64 framebuffer，並提供 Lua 5.4 腳本、
Tilemap、Sprite 與 Animation、Node、碰撞、Camera、
Language Service，以及整合 VS Code 的開發流程。

## 功能

- 使用 `.b8` 的 Lua 5.4 腳本
- 固定 64×64 framebuffer
- 固定 30 Hz 遊戲模擬
- 8×8 Tile 與穩定的 Asset ID
- 最大 256×256 Tile 的地圖
- Sprite 與 Animation 定義
- 可編寫腳本的 Map Node
- Box Collider 與 Solid Tile
- Camera 世界座標渲染
- Language Service
- VS Code Map Workspace
- 整合式 BIT8 GAME View
- 與編輯器無關的 Host Protocol

## 快速開始

### 從原始碼安裝

在 BIT8_CLI repository 根目錄執行：

    cargo install --path . --locked

執行 Bit8 專案：

    bit8 run <project-path>

Bit8 專案通常使用：

    main.b8

舊版 `main.lua` 專案仍然受到支援。

## 控制方式

桌面 Runtime 預設控制：

    方向鍵    移動
    Z         A
    X         B
    Escape    離開

## 一個最小的 Bit8 程式

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

## 專案結構

一個 Bit8 專案可以包含：

    my-game/
    ├── main.b8
    ├── world.b8map
    ├── bit8.assets.toml
    ├── bit8.sprites.toml
    ├── player.b8
    └── tilesheet (A).png

並不是每個專案都必須包含以上所有檔案。

## Tilesheet 與 Asset ID

Bit8 使用 `bit8.assets.toml` 註冊 PNG Tilesheet。

每個 Tile 為 8×8 game pixels，並擁有穩定的 ID，例如：

    A1
    A2
    A3
    B1

Asset Group 的身分是明確指定的，不會從檔名推測。

你可以直接繪製已註冊的 Tile：

    spr(A4, x, y)

## Map

Bit8 使用 `.b8map` 文字格式儲存地圖。

    version = 1
    width = 4
    height = 2

    A1 A1 A1 A1
    A1 -- A4 A1

Map 最大支援 256×256 Tiles。

Runtime API：

    map()
    mget(x, y)
    mset(x, y, tile)

Map 表示的是遊戲世界，而不是 framebuffer。
Framebuffer 仍固定為 64×64 game pixels。

## Node

Map 可以包含具有位置、Script、Sprite、Collider 與 Runtime State 的 Node。

Bit8 採用一個簡單的設計原則：

> **Main 組裝遊戲，Node 活在遊戲裡。**

Node Script 可以實作：

    func init()
    end

    func update()
    end

    func draw()
    end

## Sprite 與 Animation

Sprite 使用穩定的 Tile ID 定義於 `bit8.sprites.toml`。

Node 綁定 Sprite 後，可以：

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

## 碰撞

Tile 可以標記為 Solid。

具有 Box Collider 的 Node 可以使用：

    self:collide(dx, dy)
    self:move(dx, dy)

`self:collide()` 只進行碰撞查詢，不修改位置。

`self:move()` 則執行具有碰撞判定的移動。

## Camera

Camera Node 定義 64×64 viewport 的中心。

Map 與 Sprite 等世界座標內容會受到 Camera transform 影響，
而 screen-space drawing API 則維持螢幕座標。

## VS Code

`bit8-vscode` extension 提供：

- Bit8 語法支援
- Completion、Hover 與 Diagnostics
- Map Workspace
- Sprite 資訊
- Node 編輯
- Asset 註冊
- 整合式 BIT8 GAME View
- Run / Stop commands

VS Code 是 Bit8 的 frontend，而不是 Runtime 本身。

## Host Protocol

其他 Editor 或工具也可以在不開啟原生遊戲視窗的情況下啟動 Bit8：

    bit8 host <project-path>

Host 使用 NDJSON 透過 stdin/stdout 通訊。

這個介面刻意與 VS Code 解耦，使其他 Editor 與 frontend
未來也能整合 Bit8。

## 文件

更完整的技術文件位於 `docs/`。

Wiki 將提供偏向使用者的教學，包括：

- Getting Started
- Bit8 語言基礎
- Drawing
- Input
- Assets
- Maps
- Nodes
- Sprite 與 Animation
- Collision
- Camera
- VS Code
- CLI Reference
- Host Integration

完整的 0.1.0 功能摘要請參閱 `CHANGELOG.md`。

## 專案狀態

目前版本：**Bit8 0.1.0**

0.1.0 是 Bit8 的第一個公開版本。

Bit8 目前仍處於早期開發階段，未來版本可能調整 API 與檔案格式。

## License

Bit8 採用 MIT License。

Copyright (c) 2026 AKI
