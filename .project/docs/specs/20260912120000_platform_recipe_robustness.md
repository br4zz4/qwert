---
title: Restrição de plataforma em recipes e robustez do `apply`
status: proposed
created: 2026-09-12
updated: 2026-09-12
owner: "@gporpino"
certainty: high
---

# Restrição de plataforma em recipes e robustez do `apply`

> **TLDR**: Recipes ganham campo `platforms` para declarar onde funcionam; comandos ganham alias `linux`; `apply` ganha resumo melhorado, skip de ferramentas não-suportadas, backup automático em conflitos, e mensagens de erro mais claras. Recipes com `type=brew` sem suporte cross-platform são corrigidos. yuiop ganha provider AUR.

## Contexto

Ao rodar `qwert apply` em Arch Linux com perfil `dev`, 6 erros ocorreram:

| Ferramenta | Erro | Causa raiz |
|---|---|---|
| iterm2 | "no install steps for Arch Linux" | Recipe macOS-only, sem declaração de plataforma |
| claude | "yuiop: no knowledge of package 'claude'" | Sem install.toml, fallback yuiop não conhece |
| opencode | "yuiop: no knowledge of package 'opencode'" | `type=brew` sem mapping cross-platform |
| codex | "target not found: codex" | `type=brew` sem mapping cross-platform |
| powerlevel10k | "setup needs a source" | Setup só tem comandos macOS |
| neovim | "~/.config/nvim already exists and is not a symlink" | Diretório pré-existente não gerenciado pelo qwert |

Além disso, o resumo final (`install: 8/12 done • 4 failed`) não distingue falhas reais de ferramentas não-suportadas na plataforma.

## Objetivos

- **Plataforma**: recipes podem declarar `platforms = ["macos"]` no `[meta]`; qwert pula ferramentas não-suportadas com warning no resumo. Se omitido, funciona em todas as plataformas. Valores aceitos: `"macos"`, `"debian"`, `"arch"`, `"linux"` (alias para debian+arch). Valores desconhecidos são tratados como "plataforma não corresponde" → skip.
- **Alias `linux`**: seções `[install.linux]`, `[setup.linux]` etc. servem para debian e arch. Ordem de resolução: comando específico da plataforma (`arch`/`debian`) tem precedência sobre `linux`. Se nem específico nem `linux` existir, retorna vazio.
- **Backup automático**: quando um setup de symlink/copy encontra o destino já existente e não é um symlink gerenciado pelo qwert (apontando para o source correto), faz backup em `~/.local/share/qwert/backups/<tool>/YYYYMMDDHHMMSS/` e prossegue. Se o destino já é symlink correto → `AlreadyInstalled`. Se é symlink para outro lugar → também faz backup.
- **Resumo melhorado**: três contadores (`X installed • Y failed • Z skipped`) + seção nominal `Skipped (unsupported platform):` listando nomes e motivos
- **Setup sem comandos**: se o recipe não tem comandos para a plataforma atual e não há source declarado, pula com warning (não erro)
- **Mensagens melhores**: "yuiop: no knowledge of package 'X'" → "package 'X' not available for Arch Linux (pacman)"
- **Recipes corrigidos**: iterm2 ganha `platforms = ["macos"]`; claude e codex viram `type=qwert` com `npm install -g`; powerlevel10k usa alias `linux` e ganha setup linux; opencode permanece `type=package` e entra no catalog yuiop
- **yuiop**: novo provider AUR; opencode adicionado ao catalog pacman

## Fora de escopo

- yuiop como gerenciador universal (npm, pip, cargo) — visão de futuro, não desta spec
- Instalação automática de helpers AUR (yay/paru) — o provider AUR do yuiop sugere instalar, não instala automaticamente
- Flag `--strict` para transformar skip em erro
- Substituição de recipes existentes que funcionam (ex: tmux, fzf, neovim-install)

## Mudanças

### qwert (`src/`)

| Arquivo | Mudança |
|---|---|
| `src/recipe/schema.rs` | `RecipeMeta` ganha `platforms: Option<Vec<String>>` com valores `macos`/`debian`/`arch`/`linux`. `Commands` ganha campo `linux: Option<Commands>`. `platform_cmds()` aceita `"linux"` como fallback para `arch` e `debian`. |
| `src/recipe/runner.rs` | `install()` verifica `meta.platforms` antes de executar; retorna `NotSupported` se plataforma atual não está na lista. `setup()` idem. `install_steps_for()` e `setup_cmds_for()` aceitam fallback `linux` (depois do específico, antes de retornar vazio). `run_setup_section()`: se destino já existe e não é symlink gerenciado, faz backup automático com timestamp em `~/.local/share/qwert/backups/<name>/`. Se sem comandos para a plataforma e sem source, retorna `NotSupported` (não `Failed`). |
| `src/commands/apply.rs` | Coleta `NotSupported` da fase de install e setup. Resumo final: `install: X installed • Y failed • Z skipped` com seção `Skipped (unsupported platform):` listando nomes. |
| `src/ui/printer.rs` | Nova função `skipped(name, reason)` para output `⚠️ <name>: <reason> — skipping`. |
| `src/adapters/yuiop.rs` | Mensagem de erro traduzida: `yuiop: no knowledge of package 'X'` → `package 'X' not available for <platform> (<pm>). Install it manually or add a custom recipe.` |
| `src/platform/fs.rs` | Nova função `backup_and_remove(path, backup_dir)` — move o arquivo/diretório para backup com timestamp antes de sobrescrever. |

### qwert-recipes

| Recipe | Mudança |
|---|---|
| `iterm2/install.toml` | Adicionar `platforms = ["macos"]` no `[meta]` |
| `claude/install.toml` | **Criar arquivo.** `type = "qwert"`, `[install]` com `linux = "npm install -g @anthropic-ai/claude-code"` e `macos = "npm install -g @anthropic-ai/claude-code"`. `[check]` com `command = "claude"`, `version_flag = "--version"`. |
| `codex/install.toml` | Mudar `type` para `"qwert"`. `[install]` com `linux = "npm install -g @openai/codex"` e `macos = "npm install -g @openai/codex"`. Remover `pkg` e `type = "brew"`. |
| `powerlevel10k/install.toml` | Substituir `debian` por `linux` no `[install]`. `[check]` usar `command = "git"` com verificação do clone (fallback para `which`). |
| `powerlevel10k/setup.toml` | Adicionar seção `linux` espelhando `macos` mas usando path do git clone (`~/.oh-my-zsh/custom/themes/powerlevel10k/powerlevel10k.zsh-theme`) em vez de `brew --prefix`. |
| `opencode/install.toml` | Manter `type = "brew"`. Remover `pkg`. Confiar no yuiop (que terá opencode no catalog pacman após esta spec). |

### yuiop

| Mudança |
|---|
| Novo provider AUR em `internal/provider/aur.go`: detecta `yay` ou `paru`, implementa `install`/`upgrade`/`remove`/`search`/`status` via helper detectado. Fallback: sugere instalar `yay`. |
| Catalog pacman: adicionar entrada `opencode` → `opencode`. |
| Registrar provider `aur` no factory. |

## Como verificar

### qwert

1. **Testes unitários** (TDD):
   - `schema.rs`: `platforms` parse de TOML, `linux` fallback no `platform_cmds()`, `setup_cmds_for()` com alias `linux`
   - `runner.rs`: `install()` retorna `NotSupported` para plataforma não-listada, `install()` com `platforms` omitido funciona normal, `install()` com `platforms = ["macos"]` em Arch retorna `NotSupported`, `setup()` idem. Backup automático: destino pré-existente é movido para backup, symlink criado em seguida. Setup sem comandos nem source retorna `NotSupported` (não `Failed`)
   - `apply.rs`: contadores e seção skipped no resumo
   - `printer.rs`: função `skipped()`
   - `adapters/yuiop.rs`: mensagem de erro traduzida
2. **Integração**: `make t` passa sem regressões
3. **Manual**: em máquina Arch Linux, `qwert apply` com perfil `dev`:
   - `iterm2` aparece como `⚠️ skipped (unsupported platform: macos only)`
   - `claude`, `codex` instalam via npm
   - `powerlevel10k` faz setup completo (hooks + symlink p10k.zsh)
   - `neovim` faz backup de `~/.config/nvim` existente e cria symlink
   - Resumo final: `install: 10 installed • 0 failed • 2 skipped` com seção `Skipped`

### qwert-recipes

1. `qwert info iterm2` em Arch mostra "not supported on this platform"
2. `qwert install claude` em Arch instala via npm
3. `qwert install codex` em Arch instala via npm
4. `qwert setup powerlevel10k` em Arch cria hooks e symlink

### yuiop

1. `yuiop --json install opencode-bin` via AUR instala com yay
2. `yuiop --json status opencode-bin` retorna `installed: true/false`
3. `yuiop --json install opencode` via pacman instala opencode

## Documentação

- Atualizar `AGENTS.md` com a nova seção `platforms` no schema de recipes
- Criar `.project/docs/learnings/platform-support.md` documentando a decisão de design do alias `linux` e do campo `platforms`

---

## Apêndice A — Prompt para o agente yuiop (provider AUR)

```
Adicione um provider AUR ao yuiop (`internal/provider/aur.go`) seguindo o
mesmo contrato dos providers existentes (brew, apt, pacman). Requisitos:

1. **Detecção de helper**: procure `yay` ou `paru` no PATH. Se nenhum
   existir, retorne erro com mensagem sugerindo `sudo pacman -S --needed
   git base-devel && git clone https://aur.archlinux.org/yay.git && cd yay
   && makepkg -si`.

2. **Interface**: implemente os métodos do contrato de provider:
   - `Install(pkg string) error` → `<helper> -S --noconfirm <pkg>`
   - `Upgrade(pkg string) error` → `<helper> -S --noconfirm <pkg>`
   - `Remove(pkg string) error` → `<helper> -R --noconfirm <pkg>`
   - `Status(pkg string) (bool, error)` → `<helper> -Q <pkg>` (exit 0 = installed)
   - `Search(term string) ([]string, error)` → `<helper> -Ss <term>`, parse output
   - `Name() string` → `"aur"`

3. **Registro**: adicione `aur` ao factory de providers em
   `internal/provider/` para que `yuiop --json <verb> <pkg>` resolva via
   AUR quando o provider for `aur`.

4. **Catalog pacman**: adicione entrada `opencode` no catalog do pacman
   (nome canônico → nome do pacote: `opencode` → `opencode`). Isso não é
   AUR — é pacman nativo. A ferramenta `opencode` está nos repositórios
   oficiais do Arch.

5. **Testes**: testes unitários mockando `yay`/`paru` (use interface para
   execução de comandos, injetável). Cenários: helper encontrado/ausente,
   install/upgrade/remove com sucesso, status true/false, search com/sem
   resultados.

6. **JSON output**: siga o mesmo formato dos providers existentes:
   `{"platform":"aur","package":"<name>","installed":true/false}` para
   status; `{"platform":"aur","term":"<term>","matches":[...]}` para
   search.

Siga as convenções do projeto (Go, mesma estrutura de `brew.go`/`apt.go`/
`pacman.go`). Não quebre os providers existentes.
```