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
/// values, `given` line by line, at the address its replay gives.
fn replayed(log: &[&str], given: &[Vec<u8>]) -> Vec<u8> {
    let mut writer = Writer::new();
    let mut made = Vec::new();
    for (line, values) in log.iter().zip(given) {
        if let Some((index, _)) = apply(&mut writer, line).unwrap() {
            made.push((index, values));
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
        // A `V` line holds the values of the line before it, a gzip dataset's deflated chunks
        // as h5py deflated them; other datasets' values are made by `values`.
        let mut lines: Vec<&str> = Vec::new();
        let mut given: Vec<Vec<u8>> = Vec::new();
        for line in &log {
            if let Some(hex) = line.strip_prefix("V\t") {
                *given.last_mut().unwrap() = from_hex(hex).unwrap();
            } else {
                lines.push(line);
                given.push(values(line));
            }
        }
        // Long logs are replayed once; short ones are written change by change, as a program
        // writes them, which also shows that values written earlier are kept.
        let bytes = if lines.len() > 100 {
            replayed(&lines, &given)
        } else {
            let path = file(&name);
            write(&path, &[], &[]).unwrap();
            for count in 1..=lines.len() {
                write(&path, &lines[..count], &given[count - 1]).unwrap();
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

/// `bytes` as a zlib stream of one stored block, not compressed: valid deflated data, but
/// not the bytes h5py's zlib makes.
fn stored_zlib(bytes: &[u8]) -> Vec<u8> {
    let length = u16::try_from(bytes.len()).unwrap();
    let mut stream = vec![0x78, 0x01, 0x01];
    stream.extend_from_slice(&length.to_le_bytes());
    stream.extend_from_slice(&(!length).to_le_bytes());
    stream.extend_from_slice(bytes);
    let (mut low, mut high) = (1_u32, 0_u32);
    for &byte in bytes {
        low = (low + u32::from(byte)) % 65521;
        high = (high + low) % 65521;
    }
    stream.extend_from_slice(&((high << 16) | low).to_be_bytes());
    stream
}

#[test]
fn chunks_deflated_by_another_zlib_read_back() {
    // Ten doubles in chunks of four, the last chunk padded with zeros, each chunk deflated as
    // one stored block: what card 35 promises for gzip, a file readable whatever zlib made
    // its chunks.
    let path = file("deflated");
    let ten: Vec<Vec<u8>> = values("D\tz\t<f8\t10")
        .chunks(32)
        .map(|chunk| {
            let mut chunk = chunk.to_vec();
            chunk.resize(32, 0);
            stored_zlib(&chunk)
        })
        .collect();
    let sizes: Vec<String> = ten.iter().map(|chunk| chunk.len().to_string()).collect();
    let line = format!("D\tz\t<f8\t10\t4\tgzip=4,fletcher32\t{}", sizes.join(","));
    write(&path, &[], &[]).unwrap();
    write(&path, &[&line], &ten.concat()).unwrap();
    let file = super::super::read_file(&path).unwrap();
    let dataset = file.find("/z").unwrap().dataset.as_ref().unwrap();
    let mut memory = vec![0; 80];
    let mut inflate = |stream: &[u8]| -> Result<Vec<u8>, String> {
        let length = usize::from(u16::from_le_bytes([stream[3], stream[4]]));
        Ok(stream[7..7 + length].to_vec())
    };
    super::super::values::read_dataset(
        &path,
        &file,
        ("z", dataset, &[]),
        crate::files::fits::Pixel::F64,
        &mut memory,
        &mut inflate,
    )
    .unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(memory, values("D\tz\t<f8\t10"));
}
