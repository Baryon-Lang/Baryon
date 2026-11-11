//! The `fft` module (roadmap 3.10, decision 0040), built and run against `NumPy`'s `numpy.fft` for
//! the same input. Inputs come from `random.Philox`, which draws `NumPy`'s numbers bit for bit,
//! and results are printed to 12 significant digits, since `NumPy`'s pocketfft adds in its own
//! order.

use super::examples::prints;

/// `fft` and `ifft` of a power-of-two length, 8, of lengths 6 and 7 through Bluestein's
/// algorithm, and of length 1; `rfft` of 9 real samples and `irfft` back, with `n` given and
/// left out; `fftfreq` and `rfftfreq`. `NumPy` prints the same lines for
///
/// ```python
/// g = numpy.random.Generator(numpy.random.Philox(key=9))
/// for n in [8, 6, 7, 1]:
///     r = g.random(2 * n).reshape(n, 2) - 0.5
///     z = r[:, 0] + 1j * r[:, 1]
/// signal = g.random(9)
/// ```
#[test]
fn transforms_match_numpy() {
    let program = r#"
import fft
from fft import fftfreq, ifft, irfft, rfft, rfftfreq
from random import Philox


def show(x: Array[complex128, 1]):
    for value in x:
        print(f"{value.real:.12g},{value.imag:.12g}", end=" ")
    print()


def main():
    generator = Philox(9)
    for n in [8, 6, 7, 1]:
        z = Array.zeros(n, complex128)
        for j in range(n):
            z[j] = complex(generator.uniform() - 0.5, generator.uniform() - 0.5)
        show(fft.fft(z))
        show(ifft(z))
    signal = Array.zeros(9)
    for j in range(9):
        signal[j] = generator.uniform()
    spectrum = rfft(signal)
    show(spectrum)
    print(irfft(spectrum, 9))
    print(irfft(spectrum))
    print(fftfreq(8, 0.1))
    print(fftfreq(5))
    print(rfftfreq(9, 2.0))
"#;
    prints(
        "fft-transforms",
        program.trim_start(),
        "0.342570425154,1.14097259146 0.262082666731,0.530054305644 -0.520671287464,0.822796141154 -1.43208126667,0.624323153121 0.443121127427,-1.75066414332 -0.0959410416002,0.502066530934 0.10377826866,-0.451374287593 -1.39568005031,0.95240840154 \n\
         0.0428213031443,0.142621573933 -0.174460006289,0.119051050193 0.0129722835825,-0.0564217859491 -0.0119926302,0.0627583163668 0.0553901409284,-0.218833017915 -0.179010158334,0.0780403941402 -0.065083910933,0.102849517644 0.0327603333413,0.0662567882055 \n\
         -0.747091641036,0.117449488134 0.00616474210141,1.06251041168 1.09261472866,0.793438384043 -1.14585249546,1.04196246039 -0.248227053962,-0.314536447786 0.430391195629,-0.0495611023816 \n\
         -0.124515273506,0.019574914689 0.0717318659382,-0.00826018373026 -0.0413711756603,-0.0524227412976 -0.19097541591,0.173660410064 0.182102454776,0.132239730674 0.0010274570169,0.177085068613 \n\
         -0.42232594983,-0.555445218172 -1.41650326711,-0.406691923228 -0.582770192622,-0.14063952433 0.254516786634,0.0618748673033 0.0756640501718,1.06698781767 -1.45778580247,-0.0850753045231 0.69459895821,0.448736887152 \n\
         -0.0603322785471,-0.0793493168817 0.0992284226014,0.0641052695932 -0.208255114639,-0.0121536149319 0.0108091500245,0.152426831096 0.0363595409477,0.00883926675761 -0.0832528846603,-0.0200913606186 -0.202357609588,-0.0580988461755 \n\
         0.197594991555,-0.0157664447383 \n\
         0.197594991555,-0.0157664447383 \n\
         5.32176042283,0 -1.13717124087,-0.469744597906 -0.615254254798,0.734171847259 -0.859514948217,0.210832394764 0.647784509381,-0.288153124032 \n\
         [0.15482762 0.22196749 0.92809402 0.8101902  0.88147125 0.80759701\n\
         \u{20}0.23589236 0.83526224 0.44645823]\n\
         [0.09320801 0.39739051 1.07015093 0.86264276 1.0915511  0.40401755\n\
         \u{20}0.72986243 0.67293715]\n\
         [ 0.    1.25  2.5   3.75 -5.   -3.75 -2.5  -1.25]\n\
         [ 0.   0.2  0.4 -0.4 -0.2]\n\
         [0.         0.05555556 0.11111111 0.16666667 0.22222222]\n",
    )
    .unwrap();
}

/// The worked example of the `fft` section in the reference's Linear algebra and FFT chapter,
/// read from the chapter itself, prints what the chapter says it prints.
#[test]
fn the_reference_example_of_rfft_runs() {
    let chapter = include_str!("../../../../docs/reference/linear-algebra.md");
    let section = &chapter[chapter.find("## `fft`").unwrap()..];
    let program = section
        .split("```python\n")
        .nth(2)
        .and_then(|block| block.split("```").next())
        .unwrap();
    let printed = section
        .split("```text\n")
        .nth(1)
        .and_then(|block| block.split("```").next())
        .unwrap();
    prints("fft-reference-rfft", program, printed).unwrap();
}
