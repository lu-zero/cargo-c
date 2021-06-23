export LLVM_PROFILE_FILE="cargo-c-%p-%m.profraw"
export RUSTFLAGS=-Zinstrument-coverage
export CARGO_INCREMENTAL=0

rustup default nightly
rustup target add x86_64-pc-windows-gnu
cargo build
cargo test
unset RUSTFLAGS

target/debug/cargo-capi capi --help
target/debug/cargo-capi capi test --manifest-path=example-project/Cargo.toml
target/debug/cargo-capi capi clean --manifest-path=example-project/Cargo.toml
target/debug/cargo-capi capi build --manifest-path=example-project/Cargo.toml

target/debug/cargo-cbuild --help
target/debug/cargo-cbuild clean --manifest-path=example-project/Cargo.toml
target/debug/cargo-cbuild cbuild --manifest-path=example-project/Cargo.toml
target/debug/cargo-ctest metadata --help
target/debug/cargo-ctest ctest --manifest-path=example-project/Cargo.toml

target/debug/cargo-cinstall --help
target/debug/cargo-cinstall cinstall --manifest-path=example-project/Cargo.toml --destdir=/tmp/staging
target/debug/cargo-cinstall cinstall clean --manifest-path=example-project/Cargo.toml

target/debug/cargo-cinstall cinstall --manifest-path=example-project/Cargo.toml --destdir=/tmp/staging-win --target=x86_64-pc-windows-gnu

grcov . --binary-path target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../**' --ignore '/*' -o coverage.lcov
