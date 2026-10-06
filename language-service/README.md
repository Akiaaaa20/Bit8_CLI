# Bit8 Language Service 0.0.1

This is an experimental, editor-independent Language Server Protocol service.
It communicates over stdio and does not depend on VS Code APIs.

Install from the repository root:

```sh
cargo install --path language-service --locked
```

Editors that support LSP can launch `bit8-language-server` with stdio transport
for documents assigned to their Bit8 language mode. The VS Code extension does
this automatically for its `bit8` language mode.

The service synchronizes open documents and validates syntax by applying the
same `func name(...)` translation used by the runtime, then compiling the
result with Lua 5.4 without executing it. It also provides concise completion
and hover information for the implemented Bit8 Runtime API, only for documents
using the `bit8` language ID. It intentionally does not validate Lua semantics
or unknown globals.

An unrelated Lua language extension may still publish diagnostics for files
with a `.lua` extension; native `.b8` documents use the Bit8 language ID and
are handled by this service instead.
