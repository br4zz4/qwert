use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recipe {
    pub meta: RecipeMeta,
    pub check: Option<RecipeCheck>,
    pub install: Option<RecipeInstall>,
    pub upgrade: Option<RecipeUpgrade>,
    pub uninstall: Option<RecipeUninstall>,
    pub setup: Option<RecipeSetup>,
    /// Where the recipe was loaded from — local (~/.qwert/recipes) or remote cache.
    #[serde(default)]
    pub local: bool,

    /// Recipe declares setup but no install (setup-only: e.g. a local agents/skills recipe).
    #[serde(default)]
    pub setup_only: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeMeta {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(rename = "type")]
    pub kind: RecipeKind,
    #[serde(default)]
    pub depends: Vec<String>,
    /// Package name per package manager (brew/apt/pacman), for agnostic recipes.
    /// When missing, the platform PM installs `name` (legacy `pkg` only applies to brew).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packages: Option<std::collections::BTreeMap<String, String>>,
    pub pkg: Option<String>,
    /// Platforms this recipe supports. When absent, works on every platform.
    /// Values: "macos", "debian", "arch", or "linux" (alias for debian+arch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<String>>,
}

impl RecipeMeta {
    /// Does this recipe support the given platform?
    ///
    /// `platforms` absent → supported everywhere. "linux" matches debian and
    /// arch. Unknown values never match (they're treated as "not this platform").
    pub fn supports(&self, platform: &crate::platform::Platform) -> bool {
        let Some(platforms) = &self.platforms else { return true };
        let current = platform.as_str();
        platforms.iter().any(|p| match p.as_str() {
            "linux" => matches!(current, "debian" | "arch"),
            other => other == current,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RecipeKind {
    /// Package-manager agnostic — the platform's PM installs the package (recipes don't
    /// ship with a fixed `type = "brew"|"apt"|"pacman"` anymore; use `packages` for names).
    Package,
    /// Custom install/upgrade/uninstall commands in the recipe.
    Custom,
    /// Legacy brew recipe — treated as a package on the current platform.
    Brew,
    /// Legacy apt recipe — treated as a package on the current platform.
    Apt,
    /// Legacy pacman recipe — treated as a package on the current platform.
    Pacman,
    /// Legacy qwert recipe — custom commands.
    Qwert,
}

impl RecipeKind {
    /// Does this recipe resolve through the platform's package manager?
    pub fn is_package(&self) -> bool {
        matches!(self, RecipeKind::Package | RecipeKind::Brew | RecipeKind::Apt | RecipeKind::Pacman)
    }
}

impl std::fmt::Display for RecipeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecipeKind::Package => write!(f, "package"),
            RecipeKind::Custom => write!(f, "custom"),
            RecipeKind::Brew => write!(f, "brew"),
            RecipeKind::Apt => write!(f, "apt"),
            RecipeKind::Pacman => write!(f, "pacman"),
            RecipeKind::Qwert => write!(f, "qwert"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeCheck {
    pub command: Option<String>,
    pub version_flag: Option<String>,
    pub cmd: Option<String>,
}

/// A single command string or an ordered list of commands.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Commands {
    One(String),
    Many(Vec<String>),
}

impl Commands {
    pub fn as_steps(&self) -> Vec<&str> {
        match self {
            Commands::One(s) => vec![s.as_str()],
            Commands::Many(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeInstall {
    pub macos: Option<Commands>,
    pub debian: Option<Commands>,
    pub arch: Option<Commands>,
    pub linux: Option<Commands>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeUpgrade {
    pub macos: Option<Commands>,
    pub debian: Option<Commands>,
    pub arch: Option<Commands>,
    pub linux: Option<Commands>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeUninstall {
    pub macos: Option<Commands>,
    pub debian: Option<Commands>,
    pub arch: Option<Commands>,
    pub linux: Option<Commands>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecipeSetup {
    pub from: Option<String>,
    pub to: String,
    #[serde(default)]
    pub symlink: bool,
    /// What kind of thing `to` expects at the destination (file or directory).
    /// Lets qwert validate the config.yml source and guide the user.
    #[serde(default)]
    pub dest: Option<DestKind>,
    pub macos: Option<Commands>,
    pub debian: Option<Commands>,
    pub arch: Option<Commands>,
    pub linux: Option<Commands>,
    pub undo: Option<SetupUndo>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DestKind {
    File,
    Dir,
}

impl std::fmt::Display for DestKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DestKind::File => write!(f, "file"),
            DestKind::Dir => write!(f, "directory"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetupUndo {
    pub macos: Option<Commands>,
    pub debian: Option<Commands>,
    pub arch: Option<Commands>,
    pub linux: Option<Commands>,
}

/// Parsed from install.toml
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstallFile {
    pub meta: RecipeMeta,
    pub check: Option<RecipeCheck>,
    pub install: Option<RecipeInstall>,
    pub upgrade: Option<RecipeUpgrade>,
    pub uninstall: Option<RecipeUninstall>,
}

/// Parsed from setup.toml (flat, no sections except [undo])
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetupFile {
    pub from: Option<String>,
    pub to: String,
    #[serde(default)]
    pub symlink: bool,
    #[serde(default)]
    pub dest: Option<DestKind>,
    pub macos: Option<Commands>,
    pub debian: Option<Commands>,
    pub arch: Option<Commands>,
    pub linux: Option<Commands>,
    pub undo: Option<SetupUndo>,
}

fn platform_cmds<'a>(
    platform: &crate::platform::Platform,
    macos: Option<&'a Commands>,
    debian: Option<&'a Commands>,
    arch: Option<&'a Commands>,
    linux: Option<&'a Commands>,
) -> Vec<&'a str> {
    let cmds = match platform {
        crate::platform::Platform::MacOS => macos,
        // Arch uses its own section, falling back to debian, then linux
        crate::platform::Platform::Arch => arch.or(debian).or(linux),
        crate::platform::Platform::Debian => debian.or(linux),
        crate::platform::Platform::Unknown => debian.or(linux),
    };
    cmds.map(|c| c.as_steps()).unwrap_or_default()
}

impl Recipe {
    pub fn install_steps_for(&self, platform: &crate::platform::Platform) -> Vec<&str> {
        let Some(s) = &self.install else { return vec![] };
        platform_cmds(platform, s.macos.as_ref(), s.debian.as_ref(), s.arch.as_ref(), s.linux.as_ref())
    }

    pub fn uninstall_steps_for(&self, platform: &crate::platform::Platform) -> Vec<&str> {
        let Some(s) = &self.uninstall else { return vec![] };
        platform_cmds(platform, s.macos.as_ref(), s.debian.as_ref(), s.arch.as_ref(), s.linux.as_ref())
    }

    pub fn upgrade_steps_for(&self, platform: &crate::platform::Platform) -> Vec<&str> {
        let Some(s) = &self.upgrade else { return vec![] };
        platform_cmds(platform, s.macos.as_ref(), s.debian.as_ref(), s.arch.as_ref(), s.linux.as_ref())
    }
}

impl RecipeSetup {
    pub fn setup_cmds_for(&self, platform: &crate::platform::Platform) -> Vec<&str> {
        platform_cmds(platform, self.macos.as_ref(), self.debian.as_ref(), self.arch.as_ref(), self.linux.as_ref())
    }

    pub fn undo_cmds_for(&self, platform: &crate::platform::Platform) -> Vec<&str> {
        let Some(undo) = &self.undo else { return vec![] };
        platform_cmds(platform, undo.macos.as_ref(), undo.debian.as_ref(), undo.arch.as_ref(), undo.linux.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;

    fn make_recipe(install_macos: Option<Commands>, install_debian: Option<Commands>) -> Recipe {
        Recipe {
            meta: RecipeMeta {
                name: "test".into(),
                version: "1.0.0".into(),
                description: "test recipe".into(),
                kind: RecipeKind::Brew,
                depends: vec![],
                packages: None,
                pkg: None,
                platforms: None,
            },
            check: None,
            install: Some(RecipeInstall { macos: install_macos, debian: install_debian, arch: None, linux: None }),
            upgrade: None,
            uninstall: None,
            setup: None,
            local: false,
            setup_only: false,
        }
    }

    #[test]
    fn commands_one_returns_single_step() {
        // arrange
        let cmd = Commands::One("brew install tmux".into());
        // act
        let steps = cmd.as_steps();
        // assert
        assert_eq!(steps, vec!["brew install tmux"]);
    }

    #[test]
    fn commands_many_returns_all_steps_in_order() {
        // arrange
        let cmd = Commands::Many(vec!["step one".into(), "step two".into(), "step three".into()]);
        // act
        let steps = cmd.as_steps();
        // assert
        assert_eq!(steps, vec!["step one", "step two", "step three"]);
    }

    #[test]
    fn install_steps_for_macos_returns_macos_commands() {
        // arrange
        let recipe = make_recipe(
            Some(Commands::One("brew install nvim".into())),
            Some(Commands::One("apt install nvim".into())),
        );
        // act
        let steps = recipe.install_steps_for(&Platform::MacOS);
        // assert
        assert_eq!(steps, vec!["brew install nvim"]);
    }

    #[test]
    fn install_steps_for_debian_returns_debian_commands() {
        // arrange
        let recipe = make_recipe(
            Some(Commands::One("brew install nvim".into())),
            Some(Commands::One("apt install nvim".into())),
        );
        // act
        let steps = recipe.install_steps_for(&Platform::Debian);
        // assert
        assert_eq!(steps, vec!["apt install nvim"]);
    }

    #[test]
    fn install_steps_empty_when_platform_not_supported() {
        // arrange
        let recipe = make_recipe(Some(Commands::One("brew install nvim".into())), None);
        // act
        let steps = recipe.install_steps_for(&Platform::Debian);
        // assert
        assert!(steps.is_empty());
    }

    #[test]
    fn install_steps_empty_when_no_install_section() {
        // arrange
        let mut recipe = make_recipe(None, None);
        recipe.install = None;
        // act
        let steps = recipe.install_steps_for(&Platform::MacOS);
        // assert
        assert!(steps.is_empty());
    }

    #[test]
    fn recipe_kind_display_brew() {
        // arrange
        let kind = RecipeKind::Brew;
        // act
        let result = kind.to_string();
        // assert
        assert_eq!(result, "brew");
    }

    #[test]
    fn recipe_kind_display_qwert() {
        // arrange
        let kind = RecipeKind::Qwert;
        // act
        let result = kind.to_string();
        // assert
        assert_eq!(result, "qwert");
    }

    #[test]
    fn uninstall_steps_empty_when_no_section_and_brew_kind() {
        // arrange
        let recipe = make_recipe(None, None);
        // act
        let steps = recipe.uninstall_steps_for(&Platform::MacOS);
        // assert
        assert!(steps.is_empty());
    }

    #[test]
    fn uninstall_steps_uses_explicit_macos_section() {
        // arrange
        let mut recipe = make_recipe(None, None);
        recipe.uninstall = Some(RecipeUninstall {
            macos: Some(Commands::One("brew uninstall tmux".into())),
            debian: None,
            arch: None,
            linux: None,
        });
        // act
        let steps = recipe.uninstall_steps_for(&Platform::MacOS);
        // assert
        assert_eq!(steps, vec!["brew uninstall tmux"]);
    }

    #[test]
    fn depends_field_parsed_from_toml() {
        // arrange
        let toml = r#"
[meta]
name = "lvim"
version = "1.0.0"
description = "LunarVim"
type = "qwert"
depends = ["neovim"]

[check]
command = "lvim"
"#;
        // act
        let recipe: InstallFile = toml::from_str(toml).unwrap();
        // assert
        assert_eq!(recipe.meta.depends, vec!["neovim"]);
    }

    #[test]
    fn install_file_parses_arch_commands() {
        // arrange
        let toml = r#"
[meta]
name = "tmux"
version = "1.0.0"
description = "Terminal multiplexer"
type = "qwert"

[install]
arch = "pacman -S --noconfirm tmux"
debian = "apt-get install -y tmux"
"#;
        // act
        let recipe: InstallFile = toml::from_str(toml).unwrap();
        // assert
        let steps = recipe.install.as_ref().unwrap().arch.as_ref().unwrap().as_steps();
        assert_eq!(steps, vec!["pacman -S --noconfirm tmux"]);
    }

    #[test]
    fn setup_file_parses_arch_commands() {
        // arrange
        let toml = r#"
to = "~/.config/nvim"
symlink = true
arch = "echo arch-setup"
"#;
        // act
        let setup: SetupFile = toml::from_str(toml).unwrap();
        // assert
        assert!(setup.arch.is_some());
    }

    #[test]
    fn setup_cmds_for_macos_returns_macos_commands() {
        // arrange
        let setup = RecipeSetup {
            from: None,
            to: "~/.config/iterm2".into(),
            symlink: false,
            dest: None,
            macos: Some(Commands::One("defaults write com.foo bar".into())),
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        // act
        let cmds = setup.setup_cmds_for(&Platform::MacOS);
        // assert
        assert_eq!(cmds, vec!["defaults write com.foo bar"]);
    }

    #[test]
    fn setup_cmds_for_debian_returns_debian_commands() {
        // arrange
        let setup = RecipeSetup {
            from: None,
            to: "/etc/foo".into(),
            symlink: false,
            dest: None,
            macos: None,
            debian: Some(Commands::One("ln -s foo bar".into())),
            arch: None,
            linux: None,
undo: None,
        };
        // act
        let cmds = setup.setup_cmds_for(&Platform::Debian);
        // assert
        assert_eq!(cmds, vec!["ln -s foo bar"]);
    }

    #[test]
    fn setup_cmds_for_returns_empty_when_no_commands() {
        // arrange
        let setup = RecipeSetup {
            from: None,
            to: "~/.tmux.conf".into(),
            symlink: true,
            dest: None,
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        // act
        let cmds = setup.setup_cmds_for(&Platform::MacOS);
        // assert
        assert!(cmds.is_empty());
    }

    #[test]
    fn undo_cmds_for_returns_undo_commands() {
        // arrange
        let setup = RecipeSetup {
            from: None,
            to: "~/.config/iterm2".into(),
            symlink: false,
            dest: None,
            macos: None,
            debian: None,
            arch: None,
            linux: None,
            undo: Some(SetupUndo {
                macos: Some(Commands::One("defaults delete com.foo bar".into())),
                debian: None,
                arch: None,
                linux: None,
            }),
        };
        // act
        let cmds = setup.undo_cmds_for(&Platform::MacOS);
        // assert
        assert_eq!(cmds, vec!["defaults delete com.foo bar"]);
    }

    #[test]
    fn undo_cmds_for_returns_empty_when_no_undo_section() {
        // arrange
        let setup = RecipeSetup {
            from: None,
            to: "~/.tmux.conf".into(),
            symlink: true,
            dest: None,
            macos: None,
            debian: None,
            arch: None,
            linux: None,
            undo: None,
        };
        // act
        let cmds = setup.undo_cmds_for(&Platform::MacOS);
        // assert
        assert!(cmds.is_empty());
    }

    // --- platforms field ---

    fn recipe_with_platforms(platforms: Option<Vec<String>>) -> Recipe {
        let mut r = make_recipe(None, None);
        r.meta.platforms = platforms;
        r
    }

    #[test]
    fn supports_returns_true_when_platforms_absent() {
        // arrange
        let recipe = recipe_with_platforms(None);
        // act + assert
        assert!(recipe.meta.supports(&Platform::MacOS));
        assert!(recipe.meta.supports(&Platform::Debian));
        assert!(recipe.meta.supports(&Platform::Arch));
    }

    #[test]
    fn supports_returns_true_when_platform_matches() {
        // arrange
        let recipe = recipe_with_platforms(Some(vec!["macos".into()]));
        // act + assert
        assert!(recipe.meta.supports(&Platform::MacOS));
        assert!(!recipe.meta.supports(&Platform::Debian));
        assert!(!recipe.meta.supports(&Platform::Arch));
    }

    #[test]
    fn supports_linux_alias_matches_debian_and_arch() {
        // arrange
        let recipe = recipe_with_platforms(Some(vec!["linux".into()]));
        // act + assert
        assert!(recipe.meta.supports(&Platform::Debian));
        assert!(recipe.meta.supports(&Platform::Arch));
        assert!(!recipe.meta.supports(&Platform::MacOS));
    }

    #[test]
    fn supports_unknown_value_never_matches() {
        // arrange
        let recipe = recipe_with_platforms(Some(vec!["windows".into()]));
        // act + assert
        assert!(!recipe.meta.supports(&Platform::MacOS));
        assert!(!recipe.meta.supports(&Platform::Debian));
        assert!(!recipe.meta.supports(&Platform::Arch));
    }

    #[test]
    fn platforms_parsed_from_install_toml() {
        // arrange
        let toml = r#"
[meta]
name = "iterm2"
version = "1.0.0"
description = "macOS terminal emulator"
type = "qwert"
platforms = ["macos"]

[install]
macos = "brew install --cask iterm2"
"#;
        // act
        let recipe: InstallFile = toml::from_str(toml).unwrap();
        // assert
        assert_eq!(recipe.meta.platforms.as_deref(), Some(&["macos".to_string()][..]));
    }

    // --- linux alias in command sections ---

    #[test]
    fn install_steps_for_arch_falls_back_to_linux() {
        // arrange
        let mut recipe = make_recipe(None, None);
        recipe.install = Some(RecipeInstall {
            macos: Some(Commands::One("brew install nvim".into())),
            debian: None,
            arch: None,
            linux: Some(Commands::One("npm install -g nvim".into())),
        });
        // act
        let steps = recipe.install_steps_for(&Platform::Arch);
        // assert — arch section absent → linux alias applies
        assert_eq!(steps, vec!["npm install -g nvim"]);
    }

    #[test]
    fn install_steps_for_debian_falls_back_to_linux() {
        // arrange
        let mut recipe = make_recipe(None, None);
        recipe.install = Some(RecipeInstall {
            macos: Some(Commands::One("brew install nvim".into())),
            debian: None,
            arch: None,
            linux: Some(Commands::One("npm install -g nvim".into())),
        });
        // act
        let steps = recipe.install_steps_for(&Platform::Debian);
        // assert
        assert_eq!(steps, vec!["npm install -g nvim"]);
    }

    #[test]
    fn install_steps_specific_arch_wins_over_linux() {
        // arrange
        let mut recipe = make_recipe(None, None);
        recipe.install = Some(RecipeInstall {
            macos: None,
            debian: None,
            arch: Some(Commands::One("pacman -S nvim".into())),
            linux: Some(Commands::One("npm install -g nvim".into())),
        });
        // act — specific section takes precedence over linux alias
        let steps = recipe.install_steps_for(&Platform::Arch);
        // assert
        assert_eq!(steps, vec!["pacman -S nvim"]);
    }

    #[test]
    fn setup_cmds_for_arch_falls_back_to_linux() {
        // arrange
        let setup = RecipeSetup {
            from: None,
            to: "~/.local/share/qwert/hooks/init/p10k.sh".into(),
            symlink: false,
            dest: None,
            macos: None,
            debian: None,
            arch: None,
            linux: Some(Commands::One("echo linux-setup".into())),
            undo: None,
        };
        // act
        let cmds = setup.setup_cmds_for(&Platform::Arch);
        // assert
        assert_eq!(cmds, vec!["echo linux-setup"]);
    }

    #[test]
    fn install_file_parses_linux_commands() {
        // arrange
        let toml = r#"
[meta]
name = "claude"
version = "1.0.0"
description = "Claude Code"
type = "qwert"

[install]
linux = "npm install -g @anthropic-ai/claude-code"
macos = "npm install -g @anthropic-ai/claude-code"
"#;
        // act
        let recipe: InstallFile = toml::from_str(toml).unwrap();
        // assert
        let steps = recipe.install.as_ref().unwrap().linux.as_ref().unwrap().as_steps();
        assert_eq!(steps, vec!["npm install -g @anthropic-ai/claude-code"]);
    }
}
