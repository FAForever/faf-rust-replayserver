#!/bin/sh

# See https://github.com/mozilla/grcov for initial setup.
# HTMl coverage info will be placed in ./coverage/ .

rm -rf ./*.profraw
rm -rf ./coverage_run/debug/deps/*.gcda
rm -rf ./coverage_run/debug/deps/*.gcno

export RUSTFLAGS="-Zinstrument-coverage"

env CARGO_TARGET_DIR=./coverage_build cargo build
env CARGO_TARGET_DIR=./coverage_build LLVM_PROFILE_FILE=default.profraw cargo test $@

export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Zprofile -Ccodegen-units=1 -Copt-level=0 -Clink-dead-code -Coverflow-checks=off -Zpanic_abort_tests -Cpanic=abort"
export RUSTDOCFLAGS="-Cpanic=abort"

env CARGO_TARGET_DIR=./coverage_run cargo build
env CARGO_TARGET_DIR=./coverage_run cargo test $@

grcov . --binary-path ./coverage_run/debug/ -s . -t html --branch --ignore-not-existing --ignore "/*" -o ./coverage
