BIT8_CLI 0.1.0 Multilingual Public README Update

Repository:
/Users/komizai.aki/project/BIT8-API

Public repository:
https://github.com/Akiaaaa20/Bit8_CLI

This is a documentation-only task.

BIT8_CLI 0.1.0 will provide four synchronized README languages:

1. English
2. Traditional Chinese (Taiwan)
3. Japanese
4. Korean

Do not modify Runtime, CLI, Language Service, extension behavior,
file formats, tests, schemas, or version numbers.

==================================================
1. ENGLISH README
==================================================

Rewrite and polish:

README.md

for the actual public BIT8_CLI 0.1.0 repository.

Remove obsolete pre-publication wording, especially:

"This repository is undergoing local final-release preparation;
no publication is implied."

The README should be medium-length.

It should be welcoming and useful to a new developer while retaining
important technical information about Bit8 0.1.0.

Do not turn README.md into a complete specification.

Use approximately this structure:

# Bit8 0.1.0

Language switcher

Short introduction

## Features

## Quick Start

Include:
- installation from source
- running a project
- controls

## A Tiny Bit8 Program

Provide a small valid .b8 example.

## Project Structure

Show a representative project layout.

## Tilesheets and Asset IDs

Explain:
- explicit PNG registration
- 8x8 tiles
- stable asset IDs such as A1/A2
- direct spr() remains available

## Maps

Explain:
- .b8map
- maximum 256x256 tiles
- map(), mget(), mset()
- framebuffer remains 64x64 game pixels
- a map is a world, not a framebuffer

## Nodes

Explain the Node model.

Include the design principle exactly:

"Main assembles the game. Nodes live the game."

## Sprites and Animation

Explain:
PNG -> stable tile IDs -> Sprite/Animation definition -> Node -> script

Include a small Node animation example using:

self:play(...)
self:move(...)
self:spr()

## Collision

Explain Solid tiles, Box Collider, self:collide(), and self:move()
at a concise user-facing level.

## Camera

Explain that the active Camera controls the world-space viewport.

## VS Code

Explain the Bit8 VS Code extension and its major 0.1.0 capabilities:

- syntax support
- completion
- hover
- diagnostics
- Map Workspace
- Sprite information
- Node editing
- asset registration
- BIT8 GAME view
- Run / Stop

Clearly state that VS Code is a frontend/client of Bit8,
not the Bit8 runtime itself.

## Host Protocol

Briefly explain:

bit8 host <project-path>

and its role as an editor-independent frontend interface.

Deep NDJSON protocol details should remain in technical documentation
rather than dominate the README.

## Architecture

Provide a concise conceptual overview of:

Game Project
    |
RuntimeSession
    |
+---+---+
|       |
bit8 run
bit8 host
|       |
Desktop Editors/
Window  Tooling

The exact Markdown/ASCII formatting may be improved.

## Documentation

Point readers to docs/, CHANGELOG.md, and other existing relevant
documentation.

Do not claim that a GitHub Wiki already contains content unless it
actually exists.

## Project Status

State:

Bit8 0.1.0 is the first public release.

Explain that Bit8 is still young and APIs/file formats may evolve in
future versions.

Do not mention unreleased 0.2.0 features.

## License

MIT License

Copyright (c) 2026 AKI

==================================================
2. FOUR-LANGUAGE SWITCHER
==================================================

Use the same four-language navigation at the top of every README.

README.md:

**English** | [繁體中文](README.zh-TW.md) | [日本語](README.ja.md) | [한국어](README.ko.md)

README.zh-TW.md:

[English](README.md) | **繁體中文** | [日本語](README.ja.md) | [한국어](README.ko.md)

README.ja.md:

[English](README.md) | [繁體中文](README.zh-TW.md) | **日本語** | [한국어](README.ko.md)

README.ko.md:

[English](README.md) | [繁體中文](README.zh-TW.md) | [日本語](README.ja.md) | **한국어**

==================================================
3. TRADITIONAL CHINESE README
==================================================

Create or update:

README.zh-TW.md

This must be a complete localization of the final English README,
not a shortened summary.

Use natural Taiwan Traditional Chinese.

Do not use Simplified Chinese.

Keep Bit8 product/API terminology in English where it improves
consistency with the actual UI and API.

Examples include:

Runtime
Node
Sprite
Animation
Camera
Asset ID
Map Workspace
Host Protocol
Language Service

Do not translate:

- commands
- API identifiers
- filenames
- file extensions
- TOML keys
- .b8 code
- asset IDs

Technical explanations should sound natural to a Taiwanese developer,
not like literal machine translation.

==================================================
4. JAPANESE README
==================================================

Create or update:

README.ja.md

This must be a complete localization of the final English README.

Use natural technical Japanese appropriate for software/game
development documentation.

Avoid awkward word-for-word translation.

Preserve commands, API identifiers, filenames, code, TOML keys,
asset IDs, and Bit8-specific product terminology where appropriate.

==================================================
5. KOREAN README
==================================================

Create:

README.ko.md

This must be a complete Korean localization of the same final English
README.

Use natural Korean technical writing appropriate for software and
game-development documentation.

Do not produce literal or machine-like translation.

Preserve Bit8 API names and technical identifiers where appropriate,
including:

Runtime
Node
Sprite
Animation
Camera
Asset ID
Map Workspace
Host Protocol
Language Service

Do not translate:

- commands
- API identifiers
- filenames
- file extensions
- TOML keys
- .b8 code
- asset IDs

==================================================
6. FOUR-LANGUAGE CONSISTENCY AUDIT
==================================================

After all four READMEs are complete, compare them section by section.

They must describe the same BIT8_CLI 0.1.0 product.

Verify all four consistently state:

- Rust runtime
- Lua 5.4 scripting
- .b8 scripting
- 64x64 framebuffer
- 8x8 tiles
- stable asset IDs
- map maximum 256x256 tiles
- fixed 30 Hz simulation
- Map Nodes
- Sprite definitions
- Animation
- Solid tile / Box Collider collision
- Camera
- Language Service
- VS Code integration
- BIT8 GAME
- editor-independent host protocol
- MIT License
- Copyright (c) 2026 AKI

Check all numeric limits carefully.

Do not accidentally translate code or API names differently between
languages.

Do not introduce features that are not part of 0.1.0.

==================================================
7. LINK AUDIT
==================================================

Check every relative Markdown link in all four README files.

Verify:

- language switcher links
- CHANGELOG.md
- docs/ links
- other referenced local documentation

Do not leave broken links.

Do not invent Wiki links or documentation files that do not exist.

==================================================
8. PUBLICATION BOUNDARY
==================================================

Do NOT:

- modify Runtime source
- modify CLI source
- modify Language Service source
- modify extension implementation
- modify tests
- modify schemas
- modify BIT8-RELEASE
- rebuild binaries
- rebuild VSIX
- change version numbers
- create v0.1.0 tag
- create GitHub Release

This task changes documentation only.

==================================================
9. GIT
==================================================

Review the diff before committing.

Confirm that only intended documentation files changed.

Commit with:

Add multilingual public README

Push the documentation commit to:

origin/main

Do not create or push any tag.

==================================================
10. REPORT
==================================================

Report:

A. English README changes
B. Traditional Chinese README status
C. Japanese README status
D. Korean README status
E. four-language consistency audit
F. link audit
G. files changed
H. commit hash
I. push result
J. final git status

End with exactly:

BIT8_CLI FOUR-LANGUAGE README READY
