// Monte Carlo integration of exp(-|x|^2) over the 6-dimensional unit hypercube.
//
// Usage: monte_carlo <samples>
// Stresses: a serial random number stream, a transcendental function, a long reduction.
// Checksum: the estimate. The exact value is (sqrt(pi) / 2 * erf(1))^6 = 0.17350...;
// the error of the estimate shrinks as 1 / sqrt(samples).

#include <cmath>
#include <cstdint>

#include "support.hpp"

namespace {

constexpr int dimensions = 6;

}  // namespace

int main(int argc, char** argv) {
    const std::int64_t samples = support::argument(argc, argv, 1, "<samples>");

    support::Random random;
    const double start = support::seconds();
    double sum = 0.0;
    for (std::int64_t sample = 0; sample < samples; ++sample) {
        double radius_squared = 0.0;
        for (int d = 0; d < dimensions; ++d) {
            const double coordinate = random.uniform();
            radius_squared += coordinate * coordinate;
        }
        sum += std::exp(-radius_squared);
    }
    const double estimate = sum / static_cast<double>(samples);
    const double elapsed = support::seconds() - start;

    support::report(estimate, elapsed);
    return 0;
}
