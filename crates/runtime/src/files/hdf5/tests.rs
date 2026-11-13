//! Tests of reading HDF5 files, against the files h5py 3.16 (HDF5 2.0) wrote in the driver's
//! `tests/fixtures/hdf5`, whose `README.md` holds the script: `earliest.h5` in h5py's default
//! layout, and `latest.h5` with the same objects in the layout `libver="latest"` writes.

use super::values::Inflate;
use super::*;
use crate::files::fits::Pixel;
use crate::files::netcdf::{Item, SubscriptFailure};

/// The path of the fixture `name`.
fn fixture(name: &str) -> String {
    format!(
        "{}/../driver/tests/fixtures/hdf5/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// The values of the dataset `name` of the fixture `file`, read as `f64`s, the chunks inflated
/// by `inflate`.
fn read(file: &str, name: &str, inflate: &mut Inflate<'_>) -> Result<Vec<f64>, OsFailure> {
    pick(file, (name, &[]), inflate).map_err(|failure| match failure {
        SubscriptFailure::Os(failure) => failure,
        SubscriptFailure::Index(message) | SubscriptFailure::Value(message) => {
            panic!("{message}")
        }
    })
}

/// The values `items` pick of the dataset `name` of the fixture `file`, read as `f64`s, the
/// chunks inflated by `inflate`.
fn pick(
    file: &str,
    (name, items): (&str, &[Item]),
    inflate: &mut Inflate<'_>,
) -> Result<Vec<f64>, SubscriptFailure> {
    let path = fixture(file);
    let hdf5 = read_file(&path)?;
    let object = hdf5.find(name).unwrap();
    let dataset = object.dataset.as_ref().unwrap();
    let axes = selection::select(items, &dataset.shape)?;
    let count = usize::try_from(selection::shape(&axes).iter().product::<u64>()).unwrap();
    let mut memory = vec![0_u8; count * 8];
    read_dataset(
        &path,
        &hdf5,
        (name, dataset, items),
        Pixel::F64,
        &mut memory,
        inflate,
    )?;
    Ok(memory
        .chunks_exact(8)
        .map(|bytes| f64::from_ne_bytes(bytes.try_into().unwrap()))
        .collect())
}

/// An inflater for datasets that are not deflated.
fn never(_: &[u8]) -> Result<Vec<u8>, String> {
    Err("not deflated".to_owned())
}

/// `0, 1, ..., count - 1`, each times `scale`, as `f64`s.
fn steps(count: u32, scale: f64) -> Vec<f64> {
    (0..count).map(|value| f64::from(value) * scale).collect()
}

#[test]
fn both_layouts_describe_the_same_objects() {
    let earliest = describe(&read_file(&fixture("earliest.h5")).unwrap());
    let latest = describe(&read_file(&fixture("latest.h5")).unwrap());
    assert_eq!(earliest, latest);
    let attributes: Vec<String> = (0..10)
        .map(|index| format!("A\t/many\tint\t\ta{index}\t{}", index * 3 - 5))
        .collect();
    let mut expected = vec![
        "G\t/",
        "A\t/\tstr\t\ttitle\ttest run",
        "A\t/\tint\t\tversion\t3",
        "A\t/\tfloat\t2\tweights\t0.5\t1.25",
        "D\t/big\t>i2\t3",
        "D\t/compact\t<i2\t4",
        "D\t/count\t<i4\t3",
        "D\t/deflated\t<f8\t6,5",
        "D\t/grow\t<i8\t10",
        "D\t/grow2\t|u1\t3,4",
        "D\t/long\t<i2\t400",
        "D\t/many\t<f4\t",
    ];
    expected.extend(attributes.iter().map(String::as_str));
    expected.extend([
        "D\t/plain\t<f8\t6,5",
        "G\t/run1",
        "D\t/run1/temp\t<f4\t2,3,4",
        "A\t/run1/temp\tstr\t\tunits\tK",
        "D\t/scalar\t<f8\t",
        "D\t/shuffled\t<f8\t6,5",
        "D\t/single\t<f8\t6,5",
    ]);
    assert_eq!(earliest, expected);
}

#[test]
fn every_layout_and_chunk_index_reads_its_values() {
    let grid = steps(30, 0.5);
    let cases: [(&str, Vec<f64>); 10] = [
        (
            "run1/temp",
            steps(24, 1.0).iter().map(|value| value + 270.5).collect(),
        ),
        ("count", vec![7.0, -8.0, 9.0]),
        ("big", vec![1.0, -2.0, 300.0]),
        ("scalar", vec![2.5]),
        ("compact", vec![1.0, 2.0, 3.0, 4.0]),
        ("plain", grid.clone()),
        ("shuffled", grid),
        ("grow2", steps(12, 1.0)),
        ("long", steps(400, 1.0)),
        ("many", vec![1.5]),
    ];
    for file in ["earliest.h5", "latest.h5"] {
        for (name, values) in &cases {
            assert_eq!(
                &read(file, name, &mut never).unwrap(),
                values,
                "{file} {name}"
            );
        }
    }
}

#[test]
fn deflated_chunks_are_handed_to_the_inflater() {
    for file in ["earliest.h5", "latest.h5"] {
        // `deflated` is 6 by 5 in chunks of 4 by 2: six chunks, each a zlib stream.
        let mut streams = 0;
        let mut inflate = |bytes: &[u8]| {
            assert_eq!(bytes[0], 0x78, "{file}");
            streams += 1;
            Err("stopped".to_owned())
        };
        let failure = read(file, "deflated", &mut inflate).unwrap_err();
        assert_eq!(failure.kind, ErrorKind::InvalidData);
        assert_eq!(failure.strerror, "stopped, in the HDF5 dataset deflated");
        assert_eq!(streams, 1, "{file}");
    }
}

#[test]
fn an_array_that_cannot_hold_the_values_is_refused() {
    let path = fixture("latest.h5");
    let hdf5 = read_file(&path).unwrap();
    let dataset = hdf5.find("/count").unwrap().dataset.as_ref().unwrap();
    let mut memory = vec![0_u8; 3 * 2];
    let failure = read_dataset(
        &path,
        &hdf5,
        ("count", dataset, &[]),
        Pixel::I16,
        &mut memory,
        &mut never,
    )
    .unwrap_err();
    let SubscriptFailure::Os(failure) = failure else {
        panic!("not an OSError");
    };
    assert_eq!(
        failure.strerror,
        "the HDF5 dataset count holds <i4, which does not read as `i16`"
    );
}

#[test]
fn a_file_that_is_not_hdf5_is_refused() {
    let path = format!(
        "{}/../driver/tests/fixtures/netcdf/classic.nc",
        env!("CARGO_MANIFEST_DIR")
    );
    let failure = read_file(&path).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::InvalidData);
    assert_eq!(failure.strerror, "the file is not an HDF5 file");
}

#[test]
fn the_checksum_is_lookup3() {
    // `hashlittle("", 0)` and `hashlittle("Four score and seven years ago", 0)`, from the
    // comments of Bob Jenkins's `lookup3.c`.
    assert_eq!(cursor::checksum(b""), 0xdead_beef);
    assert_eq!(
        cursor::checksum(b"Four score and seven years ago"),
        0x1777_0551
    );
}

#[test]
fn a_subscript_reads_only_the_values_it_picks() {
    let slice = |start, stop, stride| Item::Slice {
        start,
        stop,
        stride,
    };
    for file in ["earliest.h5", "latest.h5"] {
        // `run1/temp[1, ::2, 1:3]`: rows 0 and 2 of the second block, columns 1 and 2.
        let items = [
            Item::Index(1),
            slice(None, None, Some(2)),
            slice(Some(1), Some(3), None),
        ];
        let values = pick(file, ("run1/temp", &items), &mut never).unwrap();
        assert_eq!(values, [283.5, 284.5, 291.5, 292.5], "{file}");
        // `long[-3:]`, the last three of 400 values in chunks of 1: an extensible array or a
        // B-tree read only where it holds them.
        let items = [slice(Some(-3), None, None)];
        let values = pick(file, ("long", &items), &mut never).unwrap();
        assert_eq!(values, [397.0, 398.0, 399.0], "{file}");
        // `deflated[5, ::3]` lies in the two chunks that start at row 4 and columns 0 and 2.
        let mut streams = Vec::new();
        let path = fixture(file);
        let hdf5 = read_file(&path).unwrap();
        let dataset = hdf5.find("deflated").unwrap().dataset.as_ref().unwrap();
        let items = [Item::Index(5), slice(None, None, Some(3))];
        for (address, _) in deflated_chunks(&path, &hdf5, ("deflated", dataset, &items)).unwrap() {
            streams.push(address);
        }
        assert_eq!(streams.len(), 2, "{file}");
        // A subscript that picks nothing raises as h5py does.
        let failure = pick(file, ("count", &[Item::Index(3)]), &mut never).unwrap_err();
        assert!(
            matches!(&failure, SubscriptFailure::Index(message)
                if message == "Index (3) out of range for (0-2)"),
            "{file}: {failure:?}"
        );
    }
}

#[test]
fn a_datatype_of_no_bytes_is_refused() {
    // An integer datatype message, version 1, whose size field a damaged file left at 0: its
    // values would have no width to step by.
    let message = [0x10, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(
        messages::datatype(&message).unwrap_err(),
        "the HDF5 file has a datatype of 0 bytes"
    );
    // An integer of 3 bytes is no array type's; it reads as no number.
    let message = [0x10, 0, 0, 0, 3, 0, 0, 0];
    assert_eq!(
        messages::datatype(&message).unwrap().name(),
        Datatype::Other { class: 0, size: 3 }.name()
    );
}
