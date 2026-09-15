use anyhow::Result;
use std::path::PathBuf;

pub mod fs;
pub mod impls;
pub mod shared;

/// Runtime data directory for qwert: ~/.local/share/qwert/
pub fn data_dir() -> PathBuf {
    dirs::home_dir()
        .expect("no home dir")
        .join(".local/share/qwert")
}

/// True when running with an effective uid of root.
fn is_root() -> bool {
    #[cfg(unix)]
    {
        std::fs::metadata("/proc/self")
            .as_ref()
            .map(|m| std::os::unix::fs::MetadataExt::uid(m) == 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Privilege preflight for setup commands. Commands starting with `sudo `
/// need elevated credentials: prime the sudo timestamp cache once so a whole
/// recipe setup only prompts a single time.
///
/// - already root            → proceed
/// - cached sudo (sudo -n ok)→ proceed
/// - interactive TTY         → run `sudo -v` once so the user types the
///                             password before any step is touched
/// - no TTY                  → return a clear guidance error instead of a
///                             confusing `sudo: a terminal is required`
fn ensure_sudo_preflight(cmd: &str) -> Result<()> {
    if !cmd.trim_start().starts_with("sudo ") {
        return Ok(());
    }
    if is_root() {
        return Ok(());
    }
    use std::io::IsTerminal;
    let probe = std::process::Command::new("sudo")
        .args(["-n", "true"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let cached = matches!(probe, Ok(s) if s.success());
    if cached {
        return Ok(());
    }
    if std::io::stdin().is_terminal() {
        crate::ui::printer::info(
            "elevated privileges needed — sudo will ask for the password once (sudo -v)",
        );
        let ok = std::process::Command::new("sudo")
            .arg("-v")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        return if ok {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "sudo password required but not provided — aborted before any change"
            ))
        };
    }
    Err(anyhow::anyhow!(
        "command needs elevated privileges and there is no TTY for the sudo prompt — \
         run `sudo -v` in a terminal (then retry), or run qwert inside your shell"
    ))
}

/// Platform-specific installation conventions (paths, completions, shell config).
pub trait InstallerOps {
    /// /opt/qwert/bin/qwert
    fn binary_path(&self) -> PathBuf;

    /// /usr/local/bin/qwert
    fn symlink_path(&self) -> PathBuf;

    /// System zsh completion path (e.g. /usr/local/share/zsh/site-functions/_qwert)
    fn zsh_completion_path(&self) -> PathBuf;

    /// System bash completion path — None on platforms where bash completions are not standard
    fn bash_completion_path(&self) -> Option<PathBuf>;

    /// Shell rc file candidates in priority order (first existing file wins)
    fn shell_rc_candidates(&self) -> Vec<PathBuf>;

    /// Install shell completions to system paths (requires sudo)
    fn install_completions(&self) -> Result<()>;

    /// Inject qwert hooks into the user's shell rc. Returns the rc file path used.
    fn configure_shell(&self) -> Result<PathBuf>;
}

/// Returns the platform-specific installer implementation, based on the real OS
/// (qwert's own installation layout), never on a yuiop platform override.
pub fn installer() -> Box<dyn InstallerOps> {
    if cfg!(target_os = "macos") {
        return Box::new(impls::macos::MacOS);
    }
    if std::path::Path::new("/usr/bin/pacman").exists() {
        return Box::new(impls::arch::Arch);
    }
    if std::path::Path::new("/usr/bin/apt-get").exists() {
        return Box::new(impls::debian::Debian);
    }
    Box::new(impls::linux::Linux)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Platform {
    MacOS,
    /// Debian-based Linux (Ubuntu, Debian, etc.) — uses apt-get
    Debian,
    /// Arch-based Linux (Arch, Manjaro, etc.) — uses pacman
    Arch,
    Unknown,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::MacOS => write!(f, "macOS"),
            Platform::Debian => write!(f, "Debian Linux"),
            Platform::Arch => write!(f, "Arch Linux"),
            Platform::Unknown => write!(f, "unknown"),
        }
    }
}

impl Platform {
    /// Lowercase identifier matching recipe TOML section names / `platforms` values.
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::MacOS => "macos",
            Platform::Debian => "debian",
            Platform::Arch => "arch",
            Platform::Unknown => "unknown",
        }
    }
}

/// Map a yuiop platform name (brew|apt|pacman) to a qwert Platform.
pub fn platform_for_pm(pm: Option<&str>) -> Platform {
    match pm {
        Some("brew") => Platform::MacOS,
        Some("apt") => Platform::Debian,
        Some("pacman") => Platform::Arch,
        _ => Platform::Unknown,
    }
}

/// The effective platform, per yuiop.
///
/// `yuiop` is the single owner of platform detection (it picks brew on macOS,
/// apt on Debian, pacman on Arch). qwert only needs the platform name to select
/// the right `macos`/`debian`/`arch` section of custom recipes and setups — it
/// never maps a platform to a package manager itself.
pub fn detect() -> Platform {
    platform_for_pm(crate::adapters::yuiop::platform_name().as_deref())
}

/// Execute a shell command, streaming stdout/stderr to terminal
pub fn run_cmd(cmd: &str) -> Result<()> {
    ensure_sudo_preflight(cmd)?;
    let status = std::process::Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("command failed: {}", cmd)
    }
}

/// Execute a shell command, capturing stderr; on failure returns stderr content
pub fn run_cmd_capture(cmd: &str) -> Result<(), String> {
    let out = std::process::Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("command failed: {}", cmd)
        } else {
            stderr
        })
    }
}

/// Check if a binary exists on PATH (portable across macOS and Linux).
pub fn which(binary: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_escape(binary)))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Minimal escaping for a single-word binary name in a `sh -c` string.
fn shell_escape(word: &str) -> String {
    if word.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/' || c == '.') {
        word.to_string()
    } else {
        format!("{:?}", word)
    }
}

/// Get the installed version of a binary
pub fn version_of(binary: &str, flag: &str) -> Option<String> {
    std::process::Command::new(binary)
        .arg(flag)
        .output()
        .ok()
        .and_then(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}{}", stdout, stderr);
            combined.lines().next().map(|l| l.trim().to_string())
        })
}

/// Ensure qwert shell hooks are present in the user's rc file.
/// No-op if hooks are already there.
pub fn ensure_shell() -> anyhow::Result<()> {
    let inst = installer();
    let rc = shared::resolve_rc(&inst.shell_rc_candidates())?;
    shared::ensure_shell_hooks(&rc)
}

#[cfg(test)]
#[path = "tests/platform.rs"]
mod tests;