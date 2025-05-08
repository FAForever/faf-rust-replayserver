#!/bin/sh

rm -rf coverage
mkdir coverage

env DB_HOST=localhost DB_PORT=3306 cargo llvm-cov --features local_db_tests --lcov --output-path coverage/lcov.info --ignore-filename-regex '.*rustlib.*'

cd coverage
genhtml ./lcov.info
