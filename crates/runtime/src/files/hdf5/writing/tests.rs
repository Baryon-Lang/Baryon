//! Tests of writing HDF5 files: each scenario in the driver's `tests/fixtures/hdf5/writes.txt`
//! is a log of changes, applied here change by change, and the file must have the size and
//! digest of the one h5py 3.16 wrote with the same calls (`README.md` there holds the script).

use super::*;

/// The scenarios: each one's name, size, digest and log.
fn scenarios() -> Vec<(String, u64, u64, Vec<String>)> {
    let text = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../driver/tests/fixtures/hdf5/writes.txt"
    ))
    .unwrap();
    let mut scenarios = Vec::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        if let Some(head) = line.strip_prefix("== ") {
            let fields: Vec<&str> = head.split(' ').collect();
            scenarios.push((
                fields[0].to_owned(),
                fields[1].parse().unwrap(),
                u64::from_str_radix(fields[2], 16).unwrap(),
                Vec::new(),
            ));
        } else if !line.is_empty() {
            scenarios.last_mut().unwrap().3.push(line.to_owned());
        }
    }
    scenarios
}

/// The 64-bit FNV-1a digest of `bytes`.
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The values the scenarios give a dataset `line` makes: 0, 1, 2 and so on in its element
/// type, little-endian.
fn values(line: &str) -> Vec<u8> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields[0] != "D" {
        return Vec::new();
    }
    let count: u64 = lengths(fields[3]).unwrap().iter().product();
    let element = Element::parse(fields[2]).unwrap();
    let mut bytes = Vec::new();
    for value in 0..count {
        match element {
            #[expect(clippy::cast_precision_loss, reason = "small test values")]
            Element::Float(4) => bytes.extend_from_slice(&(value as f32).to_le_bytes()),
            #[expect(clippy::cast_precision_loss, reason = "small test values")]
            Element::Float(_) => bytes.extend_from_slice(&(value as f64).to_le_bytes()),
            Element::Signed(size) | Element::Unsigned(size) => {
                bytes.extend_from_slice(&value.to_le_bytes()[..usize::from(size)]);
            }
        }
    }
    bytes
}

/// A path for one scenario's file, named after it and this process.
fn file(name: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "baryon-hdf5-write-{}-{name}.h5",
        std::process::id()
    ));
    path.to_str().unwrap().to_owned()
}

/// The file `log` makes, built in memory from one replay: the structures, and each dataset's
/// values at the address its replay gives.
fn replayed(log: &[&str]) -> Vec<u8> {
    let mut writer = Writer::new();
    let mut made = Vec::new();
    for line in log {
        if let Some((index, _)) = apply(&mut writer, line).unwrap() {
            made.push((index, values(line)));
        }
    }
    let blocks: Vec<(u64, Vec<u8>)> = made
        .iter()
        .flat_map(|(index, values)| place(&writer.objects[*index].kind, values))
        .map(|(address, bytes)| (address, bytes.into_owned()))
        .collect();
    let (end, mut structures) = writer.encode();
    structures.extend(blocks);
    let mut bytes = vec![0; to_usize(end)];
    for (address, block) in structures {
        bytes[to_usize(address)..to_usize(address) + block.len()].copy_from_slice(&block);
    }
    bytes
}

#[test]
fn every_scenario_matches_h5py_byte_for_byte() {
    let mut failures = Vec::new();
    for (name, size, expected, log) in scenarios() {
        let lines: Vec<&str> = log.iter().map(String::as_str).collect();
        // Long logs are replayed once; short ones are written change by change, as a program
        // writes them, which also shows that values written earlier are kept.
        let bytes = if lines.len() > 100 {
            replayed(&lines)
        } else {
            let path = file(&name);
            for count in 0..=lines.len() {
                let values = count
                    .checked_sub(1)
                    .map_or_else(Vec::new, |last| values(lines[last]));
                write(&path, &lines[..count], &values).unwrap();
            }
            let bytes = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            bytes
        };
        if (bytes.len() as u64, digest(&bytes)) != (size, expected) {
            failures.push(format!(
                "{name}: {} bytes, digest {:016x}",
                bytes.len(),
                digest(&bytes)
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The message of the failure the change that is the last of `log` meets, with `last` as
/// its values.
fn refusal(name: &str, log: &[&str], last: &[u8]) -> String {
    let path = file(name);
    for count in 0..log.len() {
        let values = count
            .checked_sub(1)
            .map_or_else(Vec::new, |last| values(log[last]));
        write(&path, &log[..count], &values).unwrap();
    }
    let failure = write(&path, log, last).unwrap_err();
    fs::remove_file(&path).unwrap();
    assert_eq!(failure.kind, ErrorKind::InvalidData);
    failure.strerror
}

#[test]
fn a_name_already_taken_is_refused_as_h5py_refuses_it() {
    assert_eq!(
        refusal("taken", &["G\tg", "G\tg"], &[]),
        "Unable to synchronously create object (name already exists)"
    );
}

#[test]
fn a_member_of_a_dataset_is_refused() {
    assert_eq!(
        refusal("under", &["D\tx\t<f8\t1", "G\tx/b"], &[]),
        "Unable to synchronously create object ('b' is not in a group)"
    );
}

#[test]
fn values_that_do_not_fill_the_dataset_are_refused() {
    assert_eq!(
        refusal("short", &["D\tx\t<f8\t2"], &[0; 8]),
        "the dataset holds 16 bytes of values, not 8"
    );
}

#[test]
fn a_line_the_log_does_not_hold_is_refused() {
    assert_eq!(
        refusal("unknown", &["X\tx"], &[]),
        "not a line of an HDF5 file's log: \"X\\tx\""
    );
    assert_eq!(refusal("type", &["D\tx\t>f8\t2"], &[]), "an element type");
    assert_eq!(
        refusal("length", &["D\tx\t<f8\ttwo"], &[]),
        "not a length: \"two\""
    );
}

#[test]
fn chunks_h5py_refuses_are_refused() {
    let line = |chunk: &str| format!("D\tx\t<f8\t10\t{chunk}\t");
    assert_eq!(
        refusal("wide", &[&line("20")], &[]),
        "Chunk shape must not be greater than data shape in any dimension. \
         (20,) is not compatible with (10,)"
    );
    assert_eq!(
        refusal("rank", &[&line("2,2")], &[]),
        "'chunks' must have same rank as dataset shape"
    );
    assert_eq!(
        refusal("zero", &[&line("0")], &[]),
        "All chunk dimensions must be positive (all chunk dimensions must be positive)"
    );
    assert_eq!(
        refusal("scalar", &["D\tx\t<f8\t\t\tshuffle"], &[]),
        "Scalar datasets don't support chunk/filter options"
    );
    assert_eq!(
        refusal("filter", &["D\tx\t<f8\t10\t\tszip"], &[]),
        "not a filter: \"szip\""
    );
}

#[test]
fn the_written_file_reads_back() {
    let path = file("reads");
    let log = ["G\truns", "D\truns/t\t<i4\t2,2", "A\truns/t\tunits\tstr\tK"];
    for count in 0..=log.len() {
        let values = if count == 2 {
            values(log[1])
        } else {
            Vec::new()
        };
        write(&path, &log[..count], &values).unwrap();
    }
    let file = super::super::read_file(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(
        super::super::describe(&file),
        [
            "G\t/",
            "G\t/runs",
            "D\t/runs/t\t<i4\t2,2",
            "A\t/runs/t\tstr\t\tunits\tK"
        ]
    );
}
