use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

use super::schema::{InstallFile, Recipe, RecipeCheck, RecipeMeta, RecipeKind, RecipeSetup, SetupFile};

fn load_toml_opt<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn default_kind() -> RecipeKind {
    // Recipes are package-manager agnostic: the platform's PM installs them.
    RecipeKind::Package
}

fn assemble_recipe(name: &str, install: Option<InstallFile>, setup: Option<SetupFile>, local: bool) -> Recipe {
    // A local recipe with only a setup section is setup-only (e.g. a skills/agents
    // linker) — it never installs a package. Remote recipes with only setup fall
    // back to the platform's default package manager (like the claude recipe).
    let setup_only = local && install.is_none() && setup.is_some();
    let meta = install.as_ref().map(|i| i.meta.clone()).unwrap_or_else(|| RecipeMeta {
        name: name.to_string(),
        version: String::new(),
        description: String::new(),
        kind: default_kind(),
        depends: vec![],
        packages: None,
        pkg: None,
        platforms: None,
    });

    let check = install.as_ref().and_then(|i| i.check.clone()).or_else(|| {
        Some(RecipeCheck { command: Some(name.to_string()), version_flag: None, cmd: None })
    });

    let recipe_setup = setup.map(|s| RecipeSetup {
        from: s.from,
        to: s.to,
        symlink: s.symlink,
        dest: s.dest,
        macos: s.macos,
        debian: s.debian,
        arch: s.arch,
        linux: s.linux,
        all: s.all,
        undo: s.undo,
    });

    Recipe {
        meta,
        check,
        install: install.as_ref().and_then(|i| i.install.clone()),
        upgrade: install.as_ref().and_then(|i| i.upgrade.clone()),
        uninstall: install.as_ref().and_then(|i| i.uninstall.clone()),
        setup: recipe_setup,
        local,
        setup_only,
    }
}

/// Find a recipe in a single directory tree (no precedence).
fn find_in(dir: &Path, name: &str, local: bool) -> Option<Recipe> {
    let install: Option<InstallFile> = load_toml_opt(&dir.join(name).join("install.toml"));
    let setup: Option<SetupFile> = load_toml_opt(&dir.join(name).join("setup.toml"));
    if install.is_none() && setup.is_none() {
        return None;
    }
    Some(assemble_recipe(name, install, setup, local))
}

/// Find a recipe by name — checks local user recipes (~/.qwert/recipes) first,
/// then plugin recipes (in declaration order), then the default cache. Local
/// recipes take precedence on name collision.
pub fn find(name: &str, recipes_dir: &Path) -> Option<Recipe> {
    let local_dir = crate::config::qwert_yml::config_dir().join("recipes");
    if local_dir.is_dir() {
        if let Some(recipe) = find_in(&local_dir, name, true) {
            return Some(recipe);
        }
    }
    for plugin_dir in crate::plugins::recipe_dirs() {
        if let Some(recipe) = find_in(&plugin_dir, name, false) {
            return Some(recipe);
        }
    }
    find_in(recipes_dir, name, false)
}

/// Load all recipes from plugins (declaration order) plus the default cache.
/// On name collision the higher-precedence recipe wins (plugins beat default,
/// earlier plugins beat later ones). Used by search.
pub fn load_all(recipes_dir: &Path) -> Vec<Recipe> {
    let mut seen = std::collections::HashSet::new();
    let mut recipes = Vec::new();

    for dir in crate::plugins::recipe_dirs() {
        collect_from(&dir, &mut seen, &mut recipes);
    }
    collect_from(recipes_dir, &mut seen, &mut recipes);

    recipes.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));
    recipes
}

/// Add recipes from `dir` unless a higher-precedence recipe already claimed the name.
fn collect_from(dir: &Path, seen: &mut std::collections::HashSet<String>, recipes: &mut Vec<Recipe>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if seen.contains(name) {
            continue;
        }
        if let Some(recipe) = find_in(dir, name, false) {
            seen.insert(name.to_string());
            recipes.push(recipe);
        }
    }
}

/// Path to the recipes cache directory (~/.local/share/qwert/recipes/)
pub fn cache_dir() -> Option<PathBuf> {
    Some(crate::platform::data_dir().join("recipes"))
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const INSTALL_TOML: &str = r#"
[meta]
name = "tmux"
version = "1.0.0"
description = "Terminal multiplexer"
type = "brew"

[check]
command = "tmux"
version_flag = "-V"
"#;

    const SETUP_TOML: &str = r#"
to = "~/.tmux.conf"
symlink = true
"#;

    fn make_recipe_dir(dir: &Path, name: &str, install: Option<&str>, setup: Option<&str>) {
        let recipe_dir = dir.join(name);
        fs::create_dir_all(&recipe_dir).unwrap();
        if let Some(content) = install {
            fs::write(recipe_dir.join("install.toml"), content).unwrap();
        }
        if let Some(content) = setup {
            fs::write(recipe_dir.join("setup.toml"), content).unwrap();
        }
    }

    #[test]
    fn find_returns_recipe_from_directory() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_find_dir");
        make_recipe_dir(&dir, "tmux", Some(INSTALL_TOML), None);
        // act
        let result = find("tmux", &dir);
        // assert
        assert!(result.is_some());
        assert_eq!(result.unwrap().meta.name, "tmux");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_returns_none_when_no_directory() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_find_none");
        fs::create_dir_all(&dir).unwrap();
        // act
        let result = find("neovim", &dir);
        // assert
        assert!(result.is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_assembles_install_and_setup_when_both_present() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_find_both");
        make_recipe_dir(&dir, "tmux", Some(INSTALL_TOML), Some(SETUP_TOML));
        // act
        let result = find("tmux", &dir).unwrap();
        // assert
        assert!(result.setup.is_some());
        assert_eq!(result.setup.unwrap().to, "~/.tmux.conf");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_returns_setup_none_when_only_install_toml() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_find_install_only");
        make_recipe_dir(&dir, "tmux", Some(INSTALL_TOML), None);
        // act
        let result = find("tmux", &dir).unwrap();
        // assert
        assert!(result.setup.is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_synthesizes_meta_when_only_setup_toml() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_find_setup_only");
        make_recipe_dir(&dir, "tmux", None, Some(SETUP_TOML));
        // act
        let result = find("tmux", &dir).unwrap();
        // assert
        assert_eq!(result.meta.name, "tmux");
        assert!(result.setup.is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_synthesizes_check_when_no_install_toml() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_find_check_synth");
        make_recipe_dir(&dir, "git", None, Some(SETUP_TOML));
        // act
        let result = find("git", &dir).unwrap();
        // assert
        let check = result.check.unwrap();
        assert_eq!(check.command, Some("git".to_string()));
        assert!(check.version_flag.is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_all_returns_recipes_sorted_by_name_from_subdirs() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_load_all_dirs");
        let neovim_install = INSTALL_TOML.replace("name = \"tmux\"", "name = \"neovim\"")
            .replace("command = \"tmux\"", "command = \"nvim\"");
        make_recipe_dir(&dir, "tmux", Some(INSTALL_TOML), None);
        make_recipe_dir(&dir, "neovim", Some(&neovim_install), None);
        // act
        let recipes = load_all(&dir);
        // assert
        assert_eq!(recipes.len(), 2);
        assert_eq!(recipes[0].meta.name, "neovim");
        assert_eq!(recipes[1].meta.name, "tmux");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_all_returns_empty_for_missing_dir() {
        // arrange
        let dir = std::path::Path::new("/tmp/qwert_definitely_missing_dir_xyz");
        // act
        let recipes = load_all(dir);
        // assert
        assert!(recipes.is_empty());
    }

    #[test]
    fn find_in_marks_recipe_remote() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_find_in_remote");
        make_recipe_dir(&dir, "tmux", Some(INSTALL_TOML), None);
        // act
        let recipe = find_in(&dir, "tmux", false).unwrap();
        // assert
        assert!(!recipe.local);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_all_recipes_are_not_local() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_test_load_all_local_flag");
        make_recipe_dir(&dir, "tmux", Some(INSTALL_TOML), None);
        // act
        let recipes = load_all(&dir);
        // assert
        assert!(!recipes[0].local);
        fs::remove_dir_all(&dir).ok();
    }
}
