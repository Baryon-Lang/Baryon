//! Tests of FITS files: the cards written are `AstroPy`'s (8.0.2), and the files read are written
//! by these tests under the system's temporary directory, block by block as the standard lays
//! them out.

use super::*;

/// A path for one test's file, named after it and this process.
fn file(test: &str) -> String {
    let path = std::env::temp_dir().join(format!("baryon-fits-{}-{test}.fits", std::process::id()));
    path.to_str().unwrap().to_owned()
}

/// A header of `cards` and `END`, padded with blank cards to a whole block.
fn block(cards: &[&str]) -> Vec<u8> {
    let mut text = String::new();
    for card in cards.iter().chain(&["END"]) {
        push_card(&mut text, card);
    }
    let size = usize::try_from(padded(text.len() as u64)).unwrap();
    text.push_str(&" ".repeat(size - text.len()));
    text.into_bytes()
}

/// `data` padded with zeros to a whole block.
fn data(bytes: &[u8]) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    bytes.resize(usize::try_from(padded(bytes.len() as u64)).unwrap(), 0);
    bytes
}

#[test]
fn mandatory_cards_are_astropys() {
    let cards = mandatory(Pixel::U16, &[3, 2]);
    let trimmed: Vec<&str> = cards.iter().map(|card| card.trim_end()).collect();
    assert_eq!(
        trimmed,
        [
            "SIMPLE  =                    T / conforms to FITS standard",
            "BITPIX  =                   16 / array data type",
            "NAXIS   =                    2 / number of array dimensions",
            "NAXIS1  =                    3",
            "NAXIS2  =                    2",
            "EXTEND  =                    T",
            "BSCALE  =                    1",
            "BZERO   =                32768",
        ]
    );
    assert!(cards.iter().all(|card| card.len() == CARD));
    let shifted = mandatory(Pixel::U64, &[1]);
    assert_eq!(shifted[6].trim_end(), "BZERO   =  9223372036854775808");
    assert_eq!(
        mandatory(Pixel::I8, &[1])[6].trim_end(),
        "BZERO   =                 -128"
    );
    assert_eq!(mandatory(Pixel::F32, &[1]).len(), 5);
}

#[test]
fn an_extension_is_found_past_the_primary_hdu() {
    let path = file("extension");
    let mut bytes = block(&[
        "SIMPLE  =                    T",
        "BITPIX  =                   16",
        "NAXIS   =                    1",
        "NAXIS1  =                    3",
        "EXTEND  =                    T",
    ]);
    bytes.extend(data(&[0, 1, 0, 2, 0, 3]));
    bytes.extend(block(&[
        "XTENSION= 'IMAGE   '           / Image extension",
        "BITPIX  =                  -32",
        "NAXIS   =                    2",
        "NAXIS1  =                    2",
        "NAXIS2  =                    1",
        "PCOUNT  =                    0",
        "GCOUNT  =                    1",
        "BSCALE  =               2.0D+0 / a D exponent",
        "OBJECT  = 'M31 / core'",
    ]));
    bytes.extend(data(&[0x3f, 0xc0, 0, 0, 0xc0, 0x20, 0, 0]));
    fs::write(&path, &bytes).unwrap();
    let hdu = read_hdu(&path, 1).unwrap();
    assert_eq!(hdu.offset, 3 * BLOCK as u64);
    assert_eq!(
        (hdu.bitpix, hdu.axes.clone(), hdu.size),
        (-32, vec![2, 1], 8)
    );
    assert_eq!((hdu.bscale, hdu.bzero), (2.0, 0.0));
    assert_eq!(hdu.cards.len(), 9);
    assert!(hdu.cards[8].starts_with("OBJECT  = 'M31 / core'"));
    let mut memory = [0_u8; 16];
    read_image(&path, &hdu, Pixel::F64, &mut memory).unwrap();
    assert_eq!(memory[..8], 3.0_f64.to_ne_bytes());
    assert_eq!(memory[8..], (-5.0_f64).to_ne_bytes());
    let missing = read_hdu(&path, 2).unwrap_err();
    assert_eq!(missing.kind, ErrorKind::InvalidData);
    assert_eq!(missing.strerror, "there is no HDU 2; the file has 2");
    fs::remove_file(path).unwrap();
}

#[test]
fn integers_read_as_stored_or_shifted_and_floats_scale_as_astropy_does() {
    let path = file("integers");
    let mut bytes = block(&[
        "SIMPLE  =                    T",
        "BITPIX  =                   16",
        "NAXIS   =                    1",
        "NAXIS1  =                    3",
        "BSCALE  =                    1",
        "BZERO   =                32768",
    ]);
    bytes.extend(data(&[0x80, 0, 0x7f, 0xff, 0x1c, 0x40]));
    fs::write(&path, &bytes).unwrap();
    let hdu = read_hdu(&path, 0).unwrap();
    let mut unsigned = [0_u8; 6];
    read_image(&path, &hdu, Pixel::U16, &mut unsigned).unwrap();
    let values: Vec<u16> = unsigned
        .chunks_exact(2)
        .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
        .collect();
    assert_eq!(values, [0, 65535, 40000]);
    let mut floats = [0_u8; 12];
    read_image(&path, &hdu, Pixel::F32, &mut floats).unwrap();
    assert_eq!(floats[4..8], 65535.0_f32.to_ne_bytes());
    let mut signed = [0_u8; 6];
    let failure = read_image(&path, &hdu, Pixel::I16, &mut signed).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::InvalidData);
    assert_eq!(
        failure.strerror,
        "the FITS image has BITPIX = 16 with BSCALE = 1 and BZERO = 32768, which does not read \
         as `i16`; read it as a float array"
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn what_is_not_fits_is_invalid_data() {
    let cases: [(&str, Vec<u8>, &str); 4] = [
        (
            "text",
            b"1.5 2.5\n".to_vec(),
            "the file is not a FITS file: it does not start with SIMPLE",
        ),
        (
            "short",
            b"SIMPLE  =                    T".to_vec(),
            "the file ends inside a FITS header",
        ),
        (
            "simple",
            block(&["NAXIS   =                    0"]),
            "the file is not a FITS file: it does not start with SIMPLE",
        ),
        (
            "bitpix",
            block(&[
                "SIMPLE  =                    T",
                "BITPIX  =                   12",
            ]),
            "the FITS header has BITPIX = 12, which the standard does not allow",
        ),
    ];
    for (name, bytes, message) in cases {
        let path = file(name);
        fs::write(&path, bytes).unwrap();
        let failure = read_hdu(&path, 0).unwrap_err();
        assert_eq!(failure.kind, ErrorKind::InvalidData, "{name}");
        assert_eq!(failure.strerror, message, "{name}");
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn a_written_file_is_whole_blocks_and_reads_back() {
    let path = file("written");
    let cards = vec![
        format!("{:<80}", "OBJECT  = 'M31     '"),
        format!("{:<80}", "NAXIS   =                    7"),
    ];
    let values = [1.5_f32, -2.0];
    write(&path, Pixel::F32, &[2], Some(&cards), false, |writer| {
        for value in values {
            writer.write_all(&value.to_be_bytes())?;
        }
        Ok(8)
    })
    .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 2 * BLOCK);
    let hdu = read_hdu(&path, 0).unwrap();
    assert_eq!((hdu.bitpix, hdu.axes.clone()), (-32, vec![2]));
    assert_eq!(
        hdu.cards.len(),
        5,
        "the program's NAXIS card is dropped, and EXTEND is written only without a header"
    );
    assert!(hdu.cards[4].starts_with("OBJECT  = 'M31     '"));
    let again = write(&path, Pixel::F32, &[2], Some(&cards), false, |_| Ok(0)).unwrap_err();
    assert_eq!(again.kind, ErrorKind::FileExists);
    fs::remove_file(path).unwrap();
}
