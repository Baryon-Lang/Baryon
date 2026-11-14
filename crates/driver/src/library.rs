//! The standard library's files, as the driver compiles them in (decision 0038).
//!
//! Each `.byn` file under `crates/driver/library/` is listed here once, by its path under
//! [`database::LIBRARY_ROOT`], and its text is included at build time, so every package has the
//! whole library without reading the file system. A new library file is added to [`LIBRARY`],
//! in alphabetical order, with the array's length raised by one.

/// The standard library's source files (decision 0038): each file's path under
/// [`database::LIBRARY_ROOT`] and its text, compiled into the driver so every package has them.
pub(crate) const LIBRARY: [(&str, &str); 59] = [
    ("<std>/arrays.byn", include_str!("../library/arrays.byn")),
    ("<std>/artist.byn", include_str!("../library/artist.byn")),
    ("<std>/axis.byn", include_str!("../library/axis.byn")),
    (
        "<std>/backend_agg.byn",
        include_str!("../library/backend_agg.byn"),
    ),
    (
        "<std>/backend_bases.byn",
        include_str!("../library/backend_bases.byn"),
    ),
    (
        "<std>/backend_pdf.byn",
        include_str!("../library/backend_pdf.byn"),
    ),
    (
        "<std>/backend_svg.byn",
        include_str!("../library/backend_svg.byn"),
    ),
    (
        "<std>/builtins.byn",
        include_str!("../library/builtins.byn"),
    ),
    ("<std>/byplot.byn", include_str!("../library/byplot.byn")),
    ("<std>/bz2.byn", include_str!("../library/bz2.byn")),
    ("<std>/cm.byn", include_str!("../library/cm.byn")),
    (
        "<std>/colorbar.byn",
        include_str!("../library/colorbar.byn"),
    ),
    ("<std>/colors.byn", include_str!("../library/colors.byn")),
    (
        "<std>/compression.zstd.byn",
        include_str!("../library/compression.zstd.byn"),
    ),
    ("<std>/contour.byn", include_str!("../library/contour.byn")),
    (
        "<std>/dejavu_sans.byn",
        include_str!("../library/dejavu_sans.byn"),
    ),
    ("<std>/fft.byn", include_str!("../library/fft.byn")),
    ("<std>/figure.byn", include_str!("../library/figure.byn")),
    ("<std>/fits.byn", include_str!("../library/fits.byn")),
    ("<std>/font.byn", include_str!("../library/font.byn")),
    (
        "<std>/frame_joins.byn",
        include_str!("../library/frame_joins.byn"),
    ),
    (
        "<std>/frame_missing.byn",
        include_str!("../library/frame_missing.byn"),
    ),
    ("<std>/frames.byn", include_str!("../library/frames.byn")),
    ("<std>/gzip.byn", include_str!("../library/gzip.byn")),
    ("<std>/h5by.byn", include_str!("../library/h5by.byn")),
    ("<std>/image.byn", include_str!("../library/image.byn")),
    ("<std>/legend.byn", include_str!("../library/legend.byn")),
    ("<std>/linalg.byn", include_str!("../library/linalg.byn")),
    ("<std>/lzma.byn", include_str!("../library/lzma.byn")),
    ("<std>/math.byn", include_str!("../library/math.byn")),
    ("<std>/netCDF4.byn", include_str!("../library/netCDF4.byn")),
    ("<std>/os.byn", include_str!("../library/os.byn")),
    ("<std>/pathlib.byn", include_str!("../library/pathlib.byn")),
    (
        "<std>/pdf_file.byn",
        include_str!("../library/pdf_file.byn"),
    ),
    ("<std>/png.byn", include_str!("../library/png.byn")),
    ("<std>/raster.byn", include_str!("../library/raster.byn")),
    ("<std>/special.byn", include_str!("../library/special.byn")),
    (
        "<std>/special_bessel_ik.byn",
        include_str!("../library/special_bessel_ik.byn"),
    ),
    (
        "<std>/special_bessel_jy.byn",
        include_str!("../library/special_bessel_jy.byn"),
    ),
    (
        "<std>/special_beta.byn",
        include_str!("../library/special_beta.byn"),
    ),
    (
        "<std>/special_carlson.byn",
        include_str!("../library/special_carlson.byn"),
    ),
    (
        "<std>/special_common.byn",
        include_str!("../library/special_common.byn"),
    ),
    (
        "<std>/special_distributions.byn",
        include_str!("../library/special_distributions.byn"),
    ),
    (
        "<std>/special_elliptic.byn",
        include_str!("../library/special_elliptic.byn"),
    ),
    (
        "<std>/special_error.byn",
        include_str!("../library/special_error.byn"),
    ),
    (
        "<std>/special_gamma.byn",
        include_str!("../library/special_gamma.byn"),
    ),
    (
        "<std>/special_helpers.byn",
        include_str!("../library/special_helpers.byn"),
    ),
    (
        "<std>/special_incomplete_beta.byn",
        include_str!("../library/special_incomplete_beta.byn"),
    ),
    (
        "<std>/special_incomplete_gamma.byn",
        include_str!("../library/special_incomplete_gamma.byn"),
    ),
    (
        "<std>/special_integrals.byn",
        include_str!("../library/special_integrals.byn"),
    ),
    (
        "<std>/special_kolmogorov.byn",
        include_str!("../library/special_kolmogorov.byn"),
    ),
    (
        "<std>/special_lambertw.byn",
        include_str!("../library/special_lambertw.byn"),
    ),
    (
        "<std>/special_zeta.byn",
        include_str!("../library/special_zeta.byn"),
    ),
    ("<std>/stats.byn", include_str!("../library/stats.byn")),
    (
        "<std>/subprocess.byn",
        include_str!("../library/subprocess.byn"),
    ),
    (
        "<std>/text_lines.byn",
        include_str!("../library/text_lines.byn"),
    ),
    ("<std>/ticker.byn", include_str!("../library/ticker.byn")),
    (
        "<std>/transforms.byn",
        include_str!("../library/transforms.byn"),
    ),
    ("<std>/zlib.byn", include_str!("../library/zlib.byn")),
];
