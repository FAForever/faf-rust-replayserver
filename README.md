Third take on the FAF replay server, this time in Rust.

TODO
====

* Documentation comparable to python server's
  * Building & running tests
  * Usage
  * Architecture
  * Other stuff?
* Finish up tests
  * Fix holes in coverage where it makes sense
  * More exhausive merge strategy tests
  * Some tests of server as a whole, maybe? I don't know
  * Any other stuff I can think of
* Dockerfile + CI
* Proper README
* Testing on the test server


Things that will turn into a real README
========================================

Running database tests
----------------------

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
