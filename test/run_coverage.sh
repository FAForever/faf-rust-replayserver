#!/bin/sh

# See https://github.com/mozilla/grcov for initial setup.
# HTMl coverage info will be placed in ./coverage/ .

rm -rf ./*.profraw
rm -rf ./coverage_run/debug/deps/*.gcda
rm -rf ./coverage_run/debug/deps/*.gcno

export RUSTC_BOOTSTRAP=1
export RUSTFLAGS="-Cinstrument-coverage"

env CARGO_TARGET_DIR=coverage_run cargo build
env CARGO_TARGET_DIR=coverage_run LLVM_PROFILE_FILE="coverage_run/default-%p-%m.profraw" cargo test

grcov . --ignore "src/process_test" --ignore "src/util/process.rs" \
	--binary-path ./coverage_run/debug/ -s . -t html --branch --ignore-not-existing --ignore "/*" -o ./coverage "$@"
