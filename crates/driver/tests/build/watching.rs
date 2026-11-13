//! `baryon watch` (roadmap 2.2.1): it builds and runs the package, rebuilds after each save, and
//! keeps watching after a build with errors.

use super::{BARYON, build_runtime, package};
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

/// Sends each line `stream` writes down the returned channel, from a thread of its own.
fn lines(stream: impl Read + Send + 'static) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

/// The next line from `receiver` for which `wanted` holds, skipping the others, or an error once
/// a minute passes with nothing said, so a stuck watch fails the test rather than hanging it.
fn next(
    receiver: &Receiver<String>,
    wanted: impl Fn(&str) -> bool,
) -> Result<String, RecvTimeoutError> {
    loop {
        let line = receiver.recv_timeout(Duration::from_mins(1))?;
        if wanted(&line) {
            return Ok(line);
        }
    }
}

/// A save rebuilds and runs the new program; a save with an error prints it and keeps
/// watching, and fixing it builds again; a save that changes no function skips `clang`.
#[test]
fn each_save_rebuilds_and_runs() {
    build_runtime();
    let directory = package("watch", &[("main.byn", "def main():\n    print(1)\n")]).unwrap();
    let main = directory.join("package").join("main.byn");
    let mut child = Command::new(BARYON)
        .args(["watch", "--run"])
        .arg(directory.join("package"))
        .arg(directory.join("program"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let out = lines(child.stdout.take().unwrap());
    let err = lines(child.stderr.take().unwrap());

    assert!(
        next(&err, |line| line.starts_with("built "))
            .unwrap()
            .ends_with(" ms, 1 of 1 modules compiled")
    );
    assert_eq!(
        next(&err, |line| line.starts_with("exited")).unwrap(),
        "exited with 0"
    );
    assert_eq!(next(&out, |_| true).unwrap(), "1");

    // A different length, so the change shows even where modification times are coarse.
    fs::write(&main, "def main():\n    print(22)\n").unwrap();
    assert_eq!(
        next(&err, |line| line.starts_with("exited")).unwrap(),
        "exited with 0"
    );
    assert_eq!(next(&out, |_| true).unwrap(), "22");

    fs::write(&main, "def main():\n    print(undefined)\n").unwrap();
    assert!(
        next(&err, |line| line.starts_with("error"))
            .unwrap()
            .contains("undefined")
    );
    next(&err, |line| line == "watching for changes").unwrap();

    fs::write(&main, "def main():\n    print(333)\n").unwrap();
    assert_eq!(
        next(&err, |line| line.starts_with("exited")).unwrap(),
        "exited with 0"
    );
    assert_eq!(next(&out, |_| true).unwrap(), "333");

    // A comment after the last line moves no function, so the module's machine code is kept.
    fs::write(&main, "def main():\n    print(333)\n# done\n").unwrap();
    assert!(
        next(&err, |line| line.starts_with("built "))
            .unwrap()
            .ends_with(" ms, 0 of 1 modules compiled")
    );
    assert_eq!(next(&out, |_| true).unwrap(), "333");

    child.kill().unwrap();
    child.wait().unwrap();
}
