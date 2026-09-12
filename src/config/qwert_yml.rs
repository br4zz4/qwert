use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Profile name used when a legacy config (top-level `tools:`) is loaded.
pub const PROFILE_DEFAULT: &str = "default";

/// Backward-compatible tool entry: simple version string or full config object.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ToolEntry {
    Simple(String),
    Full(ToolConfig),
}

impl ToolEntry {
    // Retained for the declared version pin (used by tests and future upgrade logic).
    #[allow(dead_code)]
    fn version(&self) -> &str {
        match self {
            ToolEntry::Simple(v) => v.as_str(),
            ToolEntry::Full(c) => c.version.as_str(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolConfig {
    #[serde(default = "default_version")]
    pub version: String,
    pub setup: Option<InlineSetup>,
}

fn default_version() -> String {
    "latest".into()
}

/// Inline setup defined in config.yml — mirrors RecipeSetup without importing recipe module.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InlineSetup {
    pub from: Option<String>,
    pub to: String,
    #[serde(default)]
    pub symlink: bool,
    pub macos: Option<StringOrList>,
    pub debian: Option<StringOrList>,
    pub arch: Option<StringOrList>,
    pub linux: Option<StringOrList>,
    pub undo: Option<InlineUndo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InlineUndo {
    pub macos: Option<StringOrList>,
    pub debian: Option<StringOrList>,
    pub arch: Option<StringOrList>,
    pub linux: Option<StringOrList>,
}

/// A single command string or an ordered list of commands (mirrors Commands in schema.rs).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrList {
    One(String),
    Many(Vec<String>),
}

impl StringOrList {
    #[allow(dead_code)]
    pub fn as_steps(&self) -> Vec<&str> {
        match self {
            StringOrList::One(s) => vec![s.as_str()],
            StringOrList::Many(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// Hooks for a single profile (flat, ordered): shell scripts to source per phase.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Hooks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prepare: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub init: Vec<String>,
}

fn hooks_empty(h: &Hooks) -> bool {
    h.prepare.is_empty() && h.init.is_empty()
}

/// A user-declared recipes source (plugin): a git repo cloned into the runtime cache.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginSource {
    /// Local name — derived from the URL at `qwert plugin add` time.
    pub name: String,
    /// Git URL of the recipes repo.
    pub url: String,
}

/// Plugin list empty? Used to skip serializing the field when unused.
pub fn plugins_empty(plugins: &[PluginSource]) -> bool {
    plugins.is_empty()
}

/// A single profile: which catalog tools it uses, its config sources and hooks.
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Profile {
    /// Catalog tool names, in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,

    /// tool → source path for the recipe's symlink/copy setup. The config.yml is
    /// the source of truth for where each dotfile lives; recipes are source-less.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub configs: IndexMap<String, String>,

    #[serde(default, skip_serializing_if = "hooks_empty")]
    pub hooks: Hooks,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(from = "QwertConfigRaw", into = "QwertConfigRaw")]
pub struct QwertConfig {
    /// Catalog of tools this setup knows about: name → version spec.
    pub tools: IndexMap<String, ToolEntry>,
    /// profile name → profile (tools, configs, hooks).
    pub profiles: IndexMap<String, Profile>,
    /// Declared recipe plugins (git repos), in add order.
    pub plugins: Vec<PluginSource>,
}

/// Raw shape used for (de)serialization. Handles both the new catalog+profiles
/// form and the legacy flat `tools:`-only form (mapped to a "default" profile).
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
struct QwertConfigRaw {
    #[serde(default)]
    tools: IndexMap<String, ToolEntry>,

    #[serde(default)]
    profiles: IndexMap<String, Profile>,

    #[serde(default, skip_serializing_if = "hooks_empty")]
    hooks: Hooks,

    #[serde(default, skip_serializing_if = "plugins_empty")]
    plugins: Vec<PluginSource>,
}

impl From<QwertConfigRaw> for QwertConfig {
    fn from(raw: QwertConfigRaw) -> Self {
        if !raw.profiles.is_empty() {
            QwertConfig { tools: raw.tools, profiles: raw.profiles, plugins: raw.plugins }
        } else {
            // Legacy flat config: catalog = all tools, active through a default profile.
            let names: Vec<String> = raw.tools.keys().cloned().collect();
            let mut profiles = IndexMap::new();
            profiles.insert(
                PROFILE_DEFAULT.to_string(),
                Profile { tools: names, configs: IndexMap::new(), hooks: raw.hooks },
            );
            QwertConfig { tools: raw.tools, profiles, plugins: raw.plugins }
        }
    }
}

impl From<QwertConfig> for QwertConfigRaw {
    fn from(cfg: QwertConfig) -> Self {
        QwertConfigRaw {
            tools: cfg.tools,
            profiles: cfg.profiles,
            hooks: Hooks::default(),
            plugins: cfg.plugins,
        }
    }
}

impl QwertConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_yml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_yml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Names of all profiles.
    pub fn profile_names(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }

    /// Does a profile exist (even with no tools)?
    pub fn has_profile(&self, profile: &str) -> bool {
        self.profiles.contains_key(profile)
    }

    /// Profiles that reference at least one catalog tool — candidates for the
    /// machine profile selection prompt.
    pub fn profiles_with_tools(&self) -> Vec<String> {
        self.profiles
            .iter()
            .filter(|(_, p)| !p.tools.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Get (or create) the profile.
    pub fn ensure_profile(&mut self, profile: &str) -> &mut Profile {
        self.profiles.entry(profile.to_string()).or_default()
    }

    /// Is a tool declared (in the catalog or referenced by any profile)?
    pub fn declared_anywhere(&self, name: &str) -> bool {
        self.tools.contains_key(name)
            || self.profiles.values().any(|p| p.tools.iter().any(|t| t == name))
    }

    /// Is a tool referenced by a specific profile?
    pub fn has_tool_in(&self, profile: &str, name: &str) -> bool {
        self.profiles
            .get(profile)
            .map(|p| p.tools.iter().any(|t| t == name))
            .unwrap_or(false)
    }

    /// Add or update a tool: ensure it exists in the catalog and reference it in
    /// the profile's tool list. `version` defaults to "latest" when adding a new
    /// tool; an existing catalog version is preserved when `version` is None.
    pub fn add_tool(&mut self, profile: &str, name: &str, version: Option<&str>) {
        match version {
            Some(ver) => {
                self.tools
                    .entry(name.to_string())
                    .and_modify(|e| match e {
                        ToolEntry::Simple(v) => *v = ver.to_string(),
                        ToolEntry::Full(c) => c.version = ver.to_string(),
                    })
                    .or_insert_with(|| ToolEntry::Simple(ver.to_string()));
            }
            None => {
                self.tools
                    .entry(name.to_string())
                    .or_insert_with(|| ToolEntry::Simple("latest".to_string()));
            }
        }
        let p = self.ensure_profile(profile);
        if !p.tools.iter().any(|t| t == name) {
            p.tools.push(name.to_string());
        }
    }

    /// Remove a tool from the catalog and every profile's tool list.
    pub fn remove_tool(&mut self, name: &str) {
        self.tools.shift_remove(name);
        for profile in self.profiles.values_mut() {
            profile.tools.retain(|t| t != name);
            profile.configs.shift_remove(name);
        }
        self.profiles.retain(|_, p| {
            !p.tools.is_empty() || !p.configs.is_empty() || !hooks_empty(&p.hooks)
        });
    }

    /// Profile tool names in declaration order. A tool referenced by the profile but
    /// missing from the catalog is still returned (version defaults to "latest").
    pub fn tool_names_for_profile(&self, profile: &str) -> Vec<String> {
        match self.profiles.get(profile) {
            Some(p) => p.tools.clone(),
            None => Vec::new(),
        }
    }

    /// Version for a catalog tool. Defaults to "latest".
    #[allow(dead_code)]
    pub fn version_of(&self, _profile: &str, name: &str) -> &str {
        self.tools.get(name).map(|e| e.version()).unwrap_or("latest")
    }

    /// Config source path declared for a tool within a profile (the `from` for the
    /// recipe's symlink/copy setup). The source of truth for where each dotfile lives.
    pub fn config_source_for(&self, profile: &str, name: &str) -> Option<&str> {
        self.profiles
            .get(profile)
            .and_then(|p| p.configs.get(name))
            .map(|s| s.as_str())
    }

    /// Set (or replace) the config source path for a tool in a profile.
    pub fn set_config_source(&mut self, profile: &str, name: &str, path: &str) {
        self.ensure_profile(profile).configs.insert(name.to_string(), path.to_string());
    }

    /// Inline setup declared in the catalog for a tool (legacy form).
    pub fn inline_setup_of(&self, name: &str) -> Option<&InlineSetup> {
        match self.tools.get(name) {
            Some(ToolEntry::Full(c)) => c.setup.as_ref(),
            _ => None,
        }
    }

    /// List of profiles that reference this tool (for display).
    pub fn profiles_of_tool(&self, name: &str) -> Vec<String> {
        self.profiles
            .iter()
            .filter(|(_, p)| p.tools.iter().any(|t| t == name))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Append a hook path to a profile's prepare/init list (dedup). No-op for unknown hooks.
    pub fn add_hook(&mut self, profile: &str, hook: &str, path: &str) {
        if hook != "prepare" && hook != "init" {
            return;
        }
        let p = self.ensure_profile(profile);
        let scripts = match hook {
            "prepare" => &mut p.hooks.prepare,
            _ => &mut p.hooks.init,
        };
        if !scripts.iter().any(|s| s == path) {
            scripts.push(path.to_string());
        }
    }

    /// Hook paths for a profile's prepare or init phase.
    pub fn hooks_for_profile(&self, profile: &str, phase: &str) -> Vec<String> {
        let Some(p) = self.profiles.get(profile) else {
            return Vec::new();
        };
        match phase {
            "prepare" => p.hooks.prepare.clone(),
            "init" => p.hooks.init.clone(),
            _ => Vec::new(),
        }
    }

    /// Declared plugins in add order.
    pub fn plugins(&self) -> &[PluginSource] {
        &self.plugins
    }

    /// Add (or replace) a plugin by name. Returns true when it was already declared.
    pub fn add_plugin(&mut self, name: &str, url: &str) {
        if let Some(existing) = self.plugins.iter_mut().find(|p| p.name == name) {
            existing.url = url.to_string();
        } else {
            self.plugins.push(PluginSource { name: name.to_string(), url: url.to_string() });
        }
    }

    /// Remove a plugin by name. Returns true when it was declared and removed.
    pub fn remove_plugin(&mut self, name: &str) -> bool {
        let before = self.plugins.len();
        self.plugins.retain(|p| p.name != name);
        self.plugins.len() != before
    }
}

/// User directory: ~/.qwert/ (dotfiles + manifest)
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".qwert")
}

/// Path to the manifest: ~/.qwert/config.yml
pub fn manifest_path() -> PathBuf {
    config_dir().join("config.yml")
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), &path[2..]);
        }
    }
    path.to_string()
}

#[cfg(test)]
#[path = "tests/qwert_yml.rs"]
mod tests;