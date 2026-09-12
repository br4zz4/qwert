use std::path::{Path, PathBuf};

use super::schema::{DestKind, Recipe, RecipeSetup};
use crate::config::qwert_yml;
use crate::platform;
use crate::platform::fs as pfs;
use crate::ui::printer;

pub enum RunResult {
    AlreadyInstalled { version: Option<String> },
    Installed,
    Failed(String),
    NotSupported,
}

/// Check if a recipe is already installed
pub fn is_installed(recipe: &Recipe) -> bool {
    // Package recipes: ask yuiop (the platform's package manager) — more accurate
    // than `which`, and it knows the per-manager package name.
    if crate::adapters::yuiop::is_package_kind(&recipe.meta.kind) {
        return match crate::adapters::yuiop::registered(&recipe.meta) {
            Some(installed) => installed,
            None => platform::which(&recipe.meta.name),
        };
    }

    let Some(check) = &recipe.check else { return false };
    if let Some(cmd) = &check.cmd {
        return platform::run_cmd_capture(cmd).is_ok();
    }
    if let Some(command) = &check.command {
        return platform::which(command);
    }
    false
}

/// Get the installed version of a recipe
pub fn installed_version(recipe: &Recipe) -> Option<String> {
    let check = recipe.check.as_ref()?;
    let command = check.command.as_deref()?;
    let flag = check.version_flag.as_deref()?;
    platform::version_of(command, flag)
}

fn run_install(cmd: &str) -> Result<(), String> {
    platform::run_cmd(cmd).map_err(|e| e.to_string())
}

fn run_upgrade(cmd: &str) -> Result<(), String> {
    platform::run_cmd(cmd).map_err(|e| e.to_string())
}

pub fn version_msg(prefix: &str, version: Option<String>) -> String {
    version.map(|v| format!("{} ({})", prefix, v)).unwrap_or_else(|| prefix.to_string())
}

/// Install a recipe on the current platform, resolving dependencies first.
pub fn install(recipe: &Recipe, recipes_dir: &Path) -> RunResult {
    let platform = platform::detect();

    if is_installed(recipe) {
        let version = installed_version(recipe);
        return RunResult::AlreadyInstalled { version };
    }

    // Setup-only recipes (e.g. a local agents/skills recipe) install nothing.
    if recipe.setup_only {
        return RunResult::Installed;
    }

    for dep_name in &recipe.meta.depends {
        match super::index::find(dep_name, recipes_dir) {
            Some(dep) => {
                if !is_installed(&dep) {
                    printer::installing(dep_name, &format!("dependency of {}...", recipe.meta.name));
                    let result = install(&dep, recipes_dir);
                    if matches!(result, RunResult::Failed(_) | RunResult::NotSupported) {
                        return RunResult::Failed(format!("dependency '{}' failed to install", dep_name));
                    }
                }
            }
            None => return RunResult::Failed(format!("dependency '{}' not found", dep_name)),
        }
    }

    // Package recipes go through yuiop — the recipe never forces a specific PM;
    // on macOS it installs via brew, on Debian via apt, on Arch via pacman.
    if crate::adapters::yuiop::is_package_kind(&recipe.meta.kind) {
        return match crate::adapters::yuiop::install(&recipe.meta.name) {
            Ok(_) => RunResult::Installed,
            Err(e) => RunResult::Failed(e),
        };
    }

    // Custom recipes run explicit commands — no silent fallback to the platform PM.
    let steps = recipe.install_steps_for(&platform);
    if steps.is_empty() {
        return RunResult::Failed(format!(
            "recipe '{}' has no install steps for {} — add a 'macos'/'debian'/'arch' section",
            recipe.meta.name, platform
        ));
    }

    for step in steps {
        if let Err(e) = run_install(step) {
            return RunResult::Failed(e);
        }
    }
    RunResult::Installed
}

/// Uninstall a recipe on the current platform
pub fn uninstall(recipe: &Recipe) -> RunResult {
    let platform = platform::detect();

    // Package recipes uninstall through yuiop.
    if crate::adapters::yuiop::is_package_kind(&recipe.meta.kind) {
        return match crate::adapters::yuiop::uninstall(&recipe.meta.name) {
            Ok(_) => RunResult::Installed,
            Err(e) => RunResult::Failed(e),
        };
    }

    // Custom recipes run explicit commands
    let steps = recipe.uninstall_steps_for(&platform);
    if steps.is_empty() {
        return RunResult::NotSupported;
    }

    for step in steps {
        if let Err(e) = run_install(step) {
            return RunResult::Failed(e);
        }
    }
    RunResult::Installed
}

/// Uninstall a recipe and print status to terminal
pub fn uninstall_with_output(recipe: &Recipe) -> bool {
    let name = &recipe.meta.name;
    printer::installing(name, "uninstalling...");
    match uninstall(recipe) {
        RunResult::Installed => { printer::ok(name, "uninstalled"); true }
        RunResult::NotSupported => { printer::failed(name, "no uninstall command defined"); false }
        RunResult::Failed(err) => {
            if err.contains("required by") || err.contains("is a dependency of") {
                printer::warning(&format!(
                    "{} not uninstalled — required by another package. Remove it manually.",
                    name
                ));
                true
            } else {
                printer::failed(name, &err);
                false
            }
        }
        RunResult::AlreadyInstalled { .. } => unreachable!(),
    }
}

/// Upgrade a recipe
pub fn upgrade(recipe: &Recipe) -> RunResult {
    let platform = platform::detect();

    // Package recipes upgrade through yuiop.
    if crate::adapters::yuiop::is_package_kind(&recipe.meta.kind) {
        return match crate::adapters::yuiop::upgrade(&recipe.meta.name) {
            Ok(_) => RunResult::Installed,
            Err(e) => RunResult::Failed(e),
        };
    }

    // Custom recipes run explicit commands
    let steps = recipe.upgrade_steps_for(&platform);
    if steps.is_empty() {
        return RunResult::NotSupported;
    }

    for step in steps {
        if let Err(e) = run_upgrade(step) {
            return RunResult::Failed(e);
        }
    }
    RunResult::Installed
}

/// Install a recipe and print status to terminal
pub fn install_with_output(recipe: &Recipe, recipes_dir: &Path) -> bool {
    let name = &recipe.meta.name;

    if recipe.setup_only {
        return true;
    }

    if recipe.local {
        printer::info(&format!("installing local recipe '{}'", name));
    }

    match install(recipe, recipes_dir) {
        RunResult::AlreadyInstalled { version } => {
            printer::ok(name, &version_msg("already installed", version));
            true
        }
        RunResult::Installed => {
            let tag = printer::kind_tag(&crate::adapters::yuiop::resolve_label(&recipe.meta));
            let msg = format!("{}  {}", version_msg("installed", installed_version(recipe)), tag);
            printer::ok(name, &msg);
            true
        }
        RunResult::Failed(err) => {
            printer::failed(name, &err);
            false
        }
        RunResult::NotSupported => {
            printer::failed(name, "not supported on this platform");
            false
        }
    }
}

/// Check and print status of a recipe
pub fn status_with_output(recipe: &Recipe) {
    let name = &recipe.meta.name;
    let tag = printer::kind_tag(&crate::adapters::yuiop::resolve_label(&recipe.meta));
    let origin = printer::origin_tag(recipe.local);

    if is_installed(recipe) {
        let msg = format!("{}  {}  {}", version_msg("installed", installed_version(recipe)), tag, origin);
        printer::ok(name, &msg);
    } else {
        printer::failed(name, &format!("not installed  {}  {}", tag, origin));
    }
}

// --- Setup phase ---

/// Core setup logic for a RecipeSetup section (shared by recipe and inline setup).
/// `from` is the resolved source for symlink/copy setups (from config.yml configs,
/// or the inline setup's own `from`). Recipes themselves are source-less.
fn run_setup_section(name: &str, s: &RecipeSetup, from: Option<PathBuf>) -> RunResult {
    let platform = platform::detect();
    let dest = PathBuf::from(qwert_yml::expand_tilde(&s.to));

    // Commands-based setup (iterm2, delta, etc.)
    let cmds = s.setup_cmds_for(&platform);
    if !cmds.is_empty() {
        for cmd in cmds {
            if let Err(e) = platform::run_cmd(cmd) {
                return RunResult::Failed(e.to_string());
            }
        }
        return RunResult::Installed;
    }

    // Copy: an existing dest already counts as set up — no source needed.
    if !s.symlink && dest.exists() {
        return RunResult::AlreadyInstalled { version: None };
    }

    // Symlink / copy setups need a source declared in config.yml under
    // `profiles.<profile>.configs.<tool>`. Recipes no longer carry a `from`.
    let from = match from {
        Some(from) => from,
        None => {
            let kind_hint = s.dest.as_ref().map(|k| format!(" — expected a {}", k)).unwrap_or_default();
            return RunResult::Failed(format!(
                "{} setup needs a source{} — declare it in config.yml under 'profiles.<profile>.configs.{}'",
                name, kind_hint, name
            ));
        }
    };

    // Validate the source's file/dir kind matches what the recipe expects, so the
    // symlink can't silently link a file where a directory is needed (or vice versa).
    if let Some(kind) = &s.dest {
        let expect_dir = *kind == DestKind::Dir;
        let actual = if from.is_dir() { "directory" } else { "file" };
        if from.exists() && from.is_dir() != expect_dir {
            return RunResult::Failed(format!(
                "{}: '{}' is a {}, but '{}' needs a {} — point configs.{} at a {}",
                name, from.display(), actual, dest.display(), kind, name, kind
            ));
        }
    }

    // Symlink
    if s.symlink {
        if dest.is_symlink() && std::fs::read_link(&dest).ok().as_deref() == Some(from.as_path()) {
            return RunResult::AlreadyInstalled { version: None };
        }
        return match pfs::create_symlink(&from, &dest) {
            Ok(_) => RunResult::Installed,
            Err(e) => RunResult::Failed(e.to_string()),
        };
    }

    // Copy (dest does not exist yet)
    if !from.exists() {
        return RunResult::Failed(format!("from not found: {}", from.display()));
    }
    match pfs::copy_file(&from, &dest) {
        Ok(_) => RunResult::Installed,
        Err(e) => RunResult::Failed(e.to_string()),
    }
}

/// Run the setup phase for a recipe. `source` is the config.yml `configs` path
/// for the tool (the `from` for symlink/copy), if declared.
pub fn setup(recipe: &Recipe, source: Option<&Path>) -> RunResult {
    let Some(s) = &recipe.setup else {
        return RunResult::NotSupported;
    };
    let from = source.map(|p| PathBuf::from(qwert_yml::expand_tilde(&*p.to_string_lossy())));
    run_setup_section(&recipe.meta.name, s, from)
}

/// Run inline setup defined in config.yml. Its own `from` wins; otherwise falls
/// back to the profile's `configs` source.
pub fn setup_inline(name: &str, inline: &qwert_yml::InlineSetup, source: Option<&Path>) -> RunResult {
    use crate::recipe::schema::{Commands, RecipeSetup, SetupUndo};

    fn to_commands(s: &qwert_yml::StringOrList) -> Commands {
        match s {
            qwert_yml::StringOrList::One(cmd) => Commands::One(cmd.clone()),
            qwert_yml::StringOrList::Many(cmds) => Commands::Many(cmds.clone()),
        }
    }

    let recipe_setup = RecipeSetup {
        from: inline.from.clone(),
        to: inline.to.clone(),
        symlink: inline.symlink,
        dest: None,
        macos: inline.macos.as_ref().map(to_commands),
        debian: inline.debian.as_ref().map(to_commands),
        arch: inline.arch.as_ref().map(to_commands),
        linux: inline.linux.as_ref().map(to_commands),
        undo: inline.undo.as_ref().map(|u| SetupUndo {
            macos: u.macos.as_ref().map(to_commands),
            debian: u.debian.as_ref().map(to_commands),
            arch: u.arch.as_ref().map(to_commands),
            linux: u.linux.as_ref().map(to_commands),
        }),
    };

    let from = inline
        .from
        .as_ref()
        .map(|f| PathBuf::from(qwert_yml::expand_tilde(f)))
        .or_else(|| source.map(|p| PathBuf::from(qwert_yml::expand_tilde(&*p.to_string_lossy()))));
    run_setup_section(name, &recipe_setup, from)
}

/// Run inline setup and print status to terminal. Returns true on success.
pub fn setup_inline_with_output(name: &str, inline: &qwert_yml::InlineSetup, source: Option<&Path>) -> bool {
    match setup_inline(name, inline, source) {
        RunResult::NotSupported => true,
        RunResult::AlreadyInstalled { .. } => {
            printer::ok(name, "setup already done");
            true
        }
        RunResult::Installed => {
            printer::ok(name, "setup applied");
            true
        }
        RunResult::Failed(err) => {
            printer::failed(name, &format!("setup failed: {}", err));
            false
        }
    }
}

/// Run setup and print status to terminal. Returns true on success.
pub fn setup_with_output(recipe: &Recipe, source: Option<&Path>) -> bool {
    let name = &recipe.meta.name;
    match setup(recipe, source) {
        RunResult::NotSupported => true,
        RunResult::AlreadyInstalled { .. } => {
            printer::ok(name, "setup already done");
            true
        }
        RunResult::Installed => {
            printer::ok(name, "setup applied");
            true
        }
        RunResult::Failed(err) => {
            printer::failed(name, &format!("setup failed: {}", err));
            false
        }
    }
}

/// Undo the setup phase for a recipe (used by drop/uninstall).
pub fn undo_setup(recipe: &Recipe, _config_dir: &Path) -> RunResult {
    let Some(s) = &recipe.setup else {
        return RunResult::NotSupported;
    };

    let platform = platform::detect();
    let dest = PathBuf::from(qwert_yml::expand_tilde(&s.to));

    // Commands-based: run undo commands
    let cmds = s.setup_cmds_for(&platform);
    if !cmds.is_empty() {
        let undo_cmds = s.undo_cmds_for(&platform);
        if undo_cmds.is_empty() {
            return RunResult::Failed(format!(
                "no undo commands defined for {} — undo setup manually",
                recipe.meta.name
            ));
        }
        for cmd in undo_cmds {
            if let Err(e) = platform::run_cmd(cmd) {
                return RunResult::Failed(e.to_string());
            }
        }
        return RunResult::Installed;
    }

    // Symlink: just remove it
    if s.symlink {
        if dest.is_symlink() {
            if let Err(e) = std::fs::remove_file(&dest) {
                return RunResult::Failed(e.to_string());
            }
        }
        return RunResult::Installed;
    }

    // Copy: backup then remove
    if dest.exists() {
        let backup_dir = crate::platform::data_dir()
            .join("backups")
            .join(&recipe.meta.name);

        let filename = dest.file_name().unwrap_or_default();
        let backup_path = backup_dir.join(filename);

        if let Err(e) = pfs::copy_file(&dest, &backup_path) {
            return RunResult::Failed(format!("backup failed: {}", e));
        }
        if let Err(e) = std::fs::remove_file(&dest) {
            return RunResult::Failed(e.to_string());
        }
        printer::info(&format!("backup saved to {}", backup_path.display()));
    }

    RunResult::Installed
}

/// Undo setup and print status to terminal. Returns true on success.
pub fn undo_setup_with_output(recipe: &Recipe, config_dir: &Path) -> bool {
    let name = &recipe.meta.name;
    match undo_setup(recipe, config_dir) {
        RunResult::NotSupported => true,
        RunResult::AlreadyInstalled { .. } => true,
        RunResult::Installed => {
            printer::ok(name, "setup undone");
            true
        }
        RunResult::Failed(err) => {
            printer::failed(name, &format!("undo setup: {}", err));
            false
        }
    }
}

/// Returns a static label for setup status
pub fn setup_status_label(setup: &RecipeSetup, source: Option<&Path>) -> &'static str {
    let platform = platform::detect();
    let dest = PathBuf::from(qwert_yml::expand_tilde(&setup.to));

    // Commands-based: always show "configured" (no reliable idempotency check)
    if !setup.setup_cmds_for(&platform).is_empty() {
        return "configured";
    }

    if setup.symlink {
        let from = source
            .map(|p| PathBuf::from(qwert_yml::expand_tilde(&*p.to_string_lossy())))
            .unwrap_or_else(|| PathBuf::new());
        if dest.is_symlink() && std::fs::read_link(&dest).ok().as_deref() == Some(from.as_path()) {
            return "linked";
        }
        return "not linked";
    }

    if dest.exists() { "copied" } else { "not copied" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::schema::{RecipeMeta, RecipeKind, RecipeCheck, Commands};
    use std::fs;

    fn make_recipe_check_cmd(cmd: &str) -> Recipe {
        let mut r = make_recipe_with_setup(None);
        r.check = Some(RecipeCheck { command: None, version_flag: None, cmd: Some(cmd.into()) });
        r
    }

    #[test]
    fn is_installed_via_cmd_returns_true_when_exits_zero() {
        // arrange
        let recipe = make_recipe_check_cmd("true");
        // act
        let result = is_installed(&recipe);
        // assert
        assert!(result);
    }

    #[test]
    fn is_installed_via_cmd_returns_false_when_exits_nonzero() {
        // arrange
        let recipe = make_recipe_check_cmd("false");
        // act
        let result = is_installed(&recipe);
        // assert
        assert!(!result);
    }

    #[test]
    fn is_installed_returns_false_when_no_check() {
        // arrange
        let mut recipe = make_recipe_with_setup(None);
        recipe.check = None;
        // act
        let result = is_installed(&recipe);
        // assert
        assert!(!result);
    }

    fn make_recipe_with_setup(setup: Option<RecipeSetup>) -> Recipe {
        Recipe {
            meta: RecipeMeta {
                name: "test".into(),
                version: "1.0.0".into(),
                description: "test".into(),
                kind: RecipeKind::Custom,
                depends: vec![],
                packages: None,
                pkg: None,
                platforms: None,
            },
            check: Some(RecipeCheck { command: Some("test-nonexistent-binary".into()), version_flag: None, cmd: None }),
            install: None,
            upgrade: None,
            uninstall: None,
            setup,
            local: false,
            setup_only: false,
        }
    }

    fn make_setup(to: &str, symlink: bool, from: Option<&str>) -> RecipeSetup {
        RecipeSetup {
            from: from.map(|s| s.to_string()),
            to: to.to_string(),
            symlink,
            dest: None,
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        }
    }

    #[test]
    fn setup_symlink_fails_when_no_source_declared() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_no_source");
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("dest.conf");
        let s = make_setup(dest.to_str().unwrap(), true, None);
        let recipe = make_recipe_with_setup(Some(s));
        // act — no source from config.yml
        let result = setup(&recipe, None);
        // assert — recipe no longer carries a `from`; source must come from config
        assert!(matches!(result, RunResult::Failed(_)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setup_returns_not_supported_when_no_setup_section() {
        // arrange
        let recipe = make_recipe_with_setup(None);
        // act
        let result = setup(&recipe, None);
        // assert
        assert!(matches!(result, RunResult::NotSupported));
    }

    #[test]
    fn setup_symlink_creates_symlink_at_dest() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_symlink");
        fs::create_dir_all(&dir).unwrap();
        let src_dir = dir.join("config");
        fs::create_dir_all(&src_dir).unwrap();
        let src_file = src_dir.join("tmux");
        fs::write(&src_file, "config content").unwrap();
        let dest = dir.join("dest").join(".tmux.conf");

        let s = RecipeSetup {
            from: Some(src_file.to_str().unwrap().to_string()),
            to: dest.to_str().unwrap().to_string(),
            symlink: true,
            dest: Some(DestKind::File),
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        let recipe = make_recipe_with_setup(Some(s));
        // act — source is the config.yml configs path for the tool
        let result = setup(&recipe, Some(&src_file));
        // assert
        assert!(matches!(result, RunResult::Installed));
        assert!(dest.is_symlink());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setup_validation_rejects_file_source_for_directory_dest() {
        // arrange — recipe expects a directory, but configs points at a file
        let dir = std::env::temp_dir().join("qwert_runner_test_kind_mismatch");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("init.lua");
        fs::write(&file, "data").unwrap();
        let dest = dir.join("dest");

        let s = RecipeSetup {
            from: None,
            to: dest.to_str().unwrap().to_string(),
            symlink: true,
            dest: Some(DestKind::Dir),
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        let recipe = make_recipe_with_setup(Some(s));
        // act — source is a file but dest needs a directory
        let result = setup(&recipe, Some(&file));
        // assert — fails with a clear kind-mismatch error, no broken symlink
        assert!(matches!(result, RunResult::Failed(_)));
        assert!(!dest.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setup_symlink_idempotent_when_already_linked() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_idempotent");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src_file");
        let dest = dir.join("dest_link");
        fs::write(&src, "data").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, &dest).unwrap();

        let s = RecipeSetup {
            from: Some(src.to_str().unwrap().to_string()),
            to: dest.to_str().unwrap().to_string(),
            symlink: true,
            dest: Some(DestKind::File),
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        let recipe = make_recipe_with_setup(Some(s));
        // act — source is already-linked src
        let result = setup(&recipe, Some(&src));
        // assert
        assert!(matches!(result, RunResult::AlreadyInstalled { .. }));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setup_copy_returns_already_installed_when_dest_exists() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_copy_exists");
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("dest.conf");
        fs::write(&dest, "existing").unwrap();

        let s = make_setup(dest.to_str().unwrap(), false, None);
        let recipe = make_recipe_with_setup(Some(s));
        // act — dest exists → AlreadyInstalled (source irrelevant)
        let result = setup(&recipe, None);
        // assert
        assert!(matches!(result, RunResult::AlreadyInstalled { .. }));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setup_copy_fails_when_src_missing() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_copy_fail");
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("dest.conf");

        let s = make_setup(dest.to_str().unwrap(), false, None);
        let recipe = make_recipe_with_setup(Some(s));
        // act — no source declared
        let result = setup(&recipe, None);
        // assert — copy needs a source from config.yml
        assert!(matches!(result, RunResult::Failed(_)));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setup_copy_copies_file_to_dest() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_copy_ok");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.conf");
        let dest = dir.join("dest.conf");
        fs::write(&src, "my config").unwrap();

        let s = RecipeSetup {
            from: Some(src.to_str().unwrap().to_string()),
            to: dest.to_str().unwrap().to_string(),
            symlink: false,
            dest: None,
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        let recipe = make_recipe_with_setup(Some(s));
        // act
        let result = setup(&recipe, Some(&src));
        // assert
        assert!(matches!(result, RunResult::Installed));
        assert_eq!(fs::read_to_string(&dest).unwrap(), "my config");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn undo_setup_removes_symlink() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_undo_symlink");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src");
        let dest = dir.join("link");
        fs::write(&src, "data").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, &dest).unwrap();

        let s = RecipeSetup {
            from: Some(src.to_str().unwrap().to_string()),
            to: dest.to_str().unwrap().to_string(),
            symlink: true,
            dest: Some(DestKind::File),
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        let recipe = make_recipe_with_setup(Some(s));
        // act
        let result = undo_setup(&recipe, &dir);
        // assert
        assert!(matches!(result, RunResult::Installed));
        assert!(!dest.is_symlink());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn undo_setup_backs_up_and_removes_copy() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_undo_copy");
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("dest.conf");
        fs::write(&dest, "config data").unwrap();

        let s = RecipeSetup {
            from: None,
            to: dest.to_str().unwrap().to_string(),
            symlink: false,
            dest: None,
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        let mut recipe = make_recipe_with_setup(Some(s));
        recipe.meta.name = "mytest".into();
        // act
        let result = undo_setup(&recipe, &dir);
        // assert
        assert!(matches!(result, RunResult::Installed));
        assert!(!dest.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn undo_setup_fails_when_commands_type_and_no_undo_defined() {
        // arrange
        let s = RecipeSetup {
            from: None,
            to: "~/.config/iterm2".into(),
            symlink: false,
            dest: None,
            macos: Some(Commands::One("defaults write com.foo bar".into())),
            debian: Some(Commands::One("echo debian-setup".into())),
            arch: None,
            linux: None,
undo: None,
        };
        let recipe = make_recipe_with_setup(Some(s));
        // act
        let result = undo_setup(&recipe, std::path::Path::new("/tmp"));
        // assert
        assert!(matches!(result, RunResult::Failed(_)));
    }

    #[test]
    fn setup_status_label_returns_dash_when_no_section() {
        // arrange
        let recipe = make_recipe_with_setup(None);
        // act
        let label = recipe.setup.as_ref()
            .map(|s| setup_status_label(s, None))
            .unwrap_or("—");
        // assert
        assert_eq!(label, "—");
    }

    #[test]
    fn setup_status_label_returns_not_linked_when_no_symlink() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_label");
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("nonexistent.conf");
        let s = make_setup(dest.to_str().unwrap(), true, None);
        let recipe = make_recipe_with_setup(Some(s));
        // act
        let label = recipe.setup.as_ref()
            .map(|s| setup_status_label(s, None))
            .unwrap_or("—");
        // assert
        assert_eq!(label, "not linked");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setup_status_label_returns_linked_when_correct_symlink_exists() {
        // arrange
        let dir = std::env::temp_dir().join("qwert_runner_test_label_linked");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test");
        let dest = dir.join("link.conf");
        fs::write(&src, "data").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, &dest).unwrap();

        let s = RecipeSetup {
            from: Some(src.to_str().unwrap().to_string()),
            to: dest.to_str().unwrap().to_string(),
            symlink: true,
            dest: Some(DestKind::File),
            macos: None,
            debian: None,
            arch: None,
            linux: None,
undo: None,
        };
        let recipe = make_recipe_with_setup(Some(s));
        // act
        let label = recipe.setup.as_ref()
            .map(|sl| setup_status_label(sl, Some(&src)))
            .unwrap_or("—");
        // assert
        assert_eq!(label, "linked");
        fs::remove_dir_all(&dir).ok();
    }
}
