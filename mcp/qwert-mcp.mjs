#!/usr/bin/env node
// qwert-mcp — stdio MCP server that teaches agents how to operate qwert.
// No dependencies. Flat buffers + JSON-RPC 2.0 over newline-delimited JSON.
// Start: node qwert-mcp.mjs  (HOME/QWERT_DIR select the user's dotfiles)

import { readFileSync, readdirSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const HOME = process.env.HOME || homedir();
const QWERT_DIR = process.env.QWERT_DIR || join(HOME, ".qwert");
const DATA_DIR = join(HOME, ".local/share/qwert");
const SKILL_PATH = join(QWERT_DIR, "config/agents/skills/qwert-ops/SKILL.md");

function readFile(p) {
  try {
    return readFileSync(p, "utf8");
  } catch {
    return null;
  }
}

function machineProfile() {
  const raw = readFile(join(DATA_DIR, "machine.yml"));
  if (!raw) return null;
  const m = raw.match(/^profile:\s*\S+/m);
  return m ? m[0].split(":")[1].trim() : null;
}

// Minimal TOML reader for recipe files: [section] headers + scalar keys (no tables).
function parseToml(text) {
  const out = { _sections: [] };
  let section = null;
  for (const line of text.split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const sm = t.match(/^\[([^\]]+)\]$/);
    if (sm) {
      const name = sm[1].trim();
      section = name;
      out._sections.push(name);
      continue;
    }
    const km = t.match(/^([A-Za-z0-9_-]+)\s*=\s*(.+)$/);
    if (!km) continue;
    if (section) {
      out[section] ||= {};
      let v = km[2].trim().replace(/^["']|["']$/g, "");
      out[section][km[1]] = v;
    } else {
      let v = km[2].trim().replace(/^["']|["']$/g, "");
      out[km[1]] = v;
    }
  }
  return out;
}

function recipeDirs() {
  const dirs = [join(QWERT_DIR, "recipes")]; // local overrides win
  dirs.push(join(DATA_DIR, "recipes")); // default catalog
  try {
    for (const n of readdirSync(join(DATA_DIR, "plugins"))) {
      dirs.push(join(DATA_DIR, "plugins", n, "recipes"));
    }
  } catch {}
  // keep only dirs that exist
  return dirs.filter((d) => existsSync(d));
}

function loadRecipe(name) {
  for (const base of recipeDirs()) {
    const dir = join(base, name);
    try {
      readdirSync(dir);
    } catch {
      continue;
    }
    return {
      _dir: dir,
      origin: base,
      install: readFile(join(dir, "install.toml")),
      setup: readFile(join(dir, "setup.toml")),
    };
  }
  return null;
}

function listRecipes() {
  const names = new Set();
  for (const base of recipeDirs()) {
    try {
      for (const n of readdirSync(base)) {
        if (!n.startsWith(".")) names.add(n);
      }
    } catch {}
  }
  const out = [];
  for (const name of [...names].sort()) {
    const r = loadRecipe(name);
    if (!r) continue;
    const meta = r.install ? parseToml(r.install) : parseToml(r.setup || "");
    out.push({
      name,
      description: meta.meta?.description || "",
      type: meta.meta?.type || "package",
      platforms: meta.meta?.platforms || "all",
      depends: meta.meta?.depends || [],
    });
  }
  return out;
}

function profilesOfConfig() {
  const yml = readFile(join(QWERT_DIR, "config.yml")) || "";
  const out = {};
  let cur = null;
  for (const line of yml.split(/\r?\n/)) {
    const pm = line.match(/^  ([A-Za-z0-9_-]+):\s*$/);
    if (pm) {
      cur = pm[1];
      out[cur] = [];
      continue;
    }
    const tm = line.match(/^    - ([A-Za-z0-9_.-]+)$/);
    if (tm && cur) out[cur].push(tm[1]);
  }
  return out;
}

const dispatch = {
  qwert_help(args) {
    const all = `qwert manual — dev environment manager (wraps yuiop for packages).

Core lifecycle:
  use <tool>        declare + install + setup (full)
  install <tool>    declare + install only
  setup <tool>      declare + run setup only
  uninstall <tool>  remove declaration + uninstall
  drop <tool>       full teardown: uninstall + undo setup (backup)
  apply             install + setup ALL declared tools; uninstalls orphans (careful: system pkgs!)
  apply <tool>      apply one tool
  status [tool]     what is declared/installed
  info <tool>       recipe details + status + which profiles declare it
  list [--all]      declared tools (active profile only; --all = union)
  search <term>     search recipes + yuiop catalog
  upgrade [tool]    upgrade one, --all for all
  reinstall <tool>  re-run install
  recipes update    sync default catalog (git pull in ~/.local/share/qwert/recipes)
  plugin add|remove|list|update   manage recipe repos
  hook prepare|init shell hook text (eval in .zshrc)
  profile [name]    show/set THIS machine's profile (state: ~/.local/share/qwert/machine.yml)
  platform [x]      show/set platform override (yuiop: macos|debian|arch)
  doctor            health check (config dir, yml, recipes, profile)
  self upgrade      upgrade qwert binary itself
  config edit       open ~/.qwert/config.yml in $EDITOR
  completions <sh>  shell completion script

Key concepts:
- ~/.qwert/config.yml: tools + profiles + configs + hooks (the repo = dotfiles).
- Profiles (personal/work/root/server): named sets of tools; "machine profile"
  selects what apply manages. apply UNINSTALLS declarations not in the active
  profile ("orphans") — user-local installs are removed; PACMAN/brew packages
  are removed too, so switching profiles across users sharing system packages
  needs care.
- Recipes: ~/.qwert/recipes (local override) > plugins > catalog
  (~/.local/share/qwert/recipes). install.toml = install/uninstall/check;
  setup.toml = symlinks/commands for config.
${
        args?.command
          ? "\nFor a specific command, prefer running `qwert " +
            args.command +
            " --help` and `qwert info <tool>` on the real machine for live state."
          : ""
      }`;
    return { text: all };
  },

  qwert_recipes() {
    return { recipes: listRecipes(), machine_profile: machineProfile() };
  },

  qwert_recipe_documentation(args) {
    const r = loadRecipe(args?.name);
    if (!r) return { error: `recipe '${args?.name}' not found (local ~/.qwert/recipes > plugins > catalog)` };
    const meta = r.install ? parseToml(r.install) : parseToml(r.setup || "");
    return {
      name: args.name,
      origin: r.origin,
      type: meta.meta?.type || "package",
      description: meta.meta?.description,
      platforms: meta.meta?.platforms || "(any)",
      install_toml: r.install || "(no install.toml — package fallback via yuiop)",
      setup_toml: r.setup || "(no setup.toml)",
    };
  },

  qwert_machine() {
    return {
      home: HOME,
      profile: machineProfile(),
      profiles: Object.keys(profilesOfConfig()),
      config_yml: readFile(join(QWERT_DIR, "config.yml")),
      machine_yml: readFile(join(DATA_DIR, "machine.yml")),
    };
  },

  qwert_skill() {
    const md = readFile(SKILL_PATH);
    if (!md) return { error: `skill not found at ${SKILL_PATH}` };
    return { path: SKILL_PATH, content: md };
  },

  qwert_ops(args) {
    const md = readFile(SKILL_PATH);
    if (!md) return { error: "skill missing; intent: hardcoded fallback" };
    const wanted = (args?.intent || "").toLowerCase();
    const sections = md.split(/^## /m).slice(1);
    if (wanted) {
      const hits = sections.filter((s) => {
        const title = s.split("\n")[0].toLowerCase();
        return wanted.split(/\s+/).some((w) => w.length > 2 && (title.includes(w) || s.toLowerCase().includes(w)));
      });
      if (hits.length) return { content: "## " + hits.join("## ") };
    }
    return { content: md };
  },
};

const result = (id, r) => console.log(JSON.stringify({ jsonrpc: "2.0", id, result: r }));
const isErr = (id, msg) =>
  console.log(JSON.stringify({ jsonrpc: "2.0", id, error: { code: -32602, message: msg } }));

let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let idx;
  while ((idx = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, idx).trim();
    buffer = buffer.slice(idx + 1);
    if (!line) continue;
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      continue;
    }
    handle(msg);
  }
});

function handle(msg) {
  const method = msg.method || "";
  if (/^notifications/.test(method)) return;
  switch (method) {
    case "initialize": {
      result(msg.id, {
        protocolVersion: "2024-11-05",
        capabilities: { tools: {} },
        serverInfo: { name: "qwert", version: "1.0.0" },
        instructions:
          "Use these tools to learn how qwert works before touching a qwert machine: read qwert_skill/qwert_ops first, then qwert_machine for live state. NEVER run uninstall/drop/apply without confirming which packages disappear (system packages are shared across users).",
      });
      return;
    }
    case "ping":
      result(msg.id, {});
      return;
    case "tools/list":
      result(msg.id, {
        tools: [
          {
            name: "qwert_skill",
            description:
              "Full qwert-ops skill (Markdown): intents, workflows, safety rules for operating qwert. READ THIS FIRST before any qwert action.",
            inputSchema: { type: "object", properties: {} },
          },
          {
            name: "qwert_ops",
            description: "qwert-ops skill filtered by intent (e.g. 'bootstrap', 'profile', 'recipe', 'hooks')",
            inputSchema: { type: "object", properties: { intent: { type: "string" } } },
          },
          {
            name: "qwert_help",
            description: "QWERT command manual (all subcommands + concepts); pass {command} for emphasis",
            inputSchema: { type: "object", properties: { command: { type: "string" } } },
          },
          {
            name: "qwert_machine",
            description: "Live machine state: profile, declared profiles + tools, machine.yml, config.yml",
            inputSchema: { type: "object", properties: {} },
          },
          {
            name: "qwert_recipes",
            description: "List available recipes (local > plugins > catalog) with type/platforms/deps",
            inputSchema: { type: "object", properties: {} },
          },
          {
            name: "qwert_recipe_documentation",
            description: "Full install.toml + setup.toml of a recipe (read before installing any tool)",
            inputSchema: { type: "object", properties: { name: { type: "string" } }, required: ["name"] },
          },
        ],
      });
      return;
    case "tools/call": {
      const name = msg.params?.name;
      const args = msg.params?.arguments || {};
      if (!dispatch[name]) {
        isErr(msg.id, `unknown tool: ${name}`);
        return;
      }
      try {
        const r = dispatch[name](args);
        const body = JSON.stringify(r, null, 2);
        result(msg.id, { content: [{ type: "text", text: body }] });
      } catch (e) {
        result(msg.id, { content: [{ type: "text", text: `error: ${e.message}` }], isError: true });
      }
      return;
    }
    default:
      console.log(
        JSON.stringify({ jsonrpc: "2.0", id: msg.id, error: { code: -32601, message: `method not found` } }),
      );
  }
}
