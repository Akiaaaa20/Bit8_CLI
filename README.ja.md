<p align="center">
  <img src="BIT_8.jpg" width="45%">
</p>

# BIT8_CLI 0.1.0

[English](README.md) | [繁體中文](README.zh-TW.md) | **日本語** | [한국어](README.ko.md)

BIT8_CLI は、Rust で開発された Lua 駆動の小型 2D ゲーム Runtime / 開発ツールキットです。

ゲームは固定 64×64 framebuffer 上で動作し、ゲームロジックには Lua 5.4 を使用します。BIT8_CLI は CLI Runtime、Tilemap、Sprite / Animation、Node、Collision、Camera、Language Service、そして VS Code と統合された開発環境を提供します。

## 機能

- `.b8` による Lua 5.4 スクリプティング
- 固定 64×64 framebuffer
- 固定 30 Hz ゲームシミュレーション
- 8×8 Tile と安定した Asset ID
- 最大 256×256 Tile の Map
- Sprite / Animation 定義
- Script を持つ Map Node
- Solid Tile と Box Collider
- Camera によるワールド座標レンダリング
- Language Service
- VS Code Map Workspace
- 統合された BIT8 GAME View
- Editor に依存しない Host Protocol

## クイックスタート

### ソースからインストール

Repository のルートディレクトリで実行します。

```bash
cargo install --path . --locked
```

BIT8_CLI プロジェクトを実行します。

```bash
bit8 run <project-path>
```

BIT8_CLI プロジェクトでは通常、`main.b8` をメイン Script として使用します。

従来の `main.lua` プロジェクトも引き続きサポートされています。

### 操作方法

Desktop Runtime のデフォルト操作：

| キー | 操作 |
| --- | --- |
| 矢印キー | UP / DOWN / LEFT / RIGHT |
| Z | A |
| X | B |
| Escape | 終了 |

Project path には、現在のディレクトリからの相対パスまたは絶対パスを使用できます。

## 小さな BIT8_CLI プログラム

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

BIT8_CLI は Lua 5.4 を使用し、`.b8` Script をサポートします。`func` は BIT8_CLI が提供する構文形式で、より簡潔に関数を定義できます。

## プロジェクト構成

BIT8_CLI プロジェクトには、次のようなファイルを含めることができます。

```text
my-game/
├── main.b8
├── world.b8map
├── bit8.assets.toml
├── bit8.sprites.toml
├── player.b8
└── tilesheet (A).png
```

すべてのプロジェクトで、これらすべてのファイルが必要なわけではありません。

`main.b8` がゲーム全体を組み立て、Map、Node、Sprite などのリソースをプロジェクトの必要に応じて追加します。

## Tilesheet と Asset ID

BIT8_CLI プロジェクトでは、PNG Tilesheet を `bit8.assets.toml` に明示的に登録します。

各 PNG のサイズは 8 で割り切れる必要があります。Tile は固定 8×8 game pixels で、Cell は左から右、上から下の順に番号付けされます。

登録された Tile には、次のような安定した Asset ID が割り当てられます。

```text
A1
A2
A3
A4
B1
```

Asset Group の識別子は明示的に指定され、ファイル名から自動推測されません。そのため `A1` と `B1` は、それぞれの Group と Cell の識別情報を維持します。

登録済みの Tile は直接描画できます。

```lua
spr(A4, x, y)
```

`spr()` / `sprite()` を直接使用する低レベル API と、従来の Numeric Sprite も引き続き利用できます。

VS Code では、PNG は通常の Image Preview / Text Editor のフローを維持します。BIT8_CLI は一般的な PNG 編集機能を置き換えるのではなく、明示的な Asset 登録機能を提供します。

## Map

BIT8_CLI は `.b8map` テキスト形式で Map を保存します。

例：

```text
version = 1
width = 4
height = 2

A1 A1 A1 A1
A1 -- A4 A1
```

`--` は空の Cell を表します。それ以外の Cell には登録済みの Asset ID を使用する必要があります。

Map は最大 **256×256 Tiles** をサポートします。

Runtime では次の API を利用できます。

```lua
map()
mget(x, y)
mset(x, y, tile)
```

`map()` はプロジェクトの `world.b8map` を描画します。

`mget(x, y)` は 0 始まりの Tile 座標から Map Cell を読み取り、Asset ID または `nil` を返します。

`mset(x, y, tile)` は現在の RuntimeSession が保持するメモリ上の Map を変更します。`nil` を渡すと Cell をクリアできます。

Runtime で行った Map の変更は `.b8map` のソースファイルには書き込まれず、Session の終了時に破棄されます。

各 Tile は 8×8 game pixels ですが、framebuffer は常に **64×64 game pixels** のままです。

つまり：

> **Map はゲーム世界であり、framebuffer そのものではありません。**

64×64 framebuffer が一度に表示するのは、ゲーム世界の一部分だけです。

VS Code の BIT8_CLI Map Workspace では `.b8map` を直接編集でき、Tile 選択、Pencil、Eraser、Pan、Zoom、Node などの編集機能を利用できます。

## Node

BIT8_CLI の Map には Node を配置できます。

通常の Map Node は安定した ID を持ち、位置、Script、Sprite、Collider、Runtime State などを保持できます。

BIT8_CLI の Node 設計には、シンプルな考え方があります。

> **Main はゲームを組み立て、Node はゲームの中で生きる。**

Node Script では次の関数を実装できます。

```lua
func init()
end

func update()
end

func draw()
end
```

複数の Node が同じ Script を使用していても、それぞれ独立した Runtime State を持ちます。

Node は `self` を通して自身の状態へアクセスできます。

```lua
self.x
self.y
self.id
self.name
self.enabled
```

これにより、ゲームオブジェクトの振る舞いをそれぞれの Script に持たせることができ、`main.b8` にゲーム全体のロジックを集中させる必要がありません。

## Sprite と Animation

BIT8_CLI の Sprite workflow：

```text
PNG
 ↓
安定した Tile ID
 ↓
Sprite / Animation 定義
 ↓
Node
 ↓
Script から Animation を制御
```

Sprite 定義は、任意の `bit8.sprites.toml` に保存されます。

例えば `Player` Sprite は `A4` を Preview として使用し、`idle` と `walk` Animation を持つことができます。

Map Node に `Player` Sprite が割り当てられている場合、Node の `init()` が実行される前に Sprite の初期バインドが行われます。

Node Script は次のように記述できます。

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

`self:play()` は現在の Animation を制御します。

`self:spr()` は Node の現在の Sprite / Animation Frame を描画します。

各 Node は独立した Animation playback state を持つため、複数の Node が同じ Sprite 定義を使用していても Animation の状態は共有されません。

Map Workspace に表示されるのは安定した静的 Sprite Preview であり、Runtime で現在再生されている Animation Frame ではありません。

## Collision

登録済みの Map Tile は Solid として設定できます。

通常の Node には Box Collider を設定することもでき、Offset、Width、Height は整数の game pixels で表されます。

Node では次の API を利用できます。

```lua
self:collide(dx, dy)
self:move(dx, dy)
```

`self:collide(dx, dy)` は位置を変更しない非破壊的な Collision Query です。

`self:move(dx, dy)` は Collision を考慮して移動し、1 game pixel ずつ Sweep しながら X、Y の順で処理します。

次のように直接変更することもできます。

```lua
self.x
self.y
```

この直接代入は、引き続き制約のない低レベル操作です。

Collision はワールド座標で処理され、Camera の画面変換とは独立しています。

## Camera

Camera Node は、64×64 framebuffer に表示されるゲーム世界の位置を制御します。

Camera の `x` / `y` は viewport の中心を表します。

Map 内に複数の Camera が存在する場合、Runtime は有効になっている Camera のうち Numeric Node ID が最も小さいものを使用します。

有効な Camera が存在しない場合、ワールド原点 `(0, 0)` が使用されます。

Map、Sprite、Node Sprite などのワールド座標コンテンツには、Camera の world-to-screen transform が適用されます。

一方、screen-space drawing API はスクリーン座標のまま維持されます。

そのため、Map や Node の実際のワールド座標を変更することなく、Camera を使ってゲーム世界の表示範囲を移動できます。

## VS Code

`bit8-vscode` extension は BIT8_CLI の統合開発機能を提供します。

- BIT8_CLI の Syntax Support
- Completion
- Hover
- Diagnostics
- Map Workspace
- Sprite 情報
- Node 編集
- Asset 登録
- BIT8 GAME View
- Run / Stop Commands

Map Workspace は `.b8map` ドキュメントを直接編集しながら、VS Code 標準の Save、Undo / Redo、Revert、Dirty State の動作を維持します。

BIT8 GAME View は Runtime の 64×64 framebuffer を表示し、キーボード入力を BIT8_CLI に送信します。

重要な点：

> **VS Code は BIT8_CLI の frontend / client であり、BIT8_CLI Runtime そのものではありません。**

BIT8_CLI の Core Runtime は VS Code に依存していません。

## Host Protocol

独自の Editor、Frontend、その他の Tool を作成する場合、ネイティブゲームウィンドウを開かずに BIT8_CLI を起動できます。

```bash
bit8 host <project-path>
```

Host は stdin / stdout 上の NDJSON を使用して Frontend と通信します。

Frontend は入力を提供して 64×64 framebuffer Frame を取得でき、ゲームシミュレーション自体は RuntimeSession が管理します。

BIT8_CLI のゲームシミュレーションは **30 Hz** に固定されており、Frontend が Frame を要求する頻度とは分離されています。

Host Protocol は意図的に VS Code から独立して設計されています。そのため、他の Editor や Tool も `bit8-vscode` に依存せず BIT8_CLI と統合できます。

より低レベルな Protocol の詳細については、技術ドキュメントを参照してください。

## Architecture

BIT8_CLI は Game Runtime と Editor Integration を分離しています。

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

`bit8 run` はネイティブ Desktop でゲームを実行します。

`bit8 host` は Editor やその他の Frontend が利用するインターフェースを提供します。

どちらも同じ BIT8_CLI Runtime を使用し、それぞれが別のゲームロジックを実装するわけではありません。

これにより BIT8_CLI は Editor-independent な構造を維持しながら、VS Code では統合された開発環境を提供できます。

## ドキュメント

より詳しい BIT8_CLI のドキュメントは、repository の `docs/` にあります。

以下も参照できます。

- `CHANGELOG.md`：0.1.0 の機能とバージョン変更の概要
- `BIT8_SPEC.md`：より詳細な技術仕様
- `docs/`：Runtime、Sprite、Node、開発 workflow などの詳細ドキュメント

README の目的は BIT8_CLI をすばやく理解できるようにすることであり、完全な技術仕様を置き換えることではありません。

## プロジェクトステータス

現在のバージョン：

**BIT8_CLI 0.1.0**

0.1.0 は BIT8_CLI の最初の公開リリースです。

BIT8_CLI はまだ開発初期段階にあるため、今後のバージョンでは API、ファイル形式、開発 workflow の一部が変更される可能性があります。

0.1.0 では、小さいながらも一通り揃った基盤として Runtime、Lua Script、Asset、Map、Node、Sprite、Animation、Collision、Camera、Editor Integration を構築することを重視しています。

## License

BIT8_CLI は MIT License のもとで公開されています。

Copyright (c) 2026 AKI
