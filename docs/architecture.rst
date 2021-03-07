Architecture
============

General
-------

Two most important entities in the replay server are the Connection and the
Replay. The Connection represents someone connecting to the server - either to
send us a replay stream from a game in progress, or to receive one. The Replay
is the game itself - data from Connections is merged in the Replay, merged data
is sent from it and a finished Replay is saved.


Connection lifetime
-------------------

At the start, FA sends us a small header that looks like this:

``G/12423353/name\0``

The first letter is "P" for sending replay data ( that is, a "writer"
connection) or "G" for receiving it (a "reader" connection). The number is the
replay ID. The text used to indicate which player's replay we send/want, but
now that we merge replays it's meaningless.

After the header is read, we check if a Replay with the given ID is in progress
or, if applicable, we create one. If found, we give the Connection to the
Replay, where replay data is either read from it or sent to it.

Replay lifetime
---------------

A Replay is created when we accept the first writer connection with its ID. The
Replay reads data from writer connections and uses a merging algorithms to
merge it into a "canonical" replay. At the same time it sends already merged
data to reader connections.

At some point all writer connections end and no more writer connections will
arrive. Once there have been no writer connection for a while (e.g. 30
seconds), the Replay stops merging data, stops accepting new connections and
saves the replay on disk. After that, once there are no more connections
remaining, the replay is over.

If the replay has been going for too long, it times out. Connections get
dropped, data merging ends, replay gets saved, all immediately.

General info
------------

The server, as a whole, interacts with the world in three ways:
* It accepts connections on a TCP port,
* It communicates with the database,
* It saves replays to a directory.

The server uses a single thread for accepting connections and reading the
initial header. From there connections are distributed among a pool of worker
threads. Each thread keeps track of its share of replays, creating new replays
and giving them connections as appropriate.

Architecture of a Replay
------------------------

The most complicated part of the server is the Replay itself. Seen from
outside, it's given connections, saves a replay at some point, then ends. Under
the hood, it looks something like this:

.. figure:: ./diagrams/replay.png

It works more-or-less as follows:

* For every writer connection, a writer replay is created which holds replay
  data and information on its progress and delayed position. A task reads data
  from the connection and appends it to the writer replay. At the same time, the
  same task periodically calculates writer replay's delayed position and calls
  the merge strategy to react to replay's new data.
* The merge strategy holds references to writer replays, which it shares with
  the above tasks. It's periodically woken up to react to changes to writer
  replays, merging them into a canonical merged replay.
* Reader connections are given the merged replay, which they share with one
  another and the merge strategy. A task waits for data becoming available in
  the merged replay and sends it out.

The merge strategy
------------------

There's a lot to write about. See ``src/replay/receive/merge_strategy.rs`` for
the general idea and ``src/replay/receive/quorum_merge_strategy.rs`` for the
description of the specific strategy we're using.

Rationale
---------

**Q:** Why is the server using its own thread pool instead of using Tokio's
multithreaded runtime?

**A:** Two reasons.

First, writer and merged replays are mutably shared by a few structs. It's
easier to reason about them when we're in a single thread, since every block
that does not await is atomic. I tried to make an architecture without mutable
sharing, but couldn't think of anything that wouldn't be a huge mess. Maybe it
can be done.

Second, because we have these mutably shared items, we'd have to place them
behind mutexes, and these mutexes would be accessed really often. Call it
premature optimization, I just don't like it.
