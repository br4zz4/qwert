# QWERT

A dev environment manager. Declare the tools you need in `~/.qwert/config.yml`, save your dotfiles in a personal repo, and run `qwert apply` on any new machine to replicate the environment exactly.

## Install

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/oporpino/qwert/latest/scripts/install.sh)"
```

## Usage

```
qwert use <tool>         # declare + install + setup
qwert install <tool>     # declare + install (no setup)
qwert setup <tool>       # declare + run setup
qwert uninstall <tool>   # remove from config + uninstall
qwert drop <tool>        # full teardown: uninstall + undo setup
qwert apply              # sync all declared tools
qwert status             # show installed tools
qwert search <term>      # search recipes + yuiop (the package manager)
qwert upgrade <tool>     # upgrade a tool
qwert upgrade --all      # upgrade all tools
qwert recipes update     # sync the default recipe catalog
qwert plugin add <url>   # add your own recipes repo
qwert self upgrade       # upgrade qwert itself
qwert doctor             # check environment health
```

## Recipes & plugins

The default catalog lives in the [qwert-recipes](https://github.com/br4zz4/qwert-recipes)
repo and is synced with `qwert recipes update`. Add your own via `qwert plugin add <url>`
(any git repo with a `recipes/` directory) — see the [qwert-recipes README](https://github.com/br4zz4/qwert-recipes)
for how to build one.

## Config

`~/.qwert/config.yml` — the manifest for your environment:

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

Save `~/.qwert/` in a private repository. On a new machine, clone it and run `qwert apply`.

## How it works

- `~/.qwert/` — your dotfiles. Free-form, version-controlled in your personal repo.
- `~/.local/share/qwert/` — qwert runtime data (recipes, plugins, state, backups). Never edited manually.
- `/opt/qwert/bin/qwert` — the binary.

The default recipes are git-cloned to `~/.local/share/qwert/recipes/` from the
[qwert-recipes](https://github.com/br4zz4/qwert-recipes) repo. Each recipe can define install
steps, setup (symlinks, copies, commands), and undo behaviour.

Package installation delegates to [yuiop](https://github.com/br4zz4/yuiop), the universal
wrapper over `brew`/`apt`/`pacman`. qwert passes a tool's canonical name and yuiop resolves the
platform's package manager — qwert never maps a platform to a PM itself.

## MCP server (for AI agents)

`mcp/qwert-mcp.mjs` is a dependency-free stdio MCP server (~11 KB, no npm deps) that teaches
agents how to operate qwert safely. Register it in any MCP client (Claude Code, OpenCode, etc.):

```json
{ "qwert": { "type": "local", "command": ["node", "~/.qwert/mcp/qwert-mcp.mjs"], "enabled": true } }
```

Tools (prefix `qwert_`):

- **Read state & docs**: `qwert_state` (profile + list + doctor), `qwert_machine` (config.yml/machine.yml), `qwert_doctor`, `qwert_info {tool}`, `qwert_recipes`, `qwert_recipe_documentation {name}`, `qwert_skill`/`qwert_ops {intent}` (qwert-ops skill), `qwert_help`.
- **Gated execution**: `qwert_cli {args, confirm?}` — read-only subcommands run directly; mutating ones (`apply/setup/install/use/upgrade`) require `confirm: true` after human approval; `uninstall/drop/self` are never allowed (system packages are shared across users).

The server reads the machine state from `~/.qwert` and `~/.local/share/qwert` (override with `HOME`/`QWERT_DIR` env).
