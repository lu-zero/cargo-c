export LLVM_PROFILE_FILE="cargo-c-%p-%m.profraw"
export RUSTFLAGS=-Zinstrument-coverage
export CARGO_INCREMENTAL=0

rustup default nightly
cargo build
cargo test
unset RUSTFLAGS

target/debug/cargo-capi capi test --manifest-path=example-project/Cargo.toml
target/debug/cargo-capi capi build --manifest-path=example-project/Cargo.toml
target/debug/cargo-cinstall cinstall --manifest-path=example-project/Cargo.toml --destdir=/tmp/staging

grcov . --binary-path target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../**' --ignore '/*' -o coverage.lcov
