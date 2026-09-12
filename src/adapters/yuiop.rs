//! yuiop — the single package-management engine behind qwert.
//!
//! `qwert` + `yuiop` = `QWERTYUIOP`. qwert delegates every system-package
//! operation to the `yuiop` binary (`yuiop <verb> <canonical> --json`) and parses
//! its JSON output. qwert passes the recipe's *canonical* name only; yuiop detects
//! the platform and resolves the per-manager package (brew/apt/pacman). qwert has
//! no package-manager knowledge of its own — no brewing, no apt, no pacman here.
//!
//! See the yuiop contract: `https://github.com/br4zz4/yuiop/blob/main/docs/CONTRACT.md`

use std::sync::OnceLock;

use crate::platform;
use crate::recipe::schema::RecipeKind;

/// Where to install yuiop for users without it on PATH (mirrors yuiop's install.sh).
const FALLBACK_DIR: &str = ".local/bin/yuiop";

/// Message shown when the yuiop binary is missing.
const MISSING_HINT: &str = "yuiop binary not found — install it with:\n  curl -fsSL https://raw.githubusercontent.com/br4zz4/yuiop/main/install.sh | bash";

/// Is the yuiop binary available to run?
pub fn available() -> bool {
    binary().is_some()
}

/// Resolved path to the yuiop binary (PATH first, then ~/.local/bin/yuiop).
pub fn binary() -> Option<String> {
    if platform::which("yuiop") {
        return Some("yuiop".to_string());
    }
    let fallback = dirs::home_dir()?.join(FALLBACK_DIR);
    if fallback.exists() {
        return Some(fallback.to_string_lossy().into_owned());
    }
    None
}

/// Which recipe kinds resolve through yuiop as system packages.
pub fn is_package_kind(kind: &RecipeKind) -> bool {
    kind.is_package()
}

/// Run `yuiop --json <cmd> <args...>` and capture stdout.
///
/// Ok(stdout) on exit 0. Err on any other exit — the message is yuiop's own
/// stderr (e.g. "no knowledge of package 'x'"), or a synthesized error when the
/// binary is missing or cannot run.
fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let bin = binary().ok_or_else(|| MISSING_HINT.to_string())?;
    let out = std::process::Command::new(&bin)
        .arg("--json")
        .arg(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("yuiop: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        code => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if stderr.is_empty() {
                Err(format!("yuiop {cmd} failed (exit {})", code.unwrap_or(-1)))
            } else {
                Err(translate_error(cmd, args, &stderr))
            }
        }
    }
}

/// Rewrite yuiop's terse errors into actionable qwert messages.
/// Falls back to the original message when the pattern isn't recognized.
fn translate_error(cmd: &str, args: &[&str], stderr: &str) -> String {
    // "no knowledge of package 'claude'" — the canonical isn't in yuiop's
    // provider database for this platform. Tell the user what to do.
    if let Some(pkg) = stderr.strip_prefix("no knowledge of package '").and_then(|s| s.strip_suffix('\'')) {
        let platform = platform_name().unwrap_or_else(|| "this platform".to_string());
        return format!(
            "package '{}' not available for {} ({}) — install it manually or add a custom recipe in ~/.qwert/recipes/{}/install.toml",
            pkg, platform_display(&platform), platform, pkg
        );
    }
    let _ = (cmd, args);
    stderr.to_string()
}

/// "brew" → "macOS", "pacman" → "Arch Linux", "apt" → "Debian Linux".
fn platform_display(pm: &str) -> &'static str {
    match pm {
        "brew" => "macOS",
        "pacman" => "Arch Linux",
        "apt" => "Debian Linux",
        _ => "Linux",
    }
}

/// Install a system package by canonical name.
pub fn install(canonical: &str) -> Result<(), String> {
    run("install", &[canonical]).map(|_| ())
}

/// Remove a system package by canonical name.
pub fn uninstall(canonical: &str) -> Result<(), String> {
    run("remove", &[canonical]).map(|_| ())
}

/// Upgrade a system package by canonical name.
pub fn upgrade(canonical: &str) -> Result<(), String> {
    run("upgrade", &[canonical]).map(|_| ())
}

/// Is a canonical package installed, per yuiop's provider database?
///
/// `None` when yuiop is unavailable or does not know the package — the caller
/// falls back to a `which` check.
pub fn status(canonical: &str) -> Option<bool> {
    let out = run("status", &[canonical]).ok()?;
    parse_bool(&out, "installed")
}

/// Is a recipe's package already installed? `None` when unresolved (caller
/// falls back to `which`).
pub fn registered(meta: &crate::recipe::schema::RecipeMeta) -> Option<bool> {
    status(&meta.name)
}

/// The effective platform name (brew|apt|pacman) according to yuiop, cached
/// for the process lifetime.
pub fn platform_name() -> Option<String> {
    static PM: OnceLock<Option<String>> = OnceLock::new();
    PM.get_or_init(|| {
        let out = run("platform", &[]).ok()?;
        parse_string(&out, "platform")
    })
    .clone()
}

/// Persist a platform override for this machine (yuiop owns the config).
pub fn set_platform(name: &str) -> Result<(), String> {
    let bin = binary().ok_or_else(|| MISSING_HINT.to_string())?;
    let out = std::process::Command::new(&bin)
        .args(["--json", "platform", name])
        .output()
        .map_err(|e| format!("yuiop: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Human label for a recipe's manager: the yuiop platform name for package
/// recipes, otherwise the recipe kind (custom/qwert/…). Used in status output.
pub fn resolve_label(meta: &crate::recipe::schema::RecipeMeta) -> String {
    if is_package_kind(&meta.kind) {
        platform_name().unwrap_or_else(|| meta.kind.to_string())
    } else {
        meta.kind.to_string()
    }
}

/// Search packages in the yuiop provider. Exit 3 (no hits) yields `vec![]`.
pub fn search(term: &str) -> Vec<String> {
    let Some(bin) = binary() else { return vec![] };
    let out = std::process::Command::new(&bin)
        .args(["--json", "search", term])
        .output();
    let Ok(out) = out else { return vec![] };
    if !out.status.success() {
        return vec![];
    }
    parse_strings(&String::from_utf8_lossy(&out.stdout), "matches")
}

// --- JSON parsing -----------------------------------------------------------
// The yuiop contract defines stable, one-document JSON. Parsing by hand keeps
// qwert free of a JSON dependency; shapes are tiny and additive-tolerant.

/// `{ ..., "key": true }` → Some(bool). Returns None when absent/unparseable.
fn parse_bool(json: &str, key: &str) -> Option<bool> {
    let pattern = format!("\"{key}\":");
    let idx = json.find(&pattern)?;
    let rest = &json[idx + pattern.len()..];
    let token = rest.split([',', '}']).next()?.trim();
    match token {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `{ ..., "key": "value" }` → Some("value"). Returns None when absent.
fn parse_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":");
    let idx = json.find(&pattern)?;
    let rest = &json[idx + pattern.len()..];
    let token = rest.trim_start();
    let quoted = token.strip_prefix('"')?;
    let end = quoted.find('"')?;
    Some(quoted[..end].to_string())
}

/// `{ ..., "key": ["a", "b"] }` → vec!["a", "b"]. Stops at the closing `]` so
/// later keys (e.g. "platform", "term") never leak into the list.
fn parse_strings(json: &str, key: &str) -> Vec<String> {
    let pattern = format!("\"{key}\":");
    let Some(idx) = json.find(&pattern) else { return vec![] };
    let rest = &json[idx + pattern.len()..];
    let rest = rest.trim_start();
    let Some(list) = rest.strip_prefix('[') else { return vec![] };
    let Some(end) = list.find(']') else { return vec![] };
    let mut out = Vec::new();
    for token in list[..end].split(',') {
        if let Some(quoted) = token.trim().strip_prefix('"') {
            if let Some(stop) = quoted.find('"') {
                out.push(quoted[..stop].to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_finds_installed_field() {
        // arrange
        let json = r#"{"platform":"brew","package":"tmux","installed":true}"#;
        // act
        let installed = parse_bool(json, "installed");
        // assert
        assert_eq!(installed, Some(true));
    }

    #[test]
    fn parse_bool_missing_field_is_none() {
        // arrange
        let json = r#"{"platform":"brew","packages":["delta","tmux"]}"#;
        // act
        let installed = parse_bool(json, "installed");
        // assert
        assert_eq!(installed, None);
    }

    #[test]
    fn parse_bool_false_when_not_installed() {
        // arrange
        let json = r#"{"platform":"apt","package":"delta","installed": false}"#;
        // act
        let installed = parse_bool(json, "installed");
        // assert
        assert_eq!(installed, Some(false));
    }

    #[test]
    fn parse_string_extracts_platform() {
        // arrange
        let json = r#"{ "platform": "pacman" }"#;
        // act
        let platform = parse_string(json, "platform");
        // assert
        assert_eq!(platform.as_deref(), Some("pacman"));
    }

    #[test]
    fn parse_strings_extracts_matches() {
        // arrange
        let json = r#"{"platform":"brew","term":"delta","matches":["delta","git-delta"]}"#;
        // act
        let matches = parse_strings(json, "matches");
        // assert
        assert_eq!(matches, vec!["delta", "git-delta"]);
    }

    #[test]
    fn parse_strings_ignores_keys_after_array() {
        // arrange — yuiop emits matches before platform/term; those keys must not leak
        let json = r#"{"matches":["fzf","ytfzf"],"platform":"pacman","term":"fzf"}"#;
        // act
        let matches = parse_strings(json, "matches");
        // assert
        assert_eq!(matches, vec!["fzf", "ytfzf"]);
    }

    #[test]
    fn parse_strings_handles_no_hits() {
        // arrange
        let json = r#"{"platform":"brew","term":"zzz","matches":[]}"#;
        // act
        let matches = parse_strings(json, "matches");
        // assert
        assert!(matches.is_empty());
    }

    #[test]
    fn parse_strings_handles_matches_last() {
        // arrange — array is the final field; no trailing comma after it
        let json = r#"{"platform":"brew","term":"delta","matches":["delta"]}"#;
        // act
        let matches = parse_strings(json, "matches");
        // assert
        assert_eq!(matches, vec!["delta"]);
    }

    #[test]
    fn package_kinds_are_package() {
        // arrange
        let kinds = [RecipeKind::Package, RecipeKind::Brew, RecipeKind::Apt, RecipeKind::Pacman];
        // act + assert
        for k in &kinds {
            assert!(is_package_kind(k), "{} should resolve via yuiop", k);
        }
    }

    #[test]
    fn custom_kinds_are_not_package() {
        // arrange
        let kinds = [RecipeKind::Custom, RecipeKind::Qwert];
        // act + assert
        for k in &kinds {
            assert!(!is_package_kind(k), "{} should be custom", k);
        }
    }

    #[test]
    fn translate_error_rewrites_unknown_package() {
        // arrange
        let stderr = "no knowledge of package 'claude'";
        // act
        let msg = translate_error("install", &["claude"], stderr);
        // assert — actionable message mentioning the platform and the fix
        assert!(msg.contains("package 'claude' not available"));
        assert!(msg.contains("add a custom recipe"));
        assert!(!msg.contains("no knowledge"));
    }

    #[test]
    fn translate_error_keeps_other_errors() {
        // arrange
        let stderr = "error: target not found: codex";
        // act
        let msg = translate_error("install", &["codex"], stderr);
        // assert — passthrough
        assert_eq!(msg, "error: target not found: codex");
    }

    #[test]
    fn platform_display_maps_pm_names() {
        // arrange + act + assert
        assert_eq!(platform_display("brew"), "macOS");
        assert_eq!(platform_display("pacman"), "Arch Linux");
        assert_eq!(platform_display("apt"), "Debian Linux");
        assert_eq!(platform_display("aur"), "Linux");
    }
}