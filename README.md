# ZZLS - Zig Language Server

A fast Zig language server written in Rust, targeting Zig 0.16.x.

## Disclaimer;
This is AI slop. Do not be fooled by the readme that this is actually functioning, it is slop and should not be taken seriously nor used by anyone in any real context.

## Features

- **Diagnostics** via `zig ast-check` (fast syntax) and `zig build-exe -fno-emit-bin` (full semantic)
- **Format on save** via `zig fmt --stdin`
- **Format on demand** via `textDocument/formatting` and `textDocument/rangeFormatting`
- **Completion** (basic keyword completions, more to come)
- **Document symbols** (`fn`, `struct`, `enum`, `const`, `var` declarations)
- **Go to definition/declaration**
- **Find references**
- **Rename**
- **Code actions**
- **Pretty terminal diagnostics** via [ariadne](https://github.com/ariadne-lang/ariadne) (CLI mode)

## Installation

```bash
cargo install --path .
```

Make sure `zig` is in your PATH, or configure the path in your editor settings.

## Usage

### LSP Server (default)

```bash
zzls --stdio
```

### CLI Check

```bash
zzls check path/to/file.zig
```

### CLI Format

```bash
# Format a file in-place
zzls format path/to/file.zig

# Check if formatting is needed (exit code 1 if changes needed)
zzls format --check path/to/file.zig
```

## Neovim Setup

ZZLS works with Neovim's native LSP client (`vim.lsp`). Add the config file `lsp/zzls.lua` to your Neovim config directory:

```lua
-- ~/.config/nvim/lsp/zzls.lua
---@type vim.lsp.Config
return {
  cmd = { 'zzls', '--stdio' },
  filetypes = { 'zig', 'zon' },
  root_markers = { 'build.zig', 'build.zig.zon', '.git' },
  settings = {
    zzls = {
      zig_path = nil, -- auto-detect from PATH
      format_on_save = true,
      diagnostics_on_save = true,
      diagnostics_debounce_ms = 200,
    },
  },
}
```

Then in your LSP setup:

```lua
vim.lsp.enable({ 'zzls' })
```

### Keymaps

ZZLS integrates with your existing LSP keymaps:

| Key | Mode | Action |
|-----|------|--------|
| `K` | n | Hover documentation |
| `<C-k>` | i | Signature help |
| `gd` | n | Go to definition |
| `gD` | n | Go to declaration |
| `<leader>rn` | n | Rename symbol |
| `<leader>la` | n | Code actions |
| `<leader>lf` | n | Format document |
| `gr` | n | Find references |
| `<leader>ls` | n | Document symbols |

## Configuration

Settings can be configured via `vim.lsp.config('zzls', { settings = { ... } })`:

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `zig_path` | string? | nil | Path to zig binary (auto-detect from PATH if nil) |
| `format_on_save` | bool | true | Format file on save |
| `diagnostics_on_save` | bool | true | Run diagnostics on save |
| `diagnostics_on_change` | bool | false | Run diagnostics on every change |
| `diagnostics_debounce_ms` | number | 200 | Debounce delay for diagnostics |

## Architecture

```
src/
├── main.rs           # CLI entry point (server/check/format subcommands)
├── server.rs         # LSP server implementation (LanguageServer trait)
├── document.rs       # Document/buffer management with ropey
├── workspace.rs      # Workspace/project detection
├── config.rs         # Configuration handling
├── bridge/
│   ├── mod.rs
│   ├── compiler.rs   # Zig compiler integration (ast-check + build-exe)
│   └── formatter.rs  # zig fmt integration
└── diagnostics/
    ├── mod.rs        # Diagnostic types and LSP conversion
    └── pretty.rs     # Ariadne-based pretty printing
```

## Dependencies

- [tower-lsp-server](https://github.com/Calastrophe/tower-lsp-server) - LSP framework
- [ls-types](https://github.com/nicorusti/ls-types) - LSP type definitions
- [ropey](https://github.com/cessen/ropey) - Rope data structure for text editing
- [tokio](https://tokio.rs/) - Async runtime
- [ariadne](https://github.com/ariadne-lang/ariadne) - Pretty error reporting
- [clap](https://docs.rs/clap) - CLI argument parsing

## License

MIT
