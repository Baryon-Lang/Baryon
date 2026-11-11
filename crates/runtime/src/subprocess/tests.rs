//! Tests of running programs. Each runs real programs, and only ones Linux and macOS both have:
//! `sh`, `cat`, `sleep` and `env`.

use super::*;

/// A request to run `args` with nothing else given.
fn request<'a>(args: &[&'a str]) -> Request<'a> {
    Request {
        args: args.to_vec(),
        ..Request::default()
    }
}

/// A request to run `script` in `sh`, with its output captured.
fn shell(script: &str) -> Request<'_> {
    Request {
        capture: true,
        ..request(&["sh", "-c", script])
    }
}

#[test]
fn gives_the_exit_status_and_no_output_without_capture() {
    let done = run(&request(&["sh", "-c", "exit 3"])).unwrap();
    assert_eq!(done.returncode, 3);
    assert_eq!((done.stdout.as_str(), done.stderr.as_str()), ("", ""));
    assert_eq!(
        run(&request(&["sh", "-c", "exit 0"])).unwrap().returncode,
        0
    );
}

#[test]
fn captures_both_streams_apart() {
    let done = run(&shell("echo out; echo err >&2; exit 1")).unwrap();
    assert_eq!(done.returncode, 1);
    assert_eq!(done.stdout, "out\n");
    assert_eq!(done.stderr, "err\n");
}

#[test]
fn passes_arguments_as_they_are_with_no_shell() {
    // One argument with a space, a quote and a `$`: a shell would split and expand it.
    let done = run(&Request {
        capture: true,
        ..request(&[
            "sh",
            "-c",
            "printf '%s|' \"$@\"",
            "sh",
            "two words",
            "$HOME 'x'",
        ])
    })
    .unwrap();
    assert_eq!(done.stdout, "two words|$HOME 'x'|");
}

#[test]
fn captured_output_reads_line_ends_as_a_text_file_does() {
    let done = run(&shell("printf 'a\\r\\nb\\rc\\n'")).unwrap();
    assert_eq!(done.stdout, "a\nb\nc\n");
}

#[test]
fn output_larger_than_a_pipe_on_both_streams_does_not_stall() {
    let script = "i=0; while [ $i -lt 4000 ]; do \
                  echo 0123456789012345678901234567890123456789; \
                  echo 0123456789012345678901234567890123456789 >&2; i=$((i+1)); done";
    let done = run(&shell(script)).unwrap();
    assert_eq!(done.stdout.len(), 4000 * 41);
    assert_eq!(done.stderr.len(), 4000 * 41);
}

#[test]
fn input_is_the_program_s_standard_input() {
    let done = run(&Request {
        input: Some("α 1\nβ 2\n"),
        capture: true,
        ..request(&["cat"])
    })
    .unwrap();
    assert_eq!(done.stdout, "α 1\nβ 2\n");
    let empty = run(&Request {
        input: Some(""),
        capture: true,
        ..request(&["cat"])
    })
    .unwrap();
    assert_eq!(empty.stdout, "");
}

#[test]
fn a_program_that_ignores_a_large_input_still_finishes() {
    let input = "x".repeat(1 << 20);
    let done = run(&Request {
        input: Some(&input),
        ..request(&["sh", "-c", "exit 0"])
    })
    .unwrap();
    assert_eq!(done.returncode, 0);
}

#[test]
fn cwd_is_where_the_program_runs() {
    let directory = std::env::temp_dir().canonicalize().unwrap();
    let done = run(&Request {
        cwd: directory.to_str(),
        ..shell("pwd -P")
    })
    .unwrap();
    assert_eq!(done.stdout.trim_end(), directory.to_str().unwrap());
}

#[test]
fn a_cwd_that_is_missing_or_a_file_is_reported_by_its_own_name() {
    let missing = run(&Request {
        cwd: Some("/no/such/directory"),
        ..request(&["sh", "-c", "exit 0"])
    })
    .unwrap_err();
    assert_eq!(missing.kind, ErrorKind::FileNotFound);
    assert_eq!(missing.filename, "/no/such/directory");
    let file = run(&Request {
        cwd: Some("/bin/sh"),
        ..request(&["sh", "-c", "exit 0"])
    })
    .unwrap_err();
    assert_eq!(file.kind, ErrorKind::NotADirectory);
    assert_eq!((file.errno, file.filename.as_str()), (20, "/bin/sh"));
}

#[test]
fn env_is_the_program_s_whole_environment() {
    let done = run(&Request {
        env: Some(vec![
            ("PATH", "/usr/bin:/bin"),
            ("BARYON_GREETING", "hello"),
        ]),
        ..shell("echo $BARYON_GREETING; env | wc -l")
    })
    .unwrap();
    let mut lines = done.stdout.lines();
    assert_eq!(lines.next(), Some("hello"));
    // Only what was given, and the few variables `sh` sets for itself, such as `PWD`.
    let count: usize = lines.next().unwrap().trim().parse().unwrap();
    assert!((2..=6).contains(&count), "{count} variables");
}

#[test]
fn a_program_that_is_not_there_is_file_not_found() {
    let failure = run(&request(&["baryon-no-such-program", "x"])).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::FileNotFound);
    assert_eq!(failure.errno, 2);
    assert_eq!(failure.strerror, "No such file or directory");
    assert_eq!(failure.filename, "baryon-no-such-program");
}

#[test]
fn a_file_that_cannot_be_run_is_a_permission_failure() {
    let failure = run(&request(&["/etc/passwd"])).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::Permission);
    assert_eq!(failure.filename, "/etc/passwd");
}

#[test]
fn a_timeout_ends_the_program_and_fails() {
    let started = Instant::now();
    let failure = run(&Request {
        timeout: Some(0.05),
        ..request(&["sleep", "30"])
    })
    .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(failure.kind, ErrorKind::Timeout);
    assert_eq!(failure.errno, 0);
    assert_eq!(failure.strerror, "timed out after 0.05 seconds");
    assert_eq!(failure.filename, "sleep");
}

#[test]
fn a_program_that_ends_in_time_is_not_timed_out() {
    let done = run(&Request {
        timeout: Some(30.0),
        ..shell("echo quick")
    })
    .unwrap();
    assert_eq!((done.returncode, done.stdout.as_str()), (0, "quick\n"));
    // A limit too large to count is no limit.
    let unlimited = run(&Request {
        timeout: Some(f64::INFINITY),
        ..shell("echo quick")
    })
    .unwrap();
    assert_eq!(unlimited.returncode, 0);
}

#[test]
fn a_negative_timeout_has_already_passed() {
    let failure = run(&Request {
        timeout: Some(-1.0),
        ..request(&["sleep", "30"])
    })
    .unwrap_err();
    assert_eq!(failure.kind, ErrorKind::Timeout);
}

#[test]
fn a_signal_gives_a_negative_return_code() {
    let done = run(&request(&["sh", "-c", "kill -9 $$"])).unwrap();
    assert_eq!(done.returncode, -9);
}

#[test]
fn output_that_is_not_utf8_is_invalid_data() {
    let failure = run(&shell("printf 'ok\\377'")).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::InvalidData);
    assert_eq!(failure.strerror, "standard output: invalid UTF-8 at byte 2");
    assert_eq!(failure.filename, "sh");
}
