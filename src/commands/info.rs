use anyhow::Result;

use crate::config::qwert_yml;
use crate::recipe::{index, runner, schema::RecipeSetup};
use crate::ui::printer;

fn setup_summary(setup: &RecipeSetup, source: Option<&std::path::Path>) -> String {
    let dest = &setup.to;
    let label = runner::setup_status_label(setup, source);
    let platform = crate::platform::detect();
    let kind = setup
        .dest
        .as_ref()
        .map(|k| format!(" ({})", k))
        .unwrap_or_default();

    if !setup.setup_cmds_for(&platform).is_empty() {
        format!("commands  [{}]", label)
    } else if setup.symlink {
        format!("symlink → {}{}  [{}]", dest, kind, label)
    } else {
        format!("copy → {}{}  [{}]", dest, kind, label)
    }
}

pub fn run(name: &str) -> Result<()> {
    let manifest_path = qwert_yml::manifest_path();
    let config = qwert_yml::QwertConfig::load(&manifest_path)?;
    let profile = crate::config::machine::MachineIdentity::load()?.active_profile().to_string();
    let recipes_dir = index::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    let source = config
        .config_source_for(&profile, name)
        .map(|s| std::path::PathBuf::from(qwert_yml::expand_tilde(s)));

    printer::h1(name);
    printer::blank();

    match index::find(name, &recipes_dir) {
        Some(recipe) => {
            let meta = &recipe.meta;

            if !meta.description.is_empty() {
                printer::field("description", &meta.description);
            }
            if !meta.version.is_empty() {
                printer::field("version", &meta.version);
            }
            printer::field("kind", &printer::kind_tag(&meta.kind.to_string()));

            // Platforms: "all" when unrestricted, or the declared list.
            let platforms = match &meta.platforms {
                Some(list) if !list.is_empty() => list.join(", "),
                _ => "all".to_string(),
            };
            printer::field("platforms", &platforms);

            if let Some(check) = &recipe.check {
                let cmd = if let Some(cmd_str) = &check.cmd {
                    cmd_str.clone()
                } else {
                    let base = check.command.as_deref().unwrap_or("");
                    match &check.version_flag {
                        Some(flag) => format!("{} {}", base, flag),
                        None => base.to_string(),
                    }
                };
                printer::field("check", &cmd);
            }

            let depends = if meta.depends.is_empty() {
                "—".to_string()
            } else {
                meta.depends.join(", ")
            };
            printer::field("depends", &depends);

            printer::blank();

            let installed_str = if runner::is_installed(&recipe) {
                let ver = runner::installed_version(&recipe)
                    .map(|v| format!(" {}", v))
                    .unwrap_or_default();
                format!("✓{}", ver)
            } else {
                "✗ not installed".to_string()
            };
            printer::field("installed", &installed_str);

            let setup_str = recipe.setup.as_ref()
                .map(|s| setup_summary(s, source.as_deref()))
                .unwrap_or_else(|| "—".to_string());
            printer::field("setup", &setup_str);

            let profiles = config.profiles_of_tool(name);
            if profiles.is_empty() {
                printer::field("declared", "no");
            } else {
                printer::field("declared", &format!("yes ({})", profiles.join(", ")));
            }
        }
        None => {
            printer::field("installed", if crate::platform::which(name) { "yes" } else { "no" });
            printer::field("recipe", "none");
            let profiles = config.profiles_of_tool(name);
            if profiles.is_empty() {
                printer::field("declared", "no");
            } else {
                printer::field("declared", &format!("yes ({})", profiles.join(", ")));
            }
        }
    }

    printer::blank();
    Ok(())
}
