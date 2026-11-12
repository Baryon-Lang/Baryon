//! Tests of `.npy` headers: the bytes written are `NumPy`'s, recorded from `numpy.lib.format`
//! (2.5.3), and the headers read are `NumPy`'s, written to real files under the system's
//! temporary directory.

use super::*;
use std::path::PathBuf;

/// `NumPy`'s version 1.0 header of `shape` and `descr`: the magic bytes, the version, the length
/// 118 and the dictionary, padded with spaces to 118 bytes with its newline.
fn numpy(dictionary: &str) -> Vec<u8> {
    let mut bytes = b"\x93NUMPY\x01\x00v\x00".to_vec();
    bytes.extend_from_slice(dictionary.as_bytes());
    bytes.resize(127, b' ');
    bytes.push(b'\n');
    bytes
}

/// A path for one test's file, named after it and this process.
fn file(test: &str) -> PathBuf {
    std::env::temp_dir().join(format!("baryon-npy-{}-{test}.npy", std::process::id()))
}

#[test]
fn headers_are_numpys_bytes() {
    let cases: [(&str, &[usize], &str); 5] = [
        (
            "<f8",
            &[3],
            "{'descr': '<f8', 'fortran_order': False, 'shape': (3,), }",
        ),
        (
            "<i4",
            &[2, 3],
            "{'descr': '<i4', 'fortran_order': False, 'shape': (2, 3), }",
        ),
        (
            "|b1",
            &[],
            "{'descr': '|b1', 'fortran_order': False, 'shape': (), }",
        ),
        (
            "|u1",
            &[0],
            "{'descr': '|u1', 'fortran_order': False, 'shape': (0,), }",
        ),
        (
            "<c16",
            &[123_456_789_012, 4],
            "{'descr': '<c16', 'fortran_order': False, 'shape': (123456789012, 4), }",
        ),
    ];
    for (descr, shape, dictionary) in cases {
        assert_eq!(header(descr, shape), numpy(dictionary), "{descr} {shape:?}");
    }
}

#[test]
fn a_header_too_long_for_version_1_is_version_2() {
    let shape = vec![1_usize; 30_000];
    let bytes = header("<f8", &shape);
    assert_eq!(&bytes[..8], b"\x93NUMPY\x02\x00");
    assert_eq!(bytes.len() % ALIGN, 0);
    let length = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    assert_eq!(bytes.len(), 12 + length);
    assert_eq!(bytes.last(), Some(&b'\n'));
}

#[test]
fn written_headers_read_back() {
    let path = file("round-trip");
    let mut bytes = header("<i4", &[2, 3]);
    let offset = bytes.len() as u64;
    bytes.extend_from_slice(&[0; 24]);
    fs::write(&path, &bytes).unwrap();
    let read = read_header(path.to_str().unwrap()).unwrap();
    assert_eq!(
        read,
        Header {
            descr: "<i4".to_owned(),
            fortran_order: false,
            shape: vec![2, 3],
            offset,
        }
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn version_2_and_any_key_order_are_read() {
    let path = file("version-2");
    let mut bytes = b"\x93NUMPY\x02\x00t\x00\x00\x00".to_vec();
    bytes.extend_from_slice(b"{'shape': (3, 1), 'fortran_order': True, 'descr': '>f4', }");
    bytes.resize(127, b' ');
    bytes.push(b'\n');
    fs::write(&path, &bytes).unwrap();
    let read = read_header(path.to_str().unwrap()).unwrap();
    assert_eq!(
        read,
        Header {
            descr: ">f4".to_owned(),
            fortran_order: true,
            shape: vec![3, 1],
            offset: 128,
        }
    );
    assert_eq!(
        check(&read, ">f4", 2),
        Err(
            "the .npy file holds its elements in Fortran order; save it from a C-ordered array"
                .to_owned()
        )
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn what_is_not_a_header_is_invalid_data() {
    let cases: [(&str, &[u8], &str); 4] = [
        ("text", b"1.5 2.5\n", "the file is not a .npy file"),
        (
            "short",
            b"\x93NU",
            "the file is not a .npy file: it ends before its header",
        ),
        (
            "version",
            b"\x93NUMPY\x04\x00\x00\x00",
            "the .npy file has format version 4.0, which is not 1.0, 2.0 or 3.0",
        ),
        (
            "records",
            b"\x93NUMPY\x01\x00\x30\x00{'descr': [('x', '<f8')], 'shape': (2,), }       \n",
            "the .npy file holds records or another structured type, [('x', '<f8')]",
        ),
    ];
    for (name, bytes, message) in cases {
        let path = file(name);
        fs::write(&path, bytes).unwrap();
        let failure = read_header(path.to_str().unwrap()).unwrap_err();
        assert_eq!(failure.kind, ErrorKind::InvalidData, "{name}");
        assert_eq!(failure.strerror, message, "{name}");
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn headers_are_checked_against_the_program() {
    let header = |descr: &str, shape: Vec<usize>| Header {
        descr: descr.to_owned(),
        fortran_order: false,
        shape,
        offset: 128,
    };
    assert_eq!(check(&header("<f8", vec![4]), "<f8", 1), Ok(()));
    assert_eq!(check(&header("|u1", vec![2, 2]), "<u1", 2), Ok(()));
    assert_eq!(
        check(&header("<i8", vec![4]), "<f8", 1),
        Err("the .npy file holds elements of type '<i8', not '<f8' as the program asks".to_owned())
    );
    assert_eq!(
        check(&header("<f8", vec![2, 2]), "<f8", 1),
        Err("the .npy file holds an array of rank 2, not rank 1 as the program asks".to_owned())
    );
}
