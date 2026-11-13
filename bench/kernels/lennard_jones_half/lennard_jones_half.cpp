// Lennard-Jones molecular dynamics on several threads, with a half neighbour list: the same liquid,
// lattice, velocities and steps as `lennard_jones.cpp`, but each pair is computed once and its force
// added into both atoms, as Newton's third law allows.
//
// Usage: lennard_jones_half <cells per side> <steps> <threads>
//   The crystal starts as a face-centred cubic lattice of 4 * cells^3 atoms. Use 5 or more cells
//   so the force cell grid is at least 3 wide. `threads` threads share the force loop.
// Stresses: adding into a shared array from parallel iterations, the pattern Baryon writes as
//   `prange(n, accumulate=forces)` (decision 0013); half the pair arithmetic of `lennard_jones`.
// Checksum: total energy per atom after the last step, in reduced units.
//
// The Fortran twin uses OpenMP. This file uses `std::thread`, because Apple's clang++ has no
// OpenMP without an extra library; the work each thread does, and the order of every addition,
// is the same as in `lennard_jones_half.f90`. Each thread adds into its own copy of the force
// arrays and the copies are summed in thread order afterwards, so for a given thread count the
// result is the same on every run, which the harness requires.

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <thread>
#include <vector>

#include "support.hpp"

namespace {

constexpr double density = 0.8442;
constexpr double temperature = 0.72;
constexpr double cutoff = 2.5;
constexpr double time_step = 0.005;

// Atoms in structure-of-arrays layout, plus the cell list that finds neighbors.
struct System {
    std::int64_t atoms = 0;
    double box = 0.0;
    std::int64_t cells = 0;  // Force cells per side; each is at least `cutoff` wide.
    std::vector<double> x, y, z, vx, vy, vz, fx, fy, fz;
    std::vector<std::int64_t> head, next;  // Linked list of atoms per cell; -1 ends a list.
    std::int64_t threads = 1;
    // Each thread's own copy of the forces, thread `t` at `t * atoms`, and its share of the energy.
    std::vector<double> thread_fx, thread_fy, thread_fz, thread_energy;
};

// Wraps a coordinate into [0, box).
double wrap(double coordinate, double box) { return coordinate - box * std::floor(coordinate / box); }

// The cell index along one axis of a wrapped coordinate.
std::int64_t cell_of(double coordinate, const System& s) {
    const auto cell = static_cast<std::int64_t>(coordinate / s.box * static_cast<double>(s.cells));
    return cell < s.cells ? cell : s.cells - 1;
}

// Adds the forces of the pairs whose lower-index atom is in [first, last) into thread `t`'s copy,
// and records the thread's share of the potential energy.
void add_pair_forces(System& s, std::int64_t t, std::int64_t first, std::int64_t last) {
    const std::int64_t m = s.cells;
    const double cutoff_squared = cutoff * cutoff;
    double* const tx = s.thread_fx.data() + t * s.atoms;
    double* const ty = s.thread_fy.data() + t * s.atoms;
    double* const tz = s.thread_fz.data() + t * s.atoms;
    std::fill(tx, tx + s.atoms, 0.0);
    std::fill(ty, ty + s.atoms, 0.0);
    std::fill(tz, tz + s.atoms, 0.0);
    double partial = 0.0;
    for (std::int64_t i = first; i < last; ++i) {
        const std::int64_t cx = cell_of(s.x[i], s);
        const std::int64_t cy = cell_of(s.y[i], s);
        const std::int64_t cz = cell_of(s.z[i], s);
        for (std::int64_t dz = -1; dz <= 1; ++dz) {
            for (std::int64_t dy = -1; dy <= 1; ++dy) {
                for (std::int64_t dx = -1; dx <= 1; ++dx) {
                    const std::int64_t neighbor =
                        (((cz + dz + m) % m) * m + (cy + dy + m) % m) * m + (cx + dx + m) % m;
                    for (std::int64_t j = s.head[neighbor]; j != -1; j = s.next[j]) {
                        if (j <= i) continue;
                        double rx = s.x[i] - s.x[j];
                        double ry = s.y[i] - s.y[j];
                        double rz = s.z[i] - s.z[j];
                        rx -= s.box * std::round(rx / s.box);
                        ry -= s.box * std::round(ry / s.box);
                        rz -= s.box * std::round(rz / s.box);
                        const double r2 = rx * rx + ry * ry + rz * rz;
                        if (r2 < cutoff_squared) {
                            const double inverse_r2 = 1.0 / r2;
                            const double inverse_r6 = inverse_r2 * inverse_r2 * inverse_r2;
                            const double magnitude = 24.0 * inverse_r2 * inverse_r6 * (2.0 * inverse_r6 - 1.0);
                            tx[i] += magnitude * rx;
                            ty[i] += magnitude * ry;
                            tz[i] += magnitude * rz;
                            tx[j] -= magnitude * rx;
                            ty[j] -= magnitude * ry;
                            tz[j] -= magnitude * rz;
                            partial += 4.0 * inverse_r6 * (inverse_r6 - 1.0);
                        }
                    }
                }
            }
        }
    }
    s.thread_energy[t] = partial;
}

// Sums the threads' copies of the forces on atoms [first, last), in thread order.
void sum_thread_forces(System& s, std::int64_t first, std::int64_t last) {
    for (std::int64_t i = first; i < last; ++i) {
        double fx = s.thread_fx[i], fy = s.thread_fy[i], fz = s.thread_fz[i];
        for (std::int64_t t = 1; t < s.threads; ++t) {
            fx += s.thread_fx[t * s.atoms + i];
            fy += s.thread_fy[t * s.atoms + i];
            fz += s.thread_fz[t * s.atoms + i];
        }
        s.fx[i] = fx;
        s.fy[i] = fy;
        s.fz[i] = fz;
    }
}

// Runs `work(t, first, last)` on `s.threads` threads, thread `t` taking a contiguous range of the
// atoms, and waits for all of them.
template <typename Work>
void on_threads(const System& s, Work work) {
    std::vector<std::thread> pool;
    for (std::int64_t t = 0; t < s.threads; ++t) {
        pool.emplace_back(work, t, t * s.atoms / s.threads, (t + 1) * s.atoms / s.threads);
    }
    for (auto& thread : pool) thread.join();
}

// Sets every force and returns the potential energy. Each pair is visited once, from the atom with
// the lower index, and its force is added into both atoms with opposite signs.
double compute_forces(System& s) {
    const std::int64_t m = s.cells;
    std::fill(s.head.begin(), s.head.end(), -1);
    for (std::int64_t i = 0; i < s.atoms; ++i) {
        const std::int64_t cell = (cell_of(s.z[i], s) * m + cell_of(s.y[i], s)) * m + cell_of(s.x[i], s);
        s.next[i] = s.head[cell];
        s.head[cell] = i;
    }

    on_threads(s, [&s](std::int64_t t, std::int64_t first, std::int64_t last) {
        add_pair_forces(s, t, first, last);
    });
    on_threads(s, [&s](std::int64_t, std::int64_t first, std::int64_t last) {
        sum_thread_forces(s, first, last);
    });
    double potential = 0.0;
    for (std::int64_t t = 0; t < s.threads; ++t) potential += s.thread_energy[t];
    return potential;
}

// Kinetic energy, with every mass equal to one.
double kinetic_energy(const System& s) {
    double total = 0.0;
    for (std::int64_t i = 0; i < s.atoms; ++i) {
        total += 0.5 * (s.vx[i] * s.vx[i] + s.vy[i] * s.vy[i] + s.vz[i] * s.vz[i]);
    }
    return total;
}

// Builds the lattice, draws random velocities, removes net momentum and scales to `temperature`.
System initialize(std::int64_t lattice_cells, std::int64_t threads) {
    System s;
    s.threads = threads;
    s.atoms = 4 * lattice_cells * lattice_cells * lattice_cells;
    s.box = std::pow(static_cast<double>(s.atoms) / density, 1.0 / 3.0);
    s.cells = static_cast<std::int64_t>(s.box / cutoff);
    for (auto* field : {&s.x, &s.y, &s.z, &s.vx, &s.vy, &s.vz, &s.fx, &s.fy, &s.fz}) {
        field->assign(s.atoms, 0.0);
    }
    s.head.assign(s.cells * s.cells * s.cells, -1);
    s.next.assign(s.atoms, -1);
    for (auto* copy : {&s.thread_fx, &s.thread_fy, &s.thread_fz}) {
        copy->assign(s.atoms * threads, 0.0);
    }
    s.thread_energy.assign(threads, 0.0);

    const double spacing = s.box / static_cast<double>(lattice_cells);
    const double basis[4][3] = {{0.0, 0.0, 0.0}, {0.5, 0.5, 0.0}, {0.5, 0.0, 0.5}, {0.0, 0.5, 0.5}};
    std::int64_t atom = 0;
    for (std::int64_t k = 0; k < lattice_cells; ++k) {
        for (std::int64_t j = 0; j < lattice_cells; ++j) {
            for (std::int64_t i = 0; i < lattice_cells; ++i) {
                for (const auto& offset : basis) {
                    s.x[atom] = (static_cast<double>(i) + offset[0] + 0.25) * spacing;
                    s.y[atom] = (static_cast<double>(j) + offset[1] + 0.25) * spacing;
                    s.z[atom] = (static_cast<double>(k) + offset[2] + 0.25) * spacing;
                    ++atom;
                }
            }
        }
    }

    support::Random random;
    double mean_x = 0.0, mean_y = 0.0, mean_z = 0.0;
    for (std::int64_t i = 0; i < s.atoms; ++i) {
        s.vx[i] = random.uniform(-0.5, 0.5);
        s.vy[i] = random.uniform(-0.5, 0.5);
        s.vz[i] = random.uniform(-0.5, 0.5);
        mean_x += s.vx[i];
        mean_y += s.vy[i];
        mean_z += s.vz[i];
    }
    const double count = static_cast<double>(s.atoms);
    mean_x /= count;
    mean_y /= count;
    mean_z /= count;
    for (std::int64_t i = 0; i < s.atoms; ++i) {
        s.vx[i] -= mean_x;
        s.vy[i] -= mean_y;
        s.vz[i] -= mean_z;
    }
    const double scale = std::sqrt(1.5 * count * temperature / kinetic_energy(s));
    for (std::int64_t i = 0; i < s.atoms; ++i) {
        s.vx[i] *= scale;
        s.vy[i] *= scale;
        s.vz[i] *= scale;
    }
    return s;
}

}  // namespace

int main(int argc, char** argv) {
    const char* usage = "<cells per side> <steps> <threads>";
    const std::int64_t lattice_cells = support::argument(argc, argv, 1, usage);
    const std::int64_t steps = support::argument(argc, argv, 2, usage);
    const std::int64_t threads = support::argument(argc, argv, 3, usage);

    System s = initialize(lattice_cells, threads);
    if (s.cells < 3) {
        std::fprintf(stderr, "lennard_jones_half: use at least 5 cells per side\n");
        return 2;
    }

    const double start = support::seconds();
    double potential = compute_forces(s);
    for (std::int64_t step = 0; step < steps; ++step) {
        for (std::int64_t i = 0; i < s.atoms; ++i) {
            s.vx[i] += 0.5 * time_step * s.fx[i];
            s.vy[i] += 0.5 * time_step * s.fy[i];
            s.vz[i] += 0.5 * time_step * s.fz[i];
            s.x[i] = wrap(s.x[i] + time_step * s.vx[i], s.box);
            s.y[i] = wrap(s.y[i] + time_step * s.vy[i], s.box);
            s.z[i] = wrap(s.z[i] + time_step * s.vz[i], s.box);
        }
        potential = compute_forces(s);
        for (std::int64_t i = 0; i < s.atoms; ++i) {
            s.vx[i] += 0.5 * time_step * s.fx[i];
            s.vy[i] += 0.5 * time_step * s.fy[i];
            s.vz[i] += 0.5 * time_step * s.fz[i];
        }
    }
    const double elapsed = support::seconds() - start;

    support::report((kinetic_energy(s) + potential) / static_cast<double>(s.atoms), elapsed);
    return 0;
}
