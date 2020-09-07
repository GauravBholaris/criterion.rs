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
elif [ "$RUSTFMT" = "yes" ]; then
    cargo fmt --all -- --check
else
    cargo build $BUILD_ARGS --release

    # TODO: Remove this hack once we no longer have to support 1.23 and 1.20
    if [ "$TRAVIS_RUST_VERSION" = "stable" ]; then
        cargo test $BUILD_ARGS --all --release
    else
        cargo test $BUILD_ARGS --all --release --tests
    fi

    cargo bench $BUILD_ARGS --all -- --test
fi
