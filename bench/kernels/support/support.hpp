// Shared support for the C++ kernels: argument parsing, the random number generator, the timer
// and the report every kernel prints. `support.f90` is the Fortran twin of this file; the two
// must stay identical in behavior, because the harness checks that both languages agree.

#ifndef NPL_BENCH_SUPPORT_HPP
#define NPL_BENCH_SUPPORT_HPP

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>

namespace support {

// Returns command-line argument `position` (counted from 1) as an integer. Exits with status 2
// and a usage message if the argument is missing or is not a positive integer.
inline std::int64_t argument(int argc, char** argv, int position, const char* usage) {
    if (position >= argc) {
        std::fprintf(stderr, "usage: %s %s\n", argv[0], usage);
        std::exit(2);
    }
    char* end = nullptr;
    const long long value = std::strtoll(argv[position], &end, 10);
    if (*end != '\0' || value <= 0) {
        std::fprintf(stderr, "usage: %s %s\n", argv[0], usage);
        std::exit(2);
    }
    return static_cast<std::int64_t>(value);
}

// Marsaglia's xorshift64 generator with shifts 13, 7, 17. It uses only shifts and exclusive-or,
// which Fortran defines for signed 64-bit integers, so both languages produce the same stream.
class Random {
public:
    // The fixed seed every kernel starts from, so every run sees the same input data.
    static constexpr std::uint64_t seed = 88172645463325252ULL;

    // Returns a uniform double in [0, 1) built from the top 53 bits of the next state.
    double uniform() {
        state_ ^= state_ << 13;
        state_ ^= state_ >> 7;
        state_ ^= state_ << 17;
        return static_cast<double>(state_ >> 11) * 0x1.0p-53;
    }

    // Returns a uniform double in [low, high).
    double uniform(double low, double high) { return low + (high - low) * uniform(); }

private:
    std::uint64_t state_ = seed;
};

// Seconds since an arbitrary fixed point, from a monotonic clock.
inline double seconds() {
    const auto now = std::chrono::steady_clock::now().time_since_epoch();
    return std::chrono::duration<double>(now).count();
}

// Prints the two lines the harness reads: the checksum that proves the result, and the time the
// timed region took, in seconds.
inline void report(double checksum, double elapsed) {
    std::printf("checksum %.17e\nseconds %.17e\n", checksum, elapsed);
}

}  // namespace support

#endif  // NPL_BENCH_SUPPORT_HPP
