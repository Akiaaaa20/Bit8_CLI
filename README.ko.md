<p align="center">
  <img src="BIT_8.jpg" width="45%">
</p>

# BIT8_CLI 0.1.0

[English](README.md) | [繁體中文](README.zh-TW.md) | [日本語](README.ja.md) | **한국어**

BIT8_CLI는 Rust로 개발된 Lua 기반의 소형 2D 게임 Runtime 및 개발 툴킷입니다.

게임은 고정된 64×64 framebuffer에서 실행되며, 게임 로직에는 Lua 5.4를 사용합니다. BIT8_CLI는 CLI Runtime, Tilemap, Sprite 및 Animation, Node, Collision, Camera, Language Service와 VS Code에 통합된 개발 환경을 제공합니다.

## 기능

- `.b8` 기반 Lua 5.4 스크립팅
- 고정 64×64 framebuffer
- 고정 30 Hz 게임 시뮬레이션
- 8×8 Tile과 안정적인 Asset ID
- 최대 256×256 Tile의 Map
- Sprite 및 Animation 정의
- Script를 사용할 수 있는 Map Node
- Solid Tile 및 Box Collider
- Camera 기반 월드 좌표 렌더링
- Language Service
- VS Code Map Workspace
- 통합 BIT8 GAME View
- Editor에 독립적인 Host Protocol

## 빠른 시작

### 소스에서 설치

Repository의 루트 디렉터리에서 실행합니다.

```bash
cargo install --path . --locked
```

BIT8_CLI 프로젝트를 실행합니다.

```bash
bit8 run <project-path>
```

BIT8_CLI 프로젝트는 일반적으로 `main.b8`을 메인 Script로 사용합니다.

기존 `main.lua` 프로젝트도 계속 지원됩니다.

### 조작 방법

Desktop Runtime의 기본 조작:

| 키 | 동작 |
| --- | --- |
| 방향키 | UP / DOWN / LEFT / RIGHT |
| Z | A |
| X | B |
| Escape | 종료 |

Project path는 현재 디렉터리를 기준으로 한 상대 경로나 절대 경로를 사용할 수 있습니다.

## 작은 BIT8_CLI 프로그램

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

BIT8_CLI는 Lua 5.4를 사용하며 `.b8` Script를 지원합니다. `func`는 BIT8_CLI에서 제공하는 문법 형식으로, 함수를 더 간결하게 정의할 수 있습니다.

## 프로젝트 구조

BIT8_CLI 프로젝트에는 다음과 같은 파일을 포함할 수 있습니다.

```text
my-game/
├── main.b8
├── world.b8map
├── bit8.assets.toml
├── bit8.sprites.toml
├── player.b8
└── tilesheet (A).png
```

모든 프로젝트에 위의 파일이 전부 필요한 것은 아닙니다.

`main.b8`은 게임 전체를 구성하며, Map, Node, Sprite 등의 리소스는 프로젝트의 필요에 따라 추가할 수 있습니다.

## Tilesheet 및 Asset ID

BIT8_CLI 프로젝트에서는 PNG Tilesheet를 `bit8.assets.toml`에 명시적으로 등록합니다.

각 PNG의 크기는 8로 나누어떨어져야 합니다. Tile은 고정된 8×8 game pixels이며, Cell은 왼쪽에서 오른쪽, 위에서 아래 순서로 번호가 지정됩니다.

등록된 Tile에는 다음과 같은 안정적인 Asset ID가 사용됩니다.

```text
A1
A2
A3
A4
B1
```

Asset Group의 식별자는 명시적으로 지정되며 파일 이름에서 자동으로 추측되지 않습니다. 따라서 `A1`과 `B1`은 각각 고유한 Group 및 Cell 식별 정보를 유지합니다.

등록된 Tile은 직접 그릴 수 있습니다.

```lua
spr(A4, x, y)
```

`spr()` / `sprite()`를 직접 사용하는 저수준 API와 기존 Numeric Sprite도 계속 사용할 수 있습니다.

VS Code에서는 PNG에 일반적인 Image Preview / Text Editor 흐름을 그대로 사용합니다. BIT8_CLI는 일반 PNG 편집 동작을 대체하는 대신 명시적인 Asset 등록 기능을 제공합니다.

## Map

BIT8_CLI는 `.b8map` 텍스트 형식으로 Map을 저장합니다.

예:

```text
version = 1
width = 4
height = 2

A1 A1 A1 A1
A1 -- A4 A1
```

`--`는 빈 Cell을 의미하며, 그 외의 Cell에는 등록된 Asset ID를 사용해야 합니다.

Map은 최대 **256×256 Tiles**를 지원합니다.

Runtime에서는 다음 API를 사용할 수 있습니다.

```lua
map()
mget(x, y)
mset(x, y, tile)
```

`map()`은 프로젝트의 `world.b8map`을 그립니다.

`mget(x, y)`는 0부터 시작하는 Tile 좌표를 사용해 Map Cell을 읽고 Asset ID 또는 `nil`을 반환합니다.

`mset(x, y, tile)`은 현재 RuntimeSession의 메모리에 있는 Map을 변경합니다. `nil`을 전달하면 Cell을 비울 수 있습니다.

Runtime에서 변경한 Map은 `.b8map` 원본 파일에 기록되지 않으며 Session이 종료되면 사라집니다.

각 Tile은 8×8 game pixels이지만 framebuffer는 항상 **64×64 game pixels**로 유지됩니다.

즉:

> **Map은 게임 세계이며 framebuffer 자체가 아닙니다.**

64×64 framebuffer에는 한 번에 게임 세계의 일부만 표시됩니다.

VS Code의 BIT8_CLI Map Workspace에서는 `.b8map`을 직접 편집할 수 있으며 Tile 선택, Pencil, Eraser, Pan, Zoom, Node 등의 편집 기능을 제공합니다.

## Node

BIT8_CLI의 Map에는 Node를 배치할 수 있습니다.

일반 Map Node는 안정적인 ID를 가지며 위치, Script, Sprite, Collider 및 Runtime State 등을 포함할 수 있습니다.

BIT8_CLI의 Node 설계는 하나의 간단한 원칙을 따릅니다.

> **Main은 게임을 구성하고, Node는 게임 안에서 살아갑니다.**

Node Script에서는 다음 함수를 구현할 수 있습니다.

```lua
func init()
end

func update()
end

func draw()
end
```

여러 Node가 같은 Script를 사용하더라도 각각 독립적인 Runtime State를 가집니다.

Node는 `self`를 통해 자신의 상태에 접근할 수 있습니다.

```lua
self.x
self.y
self.id
self.name
self.enabled
```

이를 통해 게임 오브젝트의 동작을 각자의 Script에 둘 수 있으며, `main.b8`이 게임 전체의 모든 로직을 담당할 필요가 없습니다.

## Sprite 및 Animation

BIT8_CLI의 Sprite workflow:

```text
PNG
 ↓
안정적인 Tile ID
 ↓
Sprite / Animation 정의
 ↓
Node
 ↓
Script에서 Animation 제어
```

Sprite 정의는 선택 사항인 `bit8.sprites.toml`에 저장됩니다.

예를 들어 `Player` Sprite는 `A4`를 Preview로 사용하고 `idle` 및 `walk` Animation을 가질 수 있습니다.

Map Node에 `Player` Sprite가 지정되어 있으면 Node의 `init()`이 실행되기 전에 Sprite가 먼저 초기 바인딩됩니다.

Node Script는 다음과 같이 작성할 수 있습니다.

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

`self:play()`는 현재 Animation을 제어합니다.

`self:spr()`는 Node의 현재 Sprite / Animation Frame을 그립니다.

각 Node는 독립적인 Animation playback state를 가지므로 여러 Node가 동일한 Sprite 정의를 사용하더라도 Animation 상태는 서로 공유되지 않습니다.

Map Workspace에는 Runtime에서 현재 재생 중인 Animation Frame이 아니라 안정적인 정적 Sprite Preview가 표시됩니다.

## Collision

등록된 Map Tile은 Solid로 설정할 수 있습니다.

일반 Node에는 Box Collider를 설정할 수도 있으며 Offset, Width, Height는 정수 game pixels 단위로 표현됩니다.

Node에서는 다음 API를 사용할 수 있습니다.

```lua
self:collide(dx, dy)
self:move(dx, dy)
```

`self:collide(dx, dy)`는 Node의 위치를 변경하지 않는 비파괴 Collision Query입니다.

`self:move(dx, dy)`는 Collision을 고려하여 이동하며 1 game pixel 단위로 Sweep하면서 X를 먼저 처리한 뒤 Y를 처리합니다.

다음과 같이 직접 변경할 수도 있습니다.

```lua
self.x
self.y
```

이 직접 대입은 계속해서 제한이 없는 저수준 동작입니다.

Collision은 월드 좌표에서 처리되며 Camera의 화면 변환과 독립적입니다.

## Camera

Camera Node는 64×64 framebuffer에 표시되는 게임 세계의 위치를 제어합니다.

Camera의 `x` / `y`는 viewport의 중심을 의미합니다.

Map에 여러 Camera가 존재하는 경우 Runtime은 활성화된 Camera 중 Numeric Node ID가 가장 작은 Camera를 사용합니다.

유효한 Camera가 없는 경우 월드 원점 `(0, 0)`이 사용됩니다.

Map, Sprite, Node Sprite 등의 월드 좌표 콘텐츠에는 Camera의 world-to-screen transform이 적용됩니다.

반면 screen-space drawing API는 화면 좌표를 그대로 유지합니다.

따라서 Map이나 Node의 실제 월드 좌표를 변경하지 않고 Camera를 사용해 게임 세계의 표시 영역을 이동할 수 있습니다.

## VS Code

`bit8-vscode` extension은 BIT8_CLI의 통합 개발 기능을 제공합니다.

- BIT8_CLI Syntax Support
- Completion
- Hover
- Diagnostics
- Map Workspace
- Sprite 정보
- Node 편집
- Asset 등록
- BIT8 GAME View
- Run / Stop Commands

Map Workspace에서는 `.b8map` 문서를 직접 편집하면서 VS Code의 기본 Save, Undo / Redo, Revert 및 Dirty State 동작을 그대로 유지합니다.

BIT8 GAME View는 Runtime의 64×64 framebuffer를 표시하고 키보드 입력을 BIT8_CLI에 전달합니다.

중요한 점:

> **VS Code는 BIT8_CLI의 frontend / client이며 BIT8_CLI Runtime 자체가 아닙니다.**

BIT8_CLI의 Core Runtime은 VS Code에 의존하지 않습니다.

## Host Protocol

자체 Editor, Frontend 또는 다른 Tool을 만들고 싶다면 네이티브 게임 창을 열지 않고 BIT8_CLI를 실행할 수 있습니다.

```bash
bit8 host <project-path>
```

Host는 stdin / stdout을 통해 NDJSON 형식으로 Frontend와 통신합니다.

Frontend는 입력을 제공하고 64×64 framebuffer Frame을 받을 수 있으며, 게임 시뮬레이션 자체는 RuntimeSession이 관리합니다.

BIT8_CLI의 게임 시뮬레이션은 **30 Hz**로 고정되어 있으며 Frontend가 Frame을 요청하는 빈도와 분리되어 있습니다.

Host Protocol은 의도적으로 VS Code와 독립적으로 설계되었습니다. 따라서 다른 Editor나 Tool도 `bit8-vscode`에 의존하지 않고 BIT8_CLI와 통합할 수 있습니다.

더 낮은 수준의 Protocol 세부 사항은 기술 문서를 참조하세요.

## Architecture

BIT8_CLI는 Game Runtime과 Editor Integration을 분리합니다.

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

`bit8 run`은 네이티브 Desktop에서 게임을 실행하는 방식을 제공합니다.

`bit8 host`는 Editor 및 다른 Frontend에서 사용할 수 있는 인터페이스를 제공합니다.

두 방식 모두 동일한 BIT8_CLI Runtime을 사용하며 각각 별도의 게임 로직을 구현하지 않습니다.

이를 통해 BIT8_CLI는 Editor-independent 구조를 유지하면서도 VS Code에서 통합된 개발 환경을 제공할 수 있습니다.

## 문서

더 자세한 BIT8_CLI 문서는 repository의 `docs/`에서 확인할 수 있습니다.

다음 문서도 참고할 수 있습니다.

- `CHANGELOG.md`: 0.1.0 기능 및 버전 변경 사항 요약
- `BIT8_SPEC.md`: 더 자세한 기술 사양
- `docs/`: Runtime, Sprite, Node 및 개발 workflow에 대한 상세 문서

README의 목적은 BIT8_CLI를 빠르게 이해할 수 있도록 돕는 것이며 전체 기술 사양을 대체하는 것은 아닙니다.

## 프로젝트 상태

현재 버전:

**BIT8_CLI 0.1.0**

0.1.0은 BIT8_CLI의 첫 번째 공개 릴리스입니다.

BIT8_CLI는 아직 초기 개발 단계에 있으므로 향후 버전에서 API, 파일 형식 및 일부 개발 workflow가 변경될 수 있습니다.

0.1.0은 작지만 완성된 기반을 구축하는 데 중점을 둡니다. 여기에는 Runtime, Lua Script, Asset, Map, Node, Sprite, Animation, Collision, Camera 및 Editor Integration이 포함됩니다.

## License

BIT8_CLI는 MIT License로 공개됩니다.

Copyright (c) 2026 AKI
