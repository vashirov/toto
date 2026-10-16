//! Integration tests that run the compiled binary.
//!
//! Only non-interactive paths are tested: `--best-match`, `--print`,
//! `info *` and `init <shell>`. Anything that opens a picker TTY would
//! hang the test runner and is intentionally not covered here.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn toto(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_toto"));
    // Keep tests deterministic: never read the user's real config.
    cmd.env(
        "TOTO_CONFIG",
        std::env::temp_dir().join(format!("toto-no-config-{}", std::process::id())),
    );
    cmd.args(args);
    cmd
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

// -- Best match mode --

#[test]
fn best_match_prints_selected_command() {
    let out = toto(&[
        "--path",
        fixtures().to_str().unwrap(),
        "--best-match",
        "--query",
        "connect",
        "--print",
    ])
    .output()
    .unwrap();
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert_eq!(stdout_of(&out).trim(), "arp -a");
}

#[test]
fn best_match_no_match_fails_with_message() {
    let out = toto(&[
        "--path",
        fixtures().to_str().unwrap(),
        "--best-match",
        "--query",
        "zzzznotfound",
        "--print",
    ])
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr_of(&out).contains("No match found"));
}

#[test]
fn env_toto_path_resolves() {
    let out = Command::new(env!("CARGO_BIN_EXE_toto"))
        .env("TOTO_PATH", fixtures().to_str().unwrap())
        .args(["--best-match", "--query", "connect", "--print"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert_eq!(stdout_of(&out).trim(), "arp -a");
}

// -- Tag rules --

#[test]
fn tag_rules_allow_limits_selection() {
    let out = toto(&[
        "--path",
        fixtures().to_str().unwrap(),
        "--best-match",
        "--query",
        "running",
        "--tag-rules",
        "podman",
        "--print",
    ])
    .output()
    .unwrap();
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert_eq!(stdout_of(&out).trim(), "podman ps");
}

#[test]
fn tag_rules_deny_leaves_no_match() {
    // Denying every tag in the fixtures must leave no candidate at all.
    let out = toto(&[
        "--path",
        fixtures().to_str().unwrap(),
        "--best-match",
        "--query",
        "connect",
        "--tag-rules",
        "!ssh,!git,!podman",
        "--print",
    ])
    .output()
    .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr_of(&out).contains("No match found"));
}

// -- Pipe mode --

#[test]
fn pipe_mode_empty_input_fails() {
    // stdin not a tty and no tricks flags: pipe mode reads from stdin.
    let out = Command::new(env!("CARGO_BIN_EXE_toto"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr_of(&out).contains("no input"));
}

// -- Info commands --

#[test]
fn info_sample_config_is_valid_toml() {
    let out = toto(&["info", "sample-config"]).output().unwrap();
    assert!(out.status.success());
    let text = stdout_of(&out);
    assert!(text.contains("[picker]"));
    assert!(text.contains("[style]"));
    let value: toml::Value = text.parse().expect("sample config must parse as TOML");
    // All keys are commented out: empty tables only.
    assert!(value.get("picker").unwrap().as_table().unwrap().is_empty());
}

#[test]
fn info_config_path_under_home() {
    let home = std::env::temp_dir().join(format!("toto-int-home-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_toto"))
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME")
        .env("HOME", &home)
        .args(["info", "config-path"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let path = stdout_of(&out);
    assert!(path.contains("toto"), "output: {path}");
    assert!(path.trim_end().ends_with("config.toml"), "output: {path}");
}

// -- Shell init scripts --

#[test]
fn init_bash_script() {
    let out = toto(&["init", "bash"]).output().unwrap();
    let text = stdout_of(&out);
    assert!(text.contains("_toto_tricks()"));
    assert!(text.contains("bind -x"));
    assert!(text.contains("\\C-t"));
}

#[test]
fn init_zsh_script() {
    let out = toto(&["init", "zsh"]).output().unwrap();
    let text = stdout_of(&out);
    assert!(text.contains("zle -N _toto_tricks"));
    assert!(text.contains("bindkey '\\C-t' _toto_tricks"));
}

#[test]
fn init_fish_script() {
    let out = toto(&["init", "fish"]).output().unwrap();
    let text = stdout_of(&out);
    assert!(text.contains("function _toto_tricks"));
    assert!(text.contains("bind \\ct _toto_tricks"));
    // The readline-style key must be converted, not passed through.
    assert!(!text.contains("\\C-t"));
}

#[test]
fn init_fish_converts_configured_key() {
    let dir = std::env::temp_dir().join(format!("toto-int-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    std::fs::write(
        &config,
        "[keys]\ntricks = \"\\\\C-f\"\n", // TOML value: \C-f
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_toto"))
        .env("TOTO_CONFIG", &config)
        .args(["init", "fish"])
        .output()
        .unwrap();
    let text = stdout_of(&out);
    assert!(text.contains("bind \\cf _toto_tricks"), "output: {text}");
    assert!(!text.contains("\\C-f"));
}
