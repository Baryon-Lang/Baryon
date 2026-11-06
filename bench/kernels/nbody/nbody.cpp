// N-body gravity: every body attracts every other, integrated with kick-drift-kick leapfrog.
//
// Usage: nbody <bodies> <steps>
// Stresses: an O(n^2) all-pairs loop with a reciprocal square root; vectorization of a reduction.
// Checksum: total energy (kinetic plus softened potential) after the last step.

#include <cmath>
#include <cstdint>
#include <vector>

#include "support.hpp"

namespace {

constexpr double time_step = 1.0e-3;
constexpr double softening_squared = 1.0e-2;

// Bodies in structure-of-arrays layout, so the inner loop reads contiguous memory.
struct Bodies {
    std::vector<double> x, y, z, vx, vy, vz, ax, ay, az, mass;
};

// Sets each body's acceleration from every body, itself included: its own term is zero thanks to
// the softening, and keeping it removes a branch from the inner loop.
void accelerate(Bodies& b, std::int64_t n) {
    for (std::int64_t i = 0; i < n; ++i) {
        double ax = 0.0, ay = 0.0, az = 0.0;
        for (std::int64_t j = 0; j < n; ++j) {
            const double dx = b.x[j] - b.x[i];
            const double dy = b.y[j] - b.y[i];
            const double dz = b.z[j] - b.z[i];
            const double r2 = dx * dx + dy * dy + dz * dz + softening_squared;
            const double inverse_r3 = b.mass[j] / (r2 * std::sqrt(r2));
            ax += dx * inverse_r3;
            ay += dy * inverse_r3;
            az += dz * inverse_r3;
        }
        b.ax[i] = ax;
        b.ay[i] = ay;
        b.az[i] = az;
    }
}

// Kinetic plus potential energy, each distinct pair counted once.
double energy(const Bodies& b, std::int64_t n) {
    double total = 0.0;
    for (std::int64_t i = 0; i < n; ++i) {
        total += 0.5 * b.mass[i] * (b.vx[i] * b.vx[i] + b.vy[i] * b.vy[i] + b.vz[i] * b.vz[i]);
        for (std::int64_t j = i + 1; j < n; ++j) {
            const double dx = b.x[j] - b.x[i];
            const double dy = b.y[j] - b.y[i];
            const double dz = b.z[j] - b.z[i];
            total -= b.mass[i] * b.mass[j] / std::sqrt(dx * dx + dy * dy + dz * dz + softening_squared);
        }
    }
    return total;
}

}  // namespace

int main(int argc, char** argv) {
    const char* usage = "<bodies> <steps>";
    const std::int64_t n = support::argument(argc, argv, 1, usage);
    const std::int64_t steps = support::argument(argc, argv, 2, usage);

    Bodies b;
    for (auto* field : {&b.x, &b.y, &b.z, &b.vx, &b.vy, &b.vz, &b.ax, &b.ay, &b.az, &b.mass}) {
        field->assign(n, 0.0);
    }
    support::Random random;
    for (std::int64_t i = 0; i < n; ++i) {
        b.x[i] = random.uniform(-1.0, 1.0);
        b.y[i] = random.uniform(-1.0, 1.0);
        b.z[i] = random.uniform(-1.0, 1.0);
        b.vx[i] = random.uniform(-0.05, 0.05);
        b.vy[i] = random.uniform(-0.05, 0.05);
        b.vz[i] = random.uniform(-0.05, 0.05);
        b.mass[i] = 1.0 / static_cast<double>(n);
    }

    const double start = support::seconds();
    accelerate(b, n);
    for (std::int64_t step = 0; step < steps; ++step) {
        for (std::int64_t i = 0; i < n; ++i) {
            b.vx[i] += 0.5 * time_step * b.ax[i];
            b.vy[i] += 0.5 * time_step * b.ay[i];
            b.vz[i] += 0.5 * time_step * b.az[i];
            b.x[i] += time_step * b.vx[i];
            b.y[i] += time_step * b.vy[i];
            b.z[i] += time_step * b.vz[i];
        }
        accelerate(b, n);
        for (std::int64_t i = 0; i < n; ++i) {
            b.vx[i] += 0.5 * time_step * b.ax[i];
            b.vy[i] += 0.5 * time_step * b.ay[i];
            b.vz[i] += 0.5 * time_step * b.az[i];
        }
    }
    const double elapsed = support::seconds() - start;

    support::report(energy(b, n), elapsed);
    return 0;
}
