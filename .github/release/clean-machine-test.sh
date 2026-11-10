#!/bin/sh
# Clean-machine test for a release archive (roadmap R.2.7).
#
# Usage: clean-machine-test.sh <archive.tar.gz> <repository checkout>
#
# Checks what a new user gets from a download, on a machine with `clang` and no Rust toolchain:
# the archive's checksum matches its `.sha256` file, it unpacks to `baryon` with `libruntime.a`
# beside it, and that `baryon` builds, runs and tests the five example programs, the guide's first
# program and a small package of tests, in a debug build and with `--release`. Last, it runs
# `baryon build` with nothing but `baryon` on PATH and checks that it fails with the message that
# says how to install `clang`.
#
# The examples are checked by exit status and nonempty output only: what each one prints is
# compared, exactly, by the driver's own tests on every pull request. This script answers a
# different question, whether the packaged binary works away from the machine that built it.
#
# Exits with status 0 when every check passes, and 1 at the first that fails, naming it.

set -eu

archive=$1
repository=$(cd "$2" && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

fail() {
    echo "FAILED: $*" >&2
    exit 1
}

# The Rust toolchain must be gone, or a missing file in the archive could hide behind it.
if command -v cargo >/dev/null 2>&1 || command -v rustc >/dev/null 2>&1; then
    fail "a Rust toolchain is on PATH; this test needs a machine without one"
fi

echo "== checksum"
directory=$(dirname "$archive")
name=$(basename "$archive")
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$directory" && sha256sum -c "$name.sha256") || fail "checksum of $name"
else
    (cd "$directory" && shasum -a 256 -c "$name.sha256") || fail "checksum of $name"
fi

echo "== unpack"
tar -xzf "$archive" -C "$scratch"
install="$scratch/${name%.tar.gz}"
[ -x "$install/baryon" ] || fail "the archive has no executable baryon"
[ -f "$install/libruntime.a" ] || fail "the archive has no libruntime.a"
baryon="$install/baryon"
"$baryon" --version

# The packages to build: each example as its own package, the guide's first program, and tests.
packages="$scratch/packages"
for example in "$repository"/docs/examples/*.byn; do
    package="$packages/$(basename "$example" .byn)"
    mkdir -p "$package"
    cp "$example" "$package/main.byn"
done
mkdir -p "$packages/hello" "$packages/tests"
cat >"$packages/hello/main.byn" <<'EOF'
def main():
    print("Hello, Baryon!")
    print("1 + 2 =", 1 + 2)
EOF
cat >"$packages/tests/physics.byn" <<'EOF'
"""Free fall, with one test."""


def fall_time(height: f64, gravity: f64) -> f64:
    """Seconds to fall `height` metres from rest."""
    return (2.0 * height / gravity) ** 0.5


@test
def test_fall_time():
    assert fall_time(19.6, 9.8) == 2.0
EOF

for mode in "" --release; do
    echo "== build and run${mode:+ $mode}"
    for package in "$packages"/*; do
        [ "$(basename "$package")" = tests ] && continue
        executable="$scratch/program"
        "$baryon" build $mode "$package" "$executable" || fail "baryon build $mode $package"
        output=$("$executable") || fail "running the build of $package"
        [ -n "$output" ] || fail "the build of $package printed nothing"
        rm -rf "$executable" "$executable.dSYM"
        output=$("$baryon" run $mode "$package") || fail "baryon run $mode $package"
        [ -n "$output" ] || fail "baryon run $mode $package printed nothing"
        echo "ok $(basename "$package")"
    done
    output=$("$baryon" run $mode "$packages/hello")
    [ "$output" = "$(printf 'Hello, Baryon!\n1 + 2 = 3')" ] || fail "hello printed: $output"

    echo "== test${mode:+ $mode}"
    output=$("$baryon" test $mode "$packages/tests") || fail "baryon test $mode: $output"
    echo "$output"
    echo "$output" | grep -q "1 passed; 0 failed" || fail "baryon test $mode did not pass one test"
done

echo "== missing clang"
status=0
message=$(PATH="$install" "$baryon" build "$packages/hello" "$scratch/program" 2>&1) || status=$?
echo "$message"
[ "$status" -eq 2 ] || fail "baryon build without clang exited with $status, not 2"
case $message in
*"\`clang\` was not found"*"xcode-select --install"*"the \`clang\` package on Linux"*) ;;
*) fail "baryon build without clang did not say how to install it" ;;
esac

echo "Every check passed."
