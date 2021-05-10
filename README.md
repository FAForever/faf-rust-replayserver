Third take on the FAF replay server, this time in Rust.

Usage
-----

See docs/usage.rst. If you want to build the documentation, you'll need sphinx
and [dia](http://dia-installer.de/).

Running extra tests
-------------------

To run database query unit tests, do the following:
* Setup a local database from faf-stack. See [here](https://github.com/FAForever/db).
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

Coverage
--------

To do code coverage, first configure grcov, as described at
https://github.com/mozilla/grcov. One this is done, run `test/run_coverage.sh`.
Generated coverage is available under ./coverage.

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
