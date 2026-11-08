//! The `baryon` command line itself: the flags that need no package.

use std::process::Command;

/// The `baryon` executable cargo built for these tests.
const BARYON: &str = env!("CARGO_BIN_EXE_baryon");

/// `--version` and `-V` print the release to standard output, so a bug report can say which
/// one it is about; `--help` prints the usage. Both succeed. Before, both were "wrong
/// arguments", printed the usage to standard error and exited with 2.
#[test]
fn version_and_help_print_and_succeed() {
    for flag in ["--version", "-V"] {
        let output = Command::new(BARYON).arg(flag).output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("baryon {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
    let help = Command::new(BARYON).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).starts_with("usage: baryon check"));
    let wrong = Command::new(BARYON).arg("--bogus").output().unwrap();
    assert_eq!(wrong.status.code(), Some(2));
}
