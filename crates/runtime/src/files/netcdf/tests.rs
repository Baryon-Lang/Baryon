//! Tests of NetCDF classic files: the files read are the ones `netCDF4` 1.7.2 wrote for the
//! driver's build tests (`crates/driver/tests/fixtures/netcdf`), and the corrupt ones are those
//! files cut short or changed by these tests, under the system's temporary directory.

use super::*;

/// The classic (CDF-1) file `netCDF4` wrote.
const CLASSIC: &[u8] = include_bytes!("../../../../driver/tests/fixtures/netcdf/classic.nc");

/// The same file in the 64-bit offset format (CDF-2).
const OFFSET: &[u8] = include_bytes!("../../../../driver/tests/fixtures/netcdf/offset.nc");

/// Writes `bytes` to a file named after one test and this process, and gives its path.
fn file(test: &str, bytes: &[u8]) -> String {
    let path = std::env::temp_dir().join(format!("baryon-netcdf-{}-{test}.nc", std::process::id()));
    fs::write(&path, bytes).unwrap();
    path.to_str().unwrap().to_owned()
}

/// Reads `name` of the file at `path` as `count` elements of `pixel`.
fn read(path: &str, name: &str, pixel: Pixel, count: usize) -> Result<Vec<u8>, OsFailure> {
    let header = read_header(path)?;
    let variable = header
        .variables
        .iter()
        .find(|variable| variable.name == name)
        .unwrap();
    let mut memory = vec![0_u8; count * pixel.size()];
    read_variable(path, &header, variable, pixel, &mut memory)?;
    Ok(memory)
}

/// `memory` as `f32`s.
fn floats(memory: &[u8]) -> Vec<f32> {
    memory
        .chunks_exact(4)
        .map(|bytes| f32::from_ne_bytes(bytes.try_into().unwrap()))
        .collect()
}

#[test]
fn both_classic_formats_describe_the_same_header() {
    let expected = [
        "D\ttime\t2\t1",
        "D\tlat\t2\t0",
        "D\tlon\t3\t0",
        "A\t\tstr\t1\ttitle\ttest grid",
        "A\t\tint\t1\tversion\t3",
        "A\t\tfloat\t2\tweights\t0.5,1.25",
        "V\ttemp\tf4\ttime,lat,lon",
        "A\ttemp\tstr\t1\tunits\tK",
        "V\tpacked\ti2\ttime,lat,lon",
        "A\tpacked\tfloat\t1\tscale_factor\t0.10000000149011612",
        "A\tpacked\tfloat\t1\tadd_offset\t250.0",
        "V\tcount\ti4\ttime",
        "V\tlat\tf8\tlat",
        "A\tlat\tstr\t1\tunits\tdegrees_north",
        "V\tflags\ti1\tlon",
    ];
    for (test, bytes) in [("describe-classic", CLASSIC), ("describe-offset", OFFSET)] {
        let header = read_header(&file(test, bytes)).unwrap();
        assert_eq!(describe(&header), expected);
    }
}

#[test]
fn record_variables_are_read_record_by_record() {
    for (test, bytes) in [("records-classic", CLASSIC), ("records-offset", OFFSET)] {
        let path = file(test, bytes);
        let temp = floats(&read(&path, "temp", Pixel::F32, 12).unwrap());
        let expected: Vec<f32> = (0..12_u8).map(|value| f32::from(value) + 270.5).collect();
        assert_eq!(temp, expected);
        let count = read(&path, "count", Pixel::I64, 2).unwrap();
        assert_eq!(count[..8], 7_i64.to_ne_bytes());
        assert_eq!(count[8..], (-8_i64).to_ne_bytes());
    }
}

#[test]
fn fixed_variables_are_read_whole() {
    let path = file("fixed", CLASSIC);
    let lat = read(&path, "lat", Pixel::F64, 2).unwrap();
    assert_eq!(lat[..8], 10.0_f64.to_ne_bytes());
    assert_eq!(lat[8..], 20.0_f64.to_ne_bytes());
    let flags = read(&path, "flags", Pixel::I8, 3).unwrap();
    assert_eq!(flags, [0xFF, 0, 1]);
}

#[test]
fn scaled_values_are_netcdf4s_float32_values() {
    // The bits of the values `netCDF4` 1.7.2 gives for `packed[:]`, a `float32` array, as
    // `.ravel().view("u4")` gives them.
    let bits: [u32; 12] = [
        0x4379_B333,
        0x437A_6666,
        0x437B_199A,
        0x437B_CCCD,
        0x437C_8000,
        0x437D_3333,
        0x437D_E666,
        0x437E_999A,
        0x437F_4CCD,
        0x4380_0000,
        0x4380_599A,
        0x4380_B333,
    ];
    let expected = bits.map(f32::from_bits);
    let path = file("scaled", CLASSIC);
    assert_eq!(
        floats(&read(&path, "packed", Pixel::F32, 12).unwrap()),
        expected
    );
    let wide = read(&path, "packed", Pixel::F64, 12).unwrap();
    let wide: Vec<f64> = wide
        .chunks_exact(8)
        .map(|bytes| f64::from_ne_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(wide, expected.map(f64::from));
}

#[test]
fn a_type_that_cannot_hold_the_values_is_refused() {
    let path = file("refused", CLASSIC);
    let narrow = read(&path, "count", Pixel::I16, 2).unwrap_err();
    assert_eq!(narrow.kind, ErrorKind::InvalidData);
    assert_eq!(
        narrow.strerror,
        "the NetCDF variable count holds i4, which does not read as `i16`; read it as a float \
         array"
    );
    let scaled = read(&path, "packed", Pixel::I32, 12).unwrap_err();
    assert_eq!(
        scaled.strerror,
        "the NetCDF variable packed holds i2 scaled by scale_factor or add_offset, which does not \
         read as `i32`; read it as a float array"
    );
}

#[test]
fn files_that_are_not_classic_netcdf_are_refused() {
    let mut streaming = CLASSIC.to_vec();
    streaming[4..8].copy_from_slice(&[0xFF; 4]);
    let cases: [(&str, &[u8], &str); 5] = [
        (
            "text",
            &b"hello"[..],
            "the file is not a NetCDF file: it does not start with CDF",
        ),
        (
            "cdf5",
            &b"CDF\x05\0\0\0\0"[..],
            "the file is a 64-bit data NetCDF file (CDF-5), which is not read yet",
        ),
        (
            "hdf5",
            &b"\x89HDF\r\n\x1a\n"[..],
            "the file is a NetCDF-4 (HDF5) file, which is not read yet",
        ),
        (
            "short",
            &CLASSIC[..100],
            "the file ends inside a NetCDF header",
        ),
        (
            "streaming",
            &streaming[..],
            "the NetCDF file is still being written (its number of records is not set), which is \
             not read yet",
        ),
    ];
    for (test, bytes, message) in cases {
        let failure = read_header(&file(test, bytes)).unwrap_err();
        assert_eq!(failure.kind, ErrorKind::InvalidData, "{test}");
        assert_eq!(failure.strerror, message, "{test}");
    }
}

#[test]
fn a_file_cut_inside_a_variable_is_refused() {
    let path = file("cut", &CLASSIC[..560]);
    let failure = read(&path, "temp", Pixel::F32, 12).unwrap_err();
    assert_eq!(
        failure.strerror,
        "the file ends inside the NetCDF variable temp"
    );
}

#[test]
fn a_dimension_the_file_does_not_have_is_refused() {
    let mut bytes = CLASSIC.to_vec();
    // The first dimension index of `temp`, at byte 176, names dimension 9 of 3.
    bytes[179] = 9;
    let failure = read_header(&file("dimension", &bytes)).unwrap_err();
    assert_eq!(
        failure.strerror,
        "the NetCDF variable temp names a dimension the file does not have"
    );
}

#[test]
fn python_floats_are_printed_as_python_prints_them() {
    assert_eq!(python_float(1.0), "1.0");
    assert_eq!(python_float(1e300), "1e+300");
    assert_eq!(python_float(1e16), "1e+16");
    assert_eq!(python_float(0.25), "0.25");
    assert_eq!(python_float(1.5e-7), "1.5e-07");
    assert_eq!(python_float(f64::NEG_INFINITY), "-inf");
    assert_eq!(python_float(f64::NAN), "nan");
}
