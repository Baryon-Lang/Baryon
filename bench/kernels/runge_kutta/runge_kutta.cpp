// Classical fourth-order Runge-Kutta on an ensemble of stiff Van der Pol oscillators,
//   x' = y,  y' = mu (1 - x^2) y - x,  with mu = 10,
// each starting from a different random amplitude.
//
// Usage: runge_kutta <oscillators> <steps>
// Stresses: many independent small systems; vectorization across the ensemble.
// Checksum: sum of x + y over the ensemble after the last step.
//
// mu = 10 makes the problem moderately stiff. The fixed step of 1e-3 sits well inside the explicit
// stability region; stiff implicit solvers are outside this suite.

#include <cstdint>
#include <vector>

#include "support.hpp"

namespace {

constexpr double mu = 10.0;
constexpr double time_step = 1.0e-3;

// The right-hand side of the Van der Pol system for velocity component y.
inline double acceleration(double x, double y) { return mu * (1.0 - x * x) * y - x; }

}  // namespace

int main(int argc, char** argv) {
    const char* usage = "<oscillators> <steps>";
    const std::int64_t oscillators = support::argument(argc, argv, 1, usage);
    const std::int64_t steps = support::argument(argc, argv, 2, usage);

    std::vector<double> x(oscillators), y(oscillators, 0.0);
    support::Random random;
    for (std::int64_t i = 0; i < oscillators; ++i) x[i] = random.uniform(0.5, 2.0);

    const double start = support::seconds();
    const double h = time_step;
    for (std::int64_t step = 0; step < steps; ++step) {
        for (std::int64_t i = 0; i < oscillators; ++i) {
            const double x0 = x[i], y0 = y[i];
            const double kx1 = y0;
            const double ky1 = acceleration(x0, y0);
            const double kx2 = y0 + 0.5 * h * ky1;
            const double ky2 = acceleration(x0 + 0.5 * h * kx1, y0 + 0.5 * h * ky1);
            const double kx3 = y0 + 0.5 * h * ky2;
            const double ky3 = acceleration(x0 + 0.5 * h * kx2, y0 + 0.5 * h * ky2);
            const double kx4 = y0 + h * ky3;
            const double ky4 = acceleration(x0 + h * kx3, y0 + h * ky3);
            x[i] = x0 + h / 6.0 * (kx1 + 2.0 * kx2 + 2.0 * kx3 + kx4);
            y[i] = y0 + h / 6.0 * (ky1 + 2.0 * ky2 + 2.0 * ky3 + ky4);
        }
    }
    const double elapsed = support::seconds() - start;

    double checksum = 0.0;
    for (std::int64_t i = 0; i < oscillators; ++i) checksum += x[i] + y[i];
    support::report(checksum, elapsed);
    return 0;
}
