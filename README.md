Third take on the FAF replay server, this time in Rust.

Roadmap
-------

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
After initially accepting a connection, they stay in their worker thread until
they die. Same goes for replays, so we don't worry about Send, mutexes and
other stuff. Maybe we'll share the DB connection.

We don't want to worry about ownership in a single thread, only one task will
use a thing at a time. Investigate `futures_intrusive::sync::LocalMutex` for
efficient single-threaded 'mutexes'. It's equivalent to UnsafeCell, but looks
prettier.

Fake time
=========
Investigate a good way to fake fime for tests. Something executor-assisted
would be nice. If absolutely necessary, roll our own.

So, `async-std` uses `Timer` from `async-io`, which runs all timers and i/o
events on a separate thread with epoll. This doesn't sound mockable at all.
Maybe switch to tokio?
