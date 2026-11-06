// Dense matrix multiplication C = A B for n x n matrices, written as plain loops.
//
// Usage: matmul <n>
// Stresses: floating-point throughput, cache reuse and vectorization of the innermost loop.
// Checksum: sum of every element of C.
//
// No BLAS: the suite measures what the compiler does with straightforward code. Element (i, j) of A
// and B is drawn row by row in both languages, so both multiply the same matrices. The loop order
// i, k, j keeps the inner loop contiguous in row-major storage, the mirror of the Fortran order.

#include <cstdint>
#include <vector>

#include "support.hpp"

int main(int argc, char** argv) {
    const std::int64_t n = support::argument(argc, argv, 1, "<n>");

    std::vector<double> a(n * n), b(n * n), c(n * n, 0.0);
    support::Random random;
    for (std::int64_t i = 0; i < n; ++i) {
        for (std::int64_t j = 0; j < n; ++j) a[i * n + j] = random.uniform(-1.0, 1.0);
    }
    for (std::int64_t i = 0; i < n; ++i) {
        for (std::int64_t j = 0; j < n; ++j) b[i * n + j] = random.uniform(-1.0, 1.0);
    }

    const double start = support::seconds();
    for (std::int64_t i = 0; i < n; ++i) {
        for (std::int64_t k = 0; k < n; ++k) {
            const double a_ik = a[i * n + k];
            for (std::int64_t j = 0; j < n; ++j) c[i * n + j] += a_ik * b[k * n + j];
        }
    }
    const double elapsed = support::seconds() - start;

    double checksum = 0.0;
    for (const double value : c) checksum += value;
    support::report(checksum, elapsed);
    return 0;
}
