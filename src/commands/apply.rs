use anyhow::Result;

use crate::config::{machine, qwert_yml, state_yml};
use crate::recipe::{index, runner};
use crate::ui::printer;

pub fn run(tool: Option<&str>, dry_run: bool) -> Result<()> {
    let manifest_path = qwert_yml::manifest_path();
    let state_path = state_yml::state_path();

    let config = qwert_yml::QwertConfig::load(&manifest_path)?;
    let mut state = state_yml::QwertState::load(&state_path)?;
    let machine_identity = machine::MachineIdentity::load()?;
    let profile = machine_identity.active_profile().to_string();

    // Ensure the default catalog and any declared plugins are available before
    // resolving recipes — replicates the environment on a fresh machine.
    crate::commands::recipes_cmd::update_silent();
    crate::plugins::ensure_clones().ok();

    let recipes_dir = index::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;

    // Ensure a fresh machine (or one whose recipes cache landed elsewhere) has the
    // recipe index before resolving anything. Best-effort: offline usage is unaffected.
    crate::commands::recipes_cmd::update_silent();
    crate::plugins::ensure_clones().ok();

    printer::h1("Applying machine setup...");
    if !config.profiles.contains_key(&profile) {
        printer::warning(&format!(
            "profile '{}' is not declared in qwert.yml (available: {})",
            profile,
            config.profile_names().join(", ")
        ));
    }
    printer::info(&format!("profile: {}", profile));
    printer::blank();

    // Tools active for this machine's profile.
    let active_names = config.tool_names_for_profile(&profile);

    let mut install_done = 0;
    let mut install_failed = 0;
    let mut install_skipped: Vec<String> = Vec::new();
    let mut setup_done = 0;
    let mut setup_failed = 0;
    let mut setup_skipped: Vec<String> = Vec::new();
    let mut orphan_done = 0;
    let mut orphan_failed = 0;

    // ---- Phase 1: uninstall orphans (installed but no longer active) ----
    if tool.is_none() {
        let orphans: Vec<String> = state
            .orphans(&active_names)
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        if !orphans.is_empty() {
            printer::h2("Uninstalling orphans");
            printer::blank();
        }

        for name in &orphans {
            if dry_run {
                printer::bullet(&format!("would uninstall: {}", name));
                continue;
            }
            // The system's own package manager (brew/apt/pacman) is never a tool —
            // it may appear in state from a bootstrap step. Keep it, just untrack.
            if crate::adapters::is_package_manager(name) {
                state.mark_removed(name);
                printer::ok(name, "kept (package manager)");
                orphan_done += 1;
                continue;
            }
            match index::find(name, &recipes_dir) {
                Some(recipe) => {
                    if runner::uninstall_with_output(&recipe) {
                        state.mark_removed(name);
                        orphan_done += 1;
                    } else {
                        orphan_failed += 1;
                    }
                }
                None => {
                    match crate::adapters::yuiop::uninstall(name) {
                        Ok(_) => {
                            state.mark_removed(name);
                            printer::ok(name, "uninstalled");
                            orphan_done += 1;
                        }
                        Err(e) => {
                            printer::failed(name, &format!("uninstall failed: {e}"));
                            orphan_failed += 1;
                        }
                    }
                }
            }
        }
        if !orphans.is_empty() {
            printer::blank();
        }
    }

    let tools: Vec<&str> = if let Some(t) = tool {
        vec![t]
    } else {
        active_names.iter().map(|s| s.as_str()).collect()
    };

    if tools.is_empty() && state.orphans(&active_names).is_empty() {
        printer::info("No tools declared. Run `qwert use <tool>` to add one.");
        return Ok(());
    }

    // ---- Phase 2: install all tools ----
    if dry_run {
        printer::h2("Would install");
        printer::blank();
        for name in &tools {
            printer::bullet(name);
        }
        printer::blank();
        printer::h2("Would setup");
        printer::blank();
        for name in &tools {
            printer::bullet(name);
        }
        printer::blank();
        return Ok(());
    }

    printer::h2("Installing tools");
    printer::blank();

    for name in &tools {
        match index::find(name, &recipes_dir) {
            Some(recipe) => {
                if recipe.setup_only {
                    // Setup-only recipes (e.g. local agents/skills) skip install.
                    printer::ok(name, "setup-only (no package)");
                    install_done += 1;
                    continue;
                }
                let installed = runner::install_with_output(&recipe, &recipes_dir);
                match installed {
                    runner::Outcome::Ok => {
                        let version = runner::installed_version(&recipe);
                        state.mark_installed(name, version.as_deref());
                        install_done += 1;
                    }
                    runner::Outcome::Failed => install_failed += 1,
                    runner::Outcome::Skipped => install_skipped.push(name.to_string()),
                }
            }
            None => {
                if crate::platform::which(name) {
                    let version = crate::platform::version_of(name, "--version");
                    state.mark_installed(name, version.as_deref());
                    printer::ok(name, "already installed");
                    install_done += 1;
                } else {
                    match crate::adapters::yuiop::install(name) {
                        Ok(_) => {
                            let version = crate::platform::version_of(name, "--version");
                            state.mark_installed(name, version.as_deref());
                            printer::ok(name, "installed");
                            install_done += 1;
                        }
                        Err(e) => {
                            printer::failed(name, &e);
                            install_failed += 1;
                        }
                    }
                }
            }
        }
    }

    printer::blank();

    // ---- Phase 3: setup all tools ----
    printer::h2("Setting up tools");
    printer::blank();

    for name in &tools {
        let source = config
            .config_source_for(&profile, name)
            .map(|s| std::path::PathBuf::from(qwert_yml::expand_tilde(s)));
        let has_setup = match index::find(name, &recipes_dir) {
            Some(recipe) => {
                if recipe.setup.is_some() {
                    runner::setup_with_output(&recipe, source.as_deref())
                } else if let Some(inline) = config.inline_setup_of(name) {
                    runner::setup_inline_with_output(name, inline, source.as_deref())
                } else {
                    runner::Outcome::Ok
                }
            }
            None => {
                if let Some(inline) = config.inline_setup_of(name) {
                    runner::setup_inline_with_output(name, inline, source.as_deref())
                } else {
                    runner::Outcome::Ok
                }
            }
        };
        match has_setup {
            runner::Outcome::Ok => setup_done += 1,
            runner::Outcome::Failed => setup_failed += 1,
            runner::Outcome::Skipped => setup_skipped.push(name.to_string()),
        }
    }

    printer::blank();

    state.save(&state_path)?;
    if orphan_done > 0 {
        let msg = if orphan_failed > 0 {
            format!("{} orphan(s) uninstalled, {} failed", orphan_done, orphan_failed)
        } else {
            format!("{} orphan(s) uninstalled", orphan_done)
        };
        printer::info(&msg);
    }
    let install_total = install_done + install_failed;
    let setup_total = setup_done + setup_failed;
    if install_total > 0 || !install_skipped.is_empty() {
        printer::summary_phase_skip(
            "install",
            install_done,
            install_failed,
            install_skipped.len(),
        );
    }
    if setup_total > 0 || !setup_skipped.is_empty() {
        printer::summary_phase_skip(
            "setup",
            setup_done,
            setup_failed,
            setup_skipped.len(),
        );
    }

    // Nominal list of skipped tools (unsupported platform / no steps).
    let mut skipped_all: Vec<String> = install_skipped;
    skipped_all.extend(setup_skipped);
    skipped_all.sort();
    skipped_all.dedup();
    if !skipped_all.is_empty() {
        printer::blank();
        printer::h2("Skipped");
        for name in &skipped_all {
            printer::bullet(&format!("{} — not supported on this platform", name));
        }
    }

    Ok(())
}