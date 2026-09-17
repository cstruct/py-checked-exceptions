use std::{path::PathBuf, process::Command, process::Output};

fn run_check(arguments: &[&str]) -> Output {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/config_project");
    Command::new(env!("CARGO_BIN_EXE_py-checked-exceptions"))
        .arg("check")
        .args(arguments)
        .current_dir(project)
        .output()
        .unwrap()
}

#[test]
fn reads_configuration_from_pyproject() {
    let output = run_check(&[]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(!output.status.success());
    assert!(
        stdout.contains("sample.py:25:5: error[raise] Raises undocumented error ConfiguredError")
    );
    assert!(stdout.contains("Found 1 diagnostic"));
    assert!(!stdout.contains("excluded.py"));
    assert!(!stdout.contains("OtherError"));
    assert!(!stdout.contains('\u{1b}'));
}

#[test]
fn command_line_options_override_configuration() {
    let output = run_check(&[
        "--target-exceptions",
        "sample.OtherError",
        "--output-format",
        "full",
    ]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(!output.status.success());
    assert!(stdout.contains("error[raise]: Raises undocumented error OtherError"));
    assert!(!stdout.contains("Raises undocumented error ConfiguredError"));
}

#[test]
fn command_line_excludes_replace_configured_excludes() {
    let output = run_check(&["--exclude", "does-not-match"]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(!output.status.success());
    assert!(stdout.contains("excluded.py:5:5: error[raise]"));
    assert!(stdout.contains("Found 2 diagnostics"));
}
