// 3D heat equation: explicit Jacobi time stepping with the 7-point Laplacian stencil on an n^3 grid.
//
// Usage: heat_stencil <points per side> <steps>
// Stresses: memory bandwidth; a regular stencil the compiler should vectorize; two-buffer swap.
// Checksum: sum of the temperature over the grid after the last step.
//
// The grid starts as sin(pi x) sin(pi y) sin(pi z) on the unit cube, with the boundary held at zero.
// The diffusion number alpha dt / h^2 is 1/8, inside the stability limit of 1/6.

#include <cmath>
#include <cstdint>
#include <utility>
#include <vector>

#include "support.hpp"

namespace {

constexpr double diffusion_number = 0.125;

// One time step from `from` into `to`. Boundary points are never written, so they stay zero.
// Index (i, j, k) lives at (k * n + j) * n + i: i varies fastest, as in the Fortran version.
void step(const std::vector<double>& from, std::vector<double>& to, std::int64_t n) {
    const std::int64_t plane = n * n;
    for (std::int64_t k = 1; k < n - 1; ++k) {
        for (std::int64_t j = 1; j < n - 1; ++j) {
            for (std::int64_t i = 1; i < n - 1; ++i) {
                const std::int64_t c = (k * n + j) * n + i;
                to[c] = from[c] + diffusion_number * (from[c - 1] + from[c + 1] + from[c - n] + from[c + n] +
                                                      from[c - plane] + from[c + plane] - 6.0 * from[c]);
            }
        }
    }
}

}  // namespace

int main(int argc, char** argv) {
    const char* usage = "<points per side> <steps>";
    const std::int64_t n = support::argument(argc, argv, 1, usage);
    const std::int64_t steps = support::argument(argc, argv, 2, usage);

    const double pi = std::acos(-1.0);
    const double h = 1.0 / static_cast<double>(n - 1);
    std::vector<double> current(n * n * n, 0.0);
    for (std::int64_t k = 1; k < n - 1; ++k) {
        for (std::int64_t j = 1; j < n - 1; ++j) {
            for (std::int64_t i = 1; i < n - 1; ++i) {
                current[(k * n + j) * n + i] = std::sin(pi * static_cast<double>(i) * h) *
                                               std::sin(pi * static_cast<double>(j) * h) *
                                               std::sin(pi * static_cast<double>(k) * h);
            }
        }
    }
    std::vector<double> following = current;

    const double start = support::seconds();
    for (std::int64_t t = 0; t < steps; ++t) {
        step(current, following, n);
        std::swap(current, following);
    }
    const double elapsed = support::seconds() - start;

    double checksum = 0.0;
    for (const double value : current) checksum += value;
    support::report(checksum, elapsed);
    return 0;
}
