// Fast Fourier transform: iterative radix-2 Cooley-Tukey on 2^p complex points, repeated.
//
// Usage: fft <log2 of points> <repetitions>
// Stresses: strided access that changes every pass, a bit-reversal permutation, complex arithmetic.
// Checksum: sum of the magnitudes of the transform from the last repetition.
//
// Complex numbers are stored as separate real and imaginary arrays and multiplied by hand, so both
// languages do exactly the same floating-point operations. (std::complex adds NaN-recovery branches
// to every multiplication that Fortran's complex type does not.) Twiddle factors are computed once,
// outside the timed region, as a real FFT library would.

#include <cmath>
#include <cstdint>
#include <utility>
#include <vector>

#include "support.hpp"

int main(int argc, char** argv) {
    const char* usage = "<log2 of points> <repetitions>";
    const std::int64_t log2_points = support::argument(argc, argv, 1, usage);
    const std::int64_t repetitions = support::argument(argc, argv, 2, usage);
    const std::int64_t n = std::int64_t{1} << log2_points;

    std::vector<double> input_re(n), input_im(n), re(n), im(n), twiddle_re(n / 2), twiddle_im(n / 2);
    support::Random random;
    for (std::int64_t i = 0; i < n; ++i) {
        input_re[i] = random.uniform(-1.0, 1.0);
        input_im[i] = random.uniform(-1.0, 1.0);
    }
    const double pi = std::acos(-1.0);
    for (std::int64_t k = 0; k < n / 2; ++k) {
        const double angle = -2.0 * pi * static_cast<double>(k) / static_cast<double>(n);
        twiddle_re[k] = std::cos(angle);
        twiddle_im[k] = std::sin(angle);
    }

    const double start = support::seconds();
    for (std::int64_t repetition = 0; repetition < repetitions; ++repetition) {
        re = input_re;
        im = input_im;
        // Bit-reversal permutation.
        for (std::int64_t i = 1, j = 0; i < n; ++i) {
            std::int64_t bit = n >> 1;
            for (; j & bit; bit >>= 1) j ^= bit;
            j ^= bit;
            if (i < j) {
                std::swap(re[i], re[j]);
                std::swap(im[i], im[j]);
            }
        }
        // Butterfly passes over blocks of length 2, 4, ..., n.
        for (std::int64_t length = 2; length <= n; length <<= 1) {
            const std::int64_t half = length >> 1;
            const std::int64_t stride = n / length;
            for (std::int64_t block = 0; block < n; block += length) {
                for (std::int64_t k = 0; k < half; ++k) {
                    const double w_re = twiddle_re[k * stride];
                    const double w_im = twiddle_im[k * stride];
                    const std::int64_t top = block + k;
                    const std::int64_t bottom = top + half;
                    const double t_re = w_re * re[bottom] - w_im * im[bottom];
                    const double t_im = w_re * im[bottom] + w_im * re[bottom];
                    re[bottom] = re[top] - t_re;
                    im[bottom] = im[top] - t_im;
                    re[top] += t_re;
                    im[top] += t_im;
                }
            }
        }
    }
    const double elapsed = support::seconds() - start;

    double checksum = 0.0;
    for (std::int64_t i = 0; i < n; ++i) checksum += std::sqrt(re[i] * re[i] + im[i] * im[i]);
    support::report(checksum, elapsed);
    return 0;
}
