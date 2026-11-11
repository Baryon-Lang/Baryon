//! Tests of the exported `subprocess.run` function, called as compiled code calls it.

use super::*;
use crate::dict::{DictLayout, Key, KeyPart, STR_PART};
use crate::{OsError, baryon_rt_deallocate};
use std::mem::{MaybeUninit, offset_of};

/// The text of an owned `str`, which it frees.
fn take(text: &OwnedText) -> String {
    let length = usize::try_from(text.length).unwrap();
    // SAFETY: the runtime made `text` with `length` initialized bytes.
    let bytes = unsafe { std::slice::from_raw_parts(text.bytes, length) };
    let copy = String::from_utf8(bytes.to_vec()).unwrap();
    // SAFETY: freed once, with its length.
    unsafe { baryon_rt_deallocate(text.bytes, length, 1) };
    copy
}

/// The strings of an owned `list[str]`, which it frees.
fn take_list(list: &OwnedList) -> Vec<String> {
    let length = usize::try_from(list.length).unwrap();
    let strings = (0..length)
        .map(|index| {
            // SAFETY: the runtime made `list` with `length` elements.
            let element = unsafe { list.elements.add(index) };
            // SAFETY: each element is an owned string.
            take(unsafe { &*element })
        })
        .collect();
    // SAFETY: the buffer is freed once, with the size it was allocated with.
    unsafe { baryon_rt_deallocate(list.elements.cast(), length * size_of::<OwnedText>(), 8) };
    strings
}

/// A `str` value holding `text`, flagged as a literal's when `literal` is set.
fn value(text: &str, literal: bool) -> TextValue {
    let flag = if literal { 1 << 63 } else { 0 };
    TextValue {
        bytes: text.as_ptr(),
        word: u64::try_from(text.len()).unwrap() | flag,
    }
}

/// What a call gave: the result's fields, or the error's kind, message and file name.
type Outcome = Result<(Vec<String>, i64, String, String), (u8, String, String)>;

/// Runs `args` as compiled code does, with the keywords `given` names.
fn run(
    args: &[&str],
    capture: bool,
    given: u8,
    (input, cwd): (&str, &str),
    env: *const Table,
    timeout: f64,
) -> Outcome {
    let mut values: Vec<TextValue> = args
        .iter()
        .enumerate()
        .map(|(index, text)| value(text, index % 2 == 0))
        .collect();
    let list = OwnedList {
        elements: values.as_mut_ptr().cast(),
        length: u64::try_from(values.len()).unwrap(),
        capacity: u64::try_from(values.len()).unwrap(),
    };
    let mut done = MaybeUninit::<CompletedProcess>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the list, the texts and the table are live for the call; the rest are locals.
    let failed = unsafe {
        baryon_rt_subprocess_run(
            &raw const list,
            capture,
            given,
            input.as_ptr(),
            input.len(),
            cwd.as_ptr(),
            cwd.len(),
            env,
            timeout,
            std::ptr::null(),
            done.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    if failed {
        // SAFETY: a failure wrote the error.
        let OsError {
            kind,
            strerror,
            filename,
            ..
        } = unsafe { error.assume_init() }.error;
        return Err((kind, take(&strerror), take(&filename)));
    }
    // SAFETY: success wrote the result.
    let done = unsafe { done.assume_init() };
    Ok((
        take_list(&done.args),
        done.returncode,
        take(&done.stdout),
        take(&done.stderr),
    ))
}

#[test]
fn a_completed_process_is_laid_out_as_the_library_class_declares() {
    // `CompletedProcess(args: list[str], returncode: int, stdout: str, stderr: str)`.
    assert_eq!(offset_of!(CompletedProcess, args), 0);
    assert_eq!(offset_of!(CompletedProcess, returncode), 24);
    assert_eq!(offset_of!(CompletedProcess, stdout), 32);
    assert_eq!(offset_of!(CompletedProcess, stderr), 48);
    assert_eq!(size_of::<CompletedProcess>(), 64);
}

#[test]
fn runs_a_list_and_gives_back_a_copy_of_it() {
    let args = ["sh", "-c", "echo out; echo err >&2; exit 4"];
    let (copy, returncode, stdout, stderr) =
        run(&args, true, 0, ("", ""), std::ptr::null(), 0.0).unwrap();
    assert_eq!(copy, args);
    assert_eq!(returncode, 4);
    assert_eq!((stdout.as_str(), stderr.as_str()), ("out\n", "err\n"));
}

#[test]
fn reads_only_the_keywords_that_were_given() {
    // `input` is not named by `given`, so the program shares the parent's standard input and
    // the text here is not what it reads.
    let outcome = run(
        &["sh", "-c", "exit 0"],
        false,
        GIVEN_TIMEOUT,
        ("ignored", "/no/such/directory"),
        std::ptr::null(),
        30.0,
    );
    assert_eq!(outcome.unwrap().1, 0);
    let (_, _, stdout, _) = run(
        &["cat"],
        true,
        GIVEN_INPUT,
        ("π\n", ""),
        std::ptr::null(),
        0.0,
    )
    .unwrap();
    assert_eq!(stdout, "π\n");
}

#[test]
fn env_reads_a_dict_of_str_to_str_in_insertion_order() {
    static PARTS: [KeyPart; 1] = [KeyPart {
        offset: 0,
        kind: STR_PART,
    }];
    let layout = DictLayout {
        entry_size: 40,
        key_size: 16,
        part_count: 1,
        parts: PARTS.as_ptr(),
    };
    let mut table = Table::new(&layout).unwrap();
    let pairs = [
        ("PATH", "/usr/bin:/bin"),
        ("DROPPED", "x"),
        ("BARYON_A", "1"),
    ];
    for (index, (name, text)) in pairs.iter().enumerate() {
        let key = value(name, index == 0);
        // SAFETY: a `TextValue` is 16 readable bytes, a pointer and a length word.
        let bytes = unsafe { std::slice::from_raw_parts((&raw const key).cast::<u8>(), 16) };
        // SAFETY: the key's one part is a `str` that points to the name's bytes.
        let position = table
            .insert(&layout, unsafe { Key::new(bytes, &PARTS) })
            .unwrap();
        // SAFETY: a table's first word is the address of its entries, as compiled code reads
        // it.
        let entries = unsafe { (&raw const table).cast::<*mut u64>().read() };
        let words = usize::try_from(position).unwrap() * ENVIRONMENT_ENTRY_WORDS + 3;
        // SAFETY: the entry at `position` is inside the entries the table allocated.
        let slot = unsafe { entries.add(words) }.cast::<TextValue>();
        // SAFETY: an entry's value is its last two words, after the hash and the key.
        unsafe { slot.write(value(text, false)) };
    }
    table.remove(&layout, 1);
    let script = "echo $BARYON_A$DROPPED";
    let (_, _, stdout, _) = run(
        &["sh", "-c", script],
        true,
        GIVEN_ENV,
        ("", ""),
        &raw const table,
        0.0,
    )
    .unwrap();
    assert_eq!(stdout, "1\n");
}

#[test]
fn an_empty_dict_is_an_empty_environment() {
    // An absolute path: with no `PATH`, the program is looked up on the system's default.
    let (_, _, stdout, _) = run(
        &["/bin/sh", "-c", "echo ${HOME:-unset}"],
        true,
        GIVEN_ENV,
        ("", ""),
        std::ptr::null(),
        0.0,
    )
    .unwrap();
    assert_eq!(stdout, "unset\n");
}

#[test]
fn a_failure_is_written_as_an_os_error() {
    let missing = run(
        &["baryon-no-such-program"],
        false,
        0,
        ("", ""),
        std::ptr::null(),
        0.0,
    );
    assert_eq!(
        missing.unwrap_err(),
        (
            0,
            "No such file or directory".to_owned(),
            "baryon-no-such-program".to_owned()
        )
    );
    let slow = run(
        &["sleep", "30"],
        false,
        GIVEN_TIMEOUT,
        ("", ""),
        std::ptr::null(),
        0.05,
    );
    assert_eq!(
        slow.unwrap_err(),
        (
            7,
            "timed out after 0.05 seconds".to_owned(),
            "sleep".to_owned()
        )
    );
}
