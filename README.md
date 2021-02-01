Third take on the FAF replay server, this time in Rust.

Roadmap (obsolete, irrelevant)
------------------------------

* Implement + test connection accepting and passing them to worker threads. (IN PROGRESS)
  * Test graceful shutdown.
* Add stubs for initial read (replay number + reader/writer) and further
  handling (Replay).
  * Test threads and accepting by substituting these stubs.
* Implement config, similar to Python config, but flat this time.
* Add logging.
* Investigate how to control backpressure in Rust.
* Add *real* connection header reading.
* Add half-stub Replay/Replays classes.
  * Implement creating new replays and replay life cycle. Each replay will need
    its own coroutine, this might be tricky.
  * This is the first place we start sleeping. Figure out the best way to speed
    up timers for tests.
* Pick a data type suitable for replay data. Must be:
  * Fast to append and to remove from the back,
  * Not necessarily contiguous, but made of large chunks.
  * Not required to zero-copy receive and send data (possible in theory?)
  * B-tree sounds fine.
* Add a replay data class. This includes Rust analogues of Python coroutines,
  like waiting for data reaching a certain position. Might be tricky to do
  and surrender ownership at the same time.
  * Remember about delayed data.
* Implement reading in replay headers and data.
  * Remember about graceful shutdown.
* Implement delayed data for replays.
* Implement a trivial 'take first' merge strategy.
* Implement writing replay headers and data.
  * Remember about graceful shutdown.
* Implement the Python server merge strategy.
* Implement stub replay saving. No DB or json header just yet, only saving on
  disk and compressing in a dedicated thread.
* Implement reading from DB, single thread, probably using RefCell.
* Add grafana.
* Integration tests.

Above all, TESTS TESTS TESTS. Architecture is roughly planned out, so hopefully
we can write tests as we write pieces of the server.

Random notes
------------

Unit testing
============
`faux` is amazing.

Sharing things in worker threads
================================

Use `RefCell`, never borrow across an await.

Fake time
=========

`tokio::time::pause()` does everything automagically.



Things that will turn into a real README
----------------------------------------

Running database tests
======================

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
