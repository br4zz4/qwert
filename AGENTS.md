# AGENTS.md

This file provides guidance to Claude Code when working with this repository.

## What is QWERT

QWERT is a **dev environment manager** — not a package manager. The user declares what their machine should have in `~/.qwert/config.yml`, saves their dotfiles in a personal repo, and runs `qwert apply` on any new machine to replicate the environment exactly.

- `~/.qwert/config.yml` — the manifest: tools, stacks, hooks.
- `~/.qwert/` — the developer's personal dotfiles. Free-form directory, version-controlled in a personal git repo.
- `~/.local/share/qwert/` — qwert runtime data (recipes, completions, state, hooks, backups). Never edited by the user.
- `/opt/qwert/bin/qwert` — the binary.

**Package management** delegates to [`yuiop`](https://github.com/br4zz4/yuiop), the universal
wrapper over `brew`/`apt`/`pacman`. System-package recipes call `yuiop <verb> <canonical> --json`
via subprocess; qwert no longer resolves package names itself. Custom/GUI recipes stay in qwert.

No env vars needed. No shell config beyond what the installer writes.

## Architecture (v2 — Rust)

The current implementation is in Rust (`src/`). The original shell scripts are preserved in `v1/` with shims at the repo root.

```
src/
├── main.rs
├── cli.rs                  ← clap subcommands
├── commands/               ← one file per command
│   ├── apply.rs            ← installs + sets up declared tools, uninstalls orphans
│   ├── use_cmd.rs          ← declare + install + setup
│   ├── install_cmd.rs      ← declare + install only
│   ├── setup_cmd.rs        ← declare + run setup only
│   ├── uninstall_cmd.rs    ← remove from yml + uninstall
│   ├── drop_cmd.rs         ← full teardown: uninstall + undo setup
│   ├── status.rs
│   ├── info.rs             ← recipe details + install/setup status
│   ├── search.rs           ← searches qwert recipes + brew
│   ├── list.rs
│   ├── upgrade.rs
│   ├── reinstall.rs
│   ├── hook.rs             ← outputs shell hooks (init/end)
│   ├── completions.rs      ← outputs shell completion script
│   ├── self_cmd.rs         ← self upgrade/reinstall
│   ├── recipes_cmd.rs      ← recipes update (git clone/pull default catalog)
│   ├── plugin_cmd.rs       ← plugin add/remove/list/update
│   ├── doctor.rs
│   ├── config.rs
│   └── help.rs
├── plugins.rs              ← plugin clones (~/.local/share/qwert/plugins/), git ops
├── recipe/
│   ├── schema.rs           ← Recipe, RecipeMeta, RecipeKind, RecipeSetup, Commands
│   ├── index.rs            ← find/load_all — local > plugins > default cache
│   └── runner.rs           ← install/upgrade/uninstall/setup/undo_setup
├── adapters/               ← the yuiop bridge (subprocess + JSON parsing)
│   └── yuiop.rs            ← install/uninstall/upgrade/status/search/platform — all via `yuiop`
├── config/
│   ├── qwert_yml.rs        ← reads/writes ~/.qwert/config.yml
│   └── state_yml.rs        ← tracks what qwert installed (~/.local/share/qwert/state.yml)
├── platform/
│   ├── mod.rs              ← Platform enum, detect() (from yuiop), which(), run_cmd()
│   └── fs.rs               ← create_symlink(), copy_file()
└── ui/
    ├── printer.rs           ← ok/installing/failed/search_result/command
    └── colors.rs
```

## Commands

```
qwert use <tool>             # declare + install + setup
qwert use <tool> --no-install
qwert install <tool>         # declare + install (no setup)
qwert setup <tool>           # declare + run setup
qwert uninstall <tool>       # remove from qwert.yml + uninstall
qwert drop <tool>            # full teardown: uninstall + undo setup (with backup)
qwert apply                  # install + setup all declared tools, uninstall orphans
qwert apply <tool>
qwert status / status <tool>
qwert info <tool>            # recipe details, install status, setup status
qwert search <term>          # searches recipes + yuiop (the package manager)
qwert list
qwert upgrade / upgrade <tool>
qwert upgrade --all
qwert reinstall <tool>
qwert self upgrade
qwert self reinstall
qwert recipes update
qwert plugin add <url>       # declare + git clone a recipes repo
qwert plugin remove <name>
qwert plugin list
qwert plugin update
qwert hook prepare / hook init   # output shell hooks (eval'd in .zshrc)
qwert completions <shell>    # output completion script
qwert doctor
qwert config edit
qwert help
```

## Package management == yuiop

Every system-package operation goes through the `yuiop` binary as a subprocess
(`yuiop --json <verb> <canonical>`). qwert passes only the recipe's **canonical**
name; yuiop detects the platform, resolves the per-manager package
(`packages` table / embedded catalog), and runs brew/apt/pacman.

- `qwert` ships **no** brew/apt/pacman adapters anymore; `src/adapters/yuiop.rs`
  is the single bridge (spawns the binary, parses its JSON, reports errors).
- `platform::detect()` asks `yuiop platform --json` (cached per process). qwert
  never maps a platform to a package manager itself.
- `qwert platform <macos|debian|arch>` forwards the override to `yuiop platform
  <brew|apt|pacman>` (yuiop owns persistence: `~/.config/yuiop/config.yml`).
- If the `yuiop` binary is missing, package operations fail with an install hint.
  The qwert installer (`scripts/install.sh`) installs yuiop too.

## Recipe System

Recipes come from git repos, not from this repo. The default catalog is cloned from
`https://github.com/br4zz4/qwert-recipes` into `~/.local/share/qwert/recipes/`. Users can
add more via `qwert plugin add <url>` — plugins are cloned into `~/.local/share/qwert/plugins/<name>/`
and declared in `~/.qwert/config.yml` (versioned, so `apply` on a new machine restores them).

Each recipe is a directory with up to two files — both optional:

```
recipes/
└── tmux/
    ├── install.toml   ← install/upgrade/uninstall + meta
    └── setup.toml     ← symlinks, copies, or commands for config setup
```

Recipe lookup precedence: `~/.qwert/recipes` (local override) → plugins (declaration
order) → default catalog.

If only `setup.toml` exists, qwert synthesizes meta from the directory name and treats it as a package (installed via yuiop). If neither file exists, qwert falls back to `yuiop install <name>`.

### Types

| Type | Behavior |
|------|----------|
| `package` (no `type`) | Installed via yuiop — the platform's PM (brew/apt/pacman) |
| `brew`/`apt`/`pacman` | Legacy — treated as a package; yuiop resolves the per-manager name |
| `custom`/`qwert` | Custom commands in `[install]`, `[upgrade]`, `[uninstall]` sections |

### `install.toml`

```toml
[meta]
name = "tmux"
version = "1.0.0"
description = "Terminal multiplexer"
# sem `type` → package: yuiop instala no PM da plataforma
depends = []           # other recipe names to install first
packages = { brew = "tmux", apt = "tmux", pacman = "tmux" }  # optional per-PM names; default: meta.name
platforms = ["macos"]  # optional allowlist: macos|debian|arch|linux (linux = debian+arch)

[check]
command = "tmux"
version_flag = "-V"

# Only needed for type = "custom"/"qwert" or cross-platform fallback
[install]
macos = "custom install command"
debian = ["step one", "step two"]
linux = "same command for debian and arch"
```

**Platform restriction (`platforms`)**: optional allowlist in `[meta]`. When absent, the
recipe works on every platform. Values: `macos`, `debian`, `arch`, or `linux` (alias for
debian+arch). When the current platform isn't in the list, `qwert apply` skips the tool
with a warning (counted as "skipped" in the summary, never a failure).

**`linux` command sections**: `[install.linux]`, `[setup.linux]`, `[upgrade.linux]`,
`[uninstall.linux]`, `[undo.linux]` run on both debian and arch. A platform-specific
section (`arch`/`debian`) takes precedence over `linux` when both exist.

### `setup.toml`

```toml
# symlink: ~/.tmux.conf → ~/.qwert/tmux (undo = remove symlink)
dest = "~/.tmux.conf"
symlink = true
# src optional — defaults to ~/.qwert/<name>

# commands: run on setup (undo = [undo] section)
dest = "~/.qwert/iterm2"
macos = ["defaults write com.googlecode.iterm2 PrefsCustomFolder -string ~/.qwert/iterm2"]

[undo]
macos = ["defaults delete com.googlecode.iterm2 PrefsCustomFolder"]
```

**Setup types and undo behaviour:**
- `symlink = true` — undo removes the symlink. If the destination already exists and
  is not a qwert-managed symlink, qwert backs it up to
  `~/.local/share/qwert/backups/<name>/` (timestamped) and proceeds.
- copy (dest exists, no symlink) — undo backs up to `~/.local/share/qwert/backups/<name>/` then removes
- commands — undo runs `[undo]` section; warns if not defined
- A setup with no commands for the current platform and no source declared in
  `profiles.<profile>.configs.<tool>` is **skipped** (not a failure) — `qwert apply`
  counts it as "skipped" in the summary.

### Package recipes

For `package` recipes (no `type`, or legacy `brew`/`apt`/`pacman`), **do not write
`[install]`/`[upgrade]`/`[uninstall]` sections** — yuiop derives the command from the
canonical name (or `packages.<pm>`). Explicit sections are only for `custom` recipes
or platform fallback.

## config.yml schema

```yaml
tools:
  - tmux
  - lvim

plugins:
  - name: my-recipes
    url: https://github.com/user/my-recipes

hooks:
  prepare:
    - ~/.qwert/zsh/prepare.sh
  init:
    - ~/.qwert/zsh/init.sh
```

`plugins` is managed by `qwert plugin add/remove` — each entry is a git recipes repo
cloned to `~/.local/share/qwert/plugins/<name>/`.

## State tracking

`~/.local/share/qwert/state.yml` records which tools qwert has installed. Used by `apply` to detect orphans (tools removed from `config.yml` since last apply) and uninstall them.

## Platform detection

Platform detection lives in `yuiop`, not qwert. `platform::detect()` returns
`Platform::MacOS`, `Platform::Debian`, `Platform::Arch`, or `Platform::Unknown` by
asking `yuiop platform --json`. qwert never maps an OS to a package manager — it
only needs the platform to pick the custom-recipe/setup section (`macos`/`debian`/`arch`).

## Testing

Follow `.project/ai/commands/test.md` for test conventions.

Tests follow the triple-A pattern (`// arrange`, `// act`, `// assert`). They live
in `#[cfg(test)] mod tests` — inline for new modules, or via `#[path]` for older ones.

Run tests: `make t`

## Development

```bash
make t        # cargo test
make build    # cargo build --release
```

Dependencies: `clap 4`, `serde + serde_yml`, `toml`, `anyhow`, `dirs`

## v1 (shell scripts)

The original shell implementation is in `v1/`. Root-level `bin/qwert` and `run` are shims that delegate to `v1/`. This keeps existing `~/.qwert` installations working while v2 is developed.
