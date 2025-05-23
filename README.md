Third take on the FAF replay server, this time in Rust.

Usage
-----

See docs/usage.rst. If you want to build the documentation, you'll need sphinx
and [dia](http://dia-installer.de/).

Running extra tests
-------------------

To run database query unit tests, do the following:
* Setup a local FAF database. The compose.yaml file will do it for you, like so:
  ```
  docker compose up -d --quiet-pull
  ```
  TODO: Move this file to a shared location.
* Clear the local database and load test data:
  ```
  ./test/clear_and_populate_db.sh <DB arguments to mysql>
  ```
* Run `cargo test` as follows:
  ```
  env DB_HOST=<db_host> DB_PORT=<db_port> cargo test --features local_db_tests
  ```

A few tests run outside `cargo test`, since they manipulate process-global
state. You can run them like so:

```
cargo build --features process_tests
./test/run_process_tests.sh
```

Sqlx compile-time checks
------------------------

Database queries are checked for correctness at compile time. You'll need to
setup a local FAF database like above and set DATABASE_URL env var to
"mysql://root:banana@127.0.0.1:3306/faf" for compilation to work.

Results of compile-time checking are saved to sqlx-data.json, which lets us
compile on CI without setting up the DB. If you're changing DB queries, run the following command:

```
env DATABASE_URL="mysql://root:banana@127.0.0.1:3306/faf" cargo sqlx prepare -- --lib
```

In order to regenerate the file.

Coverage
--------

To do code coverage, first install lcov (from your distribution) and llvm-cov:
```
cargo install cargo-llvm-cov
```
Once this is done, run `test/run_coverage.sh`.
Generated coverage is available under ./coverage.

If the script is complaining about "failed to find llvm-tools-preview", you can
give it paths to your llvm installation, like so:
```
export LLVM_COV=<path to llvm-cov>
export LLVM_PROFDATA=<path to llvm-profdata>
```

TODOs
-----

* More tests in src/replay.

Code improvement checklist
--------------------------

Code that needs no real improvement is marked with ✓.

```
├── accept ✓
├── config.rs ✓ (could add validation)
├── database ✓
├── error.rs ✓
├── main.rs ✓
├── metrics.rs ✓
├── process_test ✓
├── replay
│   ├── receive
│   │   ├── merger.rs
│   │   ├── merge_strategy.rs
│   │   ├── quorum_merge_strategy.rs
│   │   └── replay_delay.rs
│   ├── replay.rs ✓ (maybe more tests)
│   ├── replays.rs ✓
│   ├── save ✓
│   ├── send ✓
│   ├── streams
│   │   ├── header.rs ✓
│   │   ├── merged_replay_reader.rs
│   │   ├── merged_replay.rs
│   │   └── writer_replay.rs
│   └── runner.rs ✓
├── server
│   ├── connection.rs ✓
│   └── server.rs ✓ (maybe more tests)
├── util ✓
```

TL; DR the only thing that still feels bad is the whole state machine of replays.
