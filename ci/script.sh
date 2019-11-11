set -ex

if [ "$HTML_REPORTS" = "no" ]; then
    BUILD_ARGS="--no-default-features"
else
    BUILD_ARGS=""
fi

if [ "$CLIPPY" = "yes" ]; then
      cargo clippy --all -- -D warnings
elif [ "$DOCS" = "yes" ]; then
    cargo clean
    cargo doc --all --no-deps $BUILD_ARGS
    cd book
    mdbook build
    cd ..
    cp -r book/book/ target/doc/book/
    travis-cargo doc-upload || true
elif [ "$COVERAGE" = "yes" ]; then
    cargo tarpaulin --all --no-count --ciserver travis-ci --coveralls $TRAVIS_JOB_ID
elif [ "$BENCHMARK" = "yes" ]; then
    cargo bench --all $BUILD_ARGS
elif [ "$RUSTFMT" = "yes" ]; then
    cargo fmt -- --write-mode diff
else
    cargo build $BUILD_ARGS
    cargo test --all $BUILD_ARGS
    cargo build --benches --all $BUILD_ARGS
fi
