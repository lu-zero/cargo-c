export LLVM_PROFILE_FILE="cargo-c-%p-%m.profraw"
export RUSTFLAGS=-Cinstrument-coverage
export CARGO_INCREMENTAL=0

rustup default stable
cargo build
cargo test
rustup target add x86_64-pc-windows-gnu
unset RUSTFLAGS

function run() {
    echo "$*"
    $*
}

for project in example-project example-workspace; do
    run target/debug/cargo-capi capi --help
    run target/debug/cargo-capi capi test --manifest-path=${project}/Cargo.toml
    run target/debug/cargo-capi capi clean --manifest-path=${project}/Cargo.toml
    run target/debug/cargo-capi capi build --manifest-path=${project}/Cargo.toml

    run target/debug/cargo-cbuild --help
    run target/debug/cargo-cbuild clean --manifest-path=${project}/Cargo.toml
    run target/debug/cargo-cbuild cbuild --manifest-path=${project}/Cargo.toml
    run target/debug/cargo-ctest metadata --help
    run target/debug/cargo-ctest ctest --manifest-path=${project}/Cargo.toml

    run target/debug/cargo-cinstall --help
    run target/debug/cargo-cinstall cinstall --manifest-path=${project}/Cargo.toml --destdir=/tmp/staging
    run target/debug/cargo-cinstall cinstall clean --manifest-path=${project}/Cargo.toml

    run target/debug/cargo-cinstall cinstall --manifest-path=${project}/Cargo.toml --destdir=/tmp/staging-win --target=x86_64-pc-windows-gnu --dlltool=x86_64-w64-mingw32-dlltool
done

grcov . --binary-path target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../**' --ignore '/*' -o coverage.lcov
