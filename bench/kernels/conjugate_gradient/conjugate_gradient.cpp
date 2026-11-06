// Conjugate gradient on a sparse matrix: the 3D Poisson problem -laplace(u) = 1 on an n^3 grid of
// unknowns with zero Dirichlet boundaries, stored in compressed sparse row (CSR) form.
//
// Usage: conjugate_gradient <points per side> <iterations>
//   Runs exactly <iterations> iterations with no early exit, so the work is fixed.
// Stresses: sparse matrix-vector products with indirect column access; dot-product reductions.
// Checksum: Euclidean norm of the solution after the last iteration.

#include <cmath>
#include <cstdint>
#include <vector>

#include "support.hpp"

namespace {

// A square matrix in compressed sparse row form. Row r holds entries offsets[r] .. offsets[r + 1].
struct SparseMatrix {
    std::vector<std::int64_t> offsets, columns;
    std::vector<double> values;
};

// Builds the 7-point Laplacian (6 on the diagonal, -1 per grid neighbor) with columns in increasing
// order. Unknown (i, j, k) is row (k * n + j) * n + i.
SparseMatrix laplacian(std::int64_t n) {
    SparseMatrix a;
    a.offsets.push_back(0);
    const auto add = [&a](std::int64_t column, double value) {
        a.columns.push_back(column);
        a.values.push_back(value);
    };
    for (std::int64_t k = 0; k < n; ++k) {
        for (std::int64_t j = 0; j < n; ++j) {
            for (std::int64_t i = 0; i < n; ++i) {
                const std::int64_t row = (k * n + j) * n + i;
                if (k > 0) add(row - n * n, -1.0);
                if (j > 0) add(row - n, -1.0);
                if (i > 0) add(row - 1, -1.0);
                add(row, 6.0);
                if (i < n - 1) add(row + 1, -1.0);
                if (j < n - 1) add(row + n, -1.0);
                if (k < n - 1) add(row + n * n, -1.0);
                a.offsets.push_back(static_cast<std::int64_t>(a.columns.size()));
            }
        }
    }
    return a;
}

// y = A x.
void multiply(const SparseMatrix& a, const std::vector<double>& x, std::vector<double>& y) {
    const auto rows = static_cast<std::int64_t>(y.size());
    for (std::int64_t row = 0; row < rows; ++row) {
        double sum = 0.0;
        for (std::int64_t e = a.offsets[row]; e < a.offsets[row + 1]; ++e) {
            sum += a.values[e] * x[a.columns[e]];
        }
        y[row] = sum;
    }
}

double dot(const std::vector<double>& u, const std::vector<double>& v) {
    double sum = 0.0;
    for (std::size_t i = 0; i < u.size(); ++i) sum += u[i] * v[i];
    return sum;
}

}  // namespace

int main(int argc, char** argv) {
    const char* usage = "<points per side> <iterations>";
    const std::int64_t n = support::argument(argc, argv, 1, usage);
    const std::int64_t iterations = support::argument(argc, argv, 2, usage);

    const SparseMatrix a = laplacian(n);
    const std::int64_t unknowns = n * n * n;
    std::vector<double> x(unknowns, 0.0), r(unknowns, 1.0), p(unknowns, 1.0), q(unknowns, 0.0);

    const double start = support::seconds();
    double rho = dot(r, r);
    for (std::int64_t iteration = 0; iteration < iterations; ++iteration) {
        multiply(a, p, q);
        const double alpha = rho / dot(p, q);
        for (std::int64_t i = 0; i < unknowns; ++i) {
            x[i] += alpha * p[i];
            r[i] -= alpha * q[i];
        }
        const double rho_next = dot(r, r);
        const double beta = rho_next / rho;
        rho = rho_next;
        for (std::int64_t i = 0; i < unknowns; ++i) p[i] = r[i] + beta * p[i];
    }
    const double elapsed = support::seconds() - start;

    support::report(std::sqrt(dot(x, x)), elapsed);
    return 0;
}
