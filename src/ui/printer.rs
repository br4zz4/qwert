use super::colors::{self, *};
use colors::ORANGE;

const TICK: &str = "✓";
const ARROW: &str = "→";
const CROSS: &str = "✗";
const BULLET: &str = "•";

fn is_tty() -> bool {
    #[cfg(unix)]
    {
        extern "C" { fn isatty(fd: i32) -> i32; }
        unsafe { isatty(1) != 0 }
    }
    #[cfg(not(unix))]
    { false }
}

fn use_color() -> bool {
    is_tty() && std::env::var("NO_COLOR").is_err()
}

fn colorize(color: &str, text: &str) -> String {
    if use_color() {
        format!("{}{}{}", color, text, RESET)
    } else {
        text.to_string()
    }
}

/// Colorize helpers for table cells — the ANSI codes wrap the padded text, so
/// the visible width is unaffected.
pub fn success_text(s: &str) -> String {
    colorize(SUCCESS, s)
}

pub fn error_text(s: &str) -> String {
    colorize(ERROR, s)
}

pub fn orange_text(s: &str) -> String {
    colorize(ORANGE, s)
}

pub fn bold_text(s: &str) -> String {
    colorize(BOLD_WHITE, s)
}

// --- Status line printers ---

/// ✓  tool_name   message
pub fn ok(name: &str, msg: &str) {
    ok_w(name, 12, msg);
}

pub fn ok_w(name: &str, width: usize, msg: &str) {
    let tick = colorize(SUCCESS, TICK);
    let name_col = colorize(BOLD_WHITE, &format!("{:<width$}", name, width = width));
    let msg_col = colorize(DIM, msg);
    println!("  {}  {}  {}", tick, name_col, msg_col);
}

/// →  tool_name   message
pub fn installing(name: &str, msg: &str) {
    let arrow = colorize(INFO, ARROW);
    let name_col = colorize(BOLD_WHITE, &format!("{:<12}", name));
    let msg_col = colorize(DIM, msg);
    println!("  {}  {}  {}", arrow, name_col, msg_col);
}

/// ✗  tool_name   message
pub fn failed(name: &str, msg: &str) {
    failed_w(name, 12, msg);
}

pub fn failed_w(name: &str, width: usize, msg: &str) {
    let cross = colorize(ERROR, CROSS);
    let name_col = colorize(BOLD_WHITE, &format!("{:<width$}", name, width = width));
    let msg_col = colorize(ERROR, msg);
    println!("  {}  {}  {}", cross, name_col, msg_col);
}

/// ⚠️  tool_name   reason — skipping (platform not supported, no steps, etc.)
pub fn skipped(name: &str, reason: &str) {
    let warn = colorize(WARNING, "⚠");
    let name_col = colorize(BOLD_WHITE, &format!("{:<12}", name));
    let msg_col = colorize(WARNING, &format!("{} — skipping", reason));
    println!("  {}  {}  {}", warn, name_col, msg_col);
}

// --- Headings ---

pub fn h1(title: &str) {
    if use_color() {
        println!("\n{}{}{}", BOLD_WHITE, title, RESET);
        println!("{}{}{}", ORANGE, "─".repeat(title.len()), RESET);
    } else {
        println!("\n{}", title);
        println!("{}", "─".repeat(title.len()));
    }
}

pub fn h2(title: &str) {
    if use_color() {
        println!("\n  {}{}{}", BOLD_WHITE, title, RESET);
    } else {
        println!("\n  {}", title);
    }
}

// --- Info lines ---

pub fn info(msg: &str) {
    println!("  {}", colorize(INFO, msg));
}

pub fn warning(msg: &str) {
    println!("  {}", colorize(WARNING, &format!("warning: {}", msg)));
}

pub fn error(msg: &str) {
    eprintln!("  {}", colorize(ERROR, &format!("error: {}", msg)));
}

pub fn bullet(msg: &str) {
    println!("  {}  {}", colorize(ORANGE, BULLET), colorize(DIM, msg));
}

/// "  use <tool>      declare a tool for this machine"
pub fn command(cmd: &str, description: &str) {
    println!("  {}  {}", colorize(BOLD_WHITE, &format!("{:<30}", cmd)), colorize(DIM, description));
}

// --- Summary line ---

/// "  install: 8 done • 2 failed • 1 skipped" — phase-scoped counter line.
pub fn summary_phase_skip(label: &str, done: usize, failed: usize, skipped: usize) {
    let head = colorize(BOLD_WHITE, &format!("{}:", label));
    let mut parts = vec![colorize(SUCCESS, &format!("{} done", done))];
    if failed > 0 {
        parts.push(colorize(ERROR, &format!("{} failed", failed)));
    }
    if skipped > 0 {
        parts.push(colorize(WARNING, &format!("{} skipped", skipped)));
    }
    println!("  {}  {}", head, parts.join(colorize(ORANGE, " • ").as_str()));
}

// --- Kind tag ---

/// "[brew]" colored by manager — qwert recipe, platform default, or config-only.
pub fn kind_tag(kind: &str) -> String {
    match kind {
        "qwert" => colorize(ORANGE, &format!("[{}]", kind)),
        "brew" => colorize(BRIGHT_BLUE, &format!("[{}]", kind)),
        "apt" | "pacman" => colorize(ORANGE, &format!("[{}]", kind)),
        "default" => colorize(DIM, &format!("[{}]", kind)),
        "config only" => colorize(LIME, &format!("[{}]", kind)),
        _ => colorize(DIM, &format!("[{}]", kind)),
    }
}

/// "[brew]   " — kind tag padded to fixed visible width for column alignment
pub fn kind_tag_col(kind: &str) -> String {
    let tag = kind_tag(kind);
    let visible_len = kind.len() + 2; // "[" + kind + "]"
    let padding = " ".repeat(9usize.saturating_sub(visible_len));
    format!("{}{}", tag, padding)
}

/// "[local]" or "[remote]" tag — recipe origin
pub fn origin_tag(local: bool) -> String {
    if local {
        colorize(LIME, "[local]")
    } else {
        colorize(ORANGE, "[remote]")
    }
}

// --- Search result ---

/// "  neovim              [brew]   Neovim text editor    v0.10.2"
pub fn search_result(name: &str, kind: &str, description: &str, version: Option<&str>) {
    let name_col = colorize(BOLD_WHITE, &format!("{:<20}", name));
    let kind_col = kind_tag_col(kind);
    let ver = version
        .map(|v| colorize(PINK, &format!("  {}", v)))
        .unwrap_or_default();
    if description.is_empty() {
        println!("  {}  {}{}", name_col, kind_col, ver);
    } else {
        println!("  {}  {}  {}{}", name_col, kind_col, colorize(DIM, description), ver);
    }
}

// --- Field line ---

/// "  label          value"
pub fn field(label: &str, value: &str) {
    println!("  {}  {}", colorize(DIM, &format!("{:<14}", label)), value);
}

// --- Blank line ---
pub fn blank() {
    println!();
}
