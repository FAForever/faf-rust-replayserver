Extra headers
=============

The original protocol does not support graceful reconnection. However, since we
moved to websockets and Cloudflare, we now have to support it as Cloudflare can
randomly close websockets. This section describes the extensions to the
protocol that were added in order to support this.

For context, read the "Architecture" section first.

Reconnection headers
--------------------

A client that wants to gracefully reconnect can send a few additional pieces of
data before the initial connection header. The initial connection header looks
like so:

``G/12423353/name\0``

Each piece of that extra data is likewise a string delimited with a null byte.
The server reads these strings, **then the initial connection header**, then
responds with other null-delimited strings. The client should send all the
strings plus the connection header before expecting a response.

Supported headers
-----------------

The following headers are currently supported:

* ``read_offset/<decimal number string>\0``

This defines a read offset when watching a live replay. The offset is counted
from the start of the replay itself (that is, the data that's saved as a replay
on disk), ignoring all connection headers. The server will send data starting
from that offset.

* ``request_writer_token\0``

This requests a connection identifier from the server. If the connection is a
replay writer, this identifier will allow reconnecting as the same peer. The
server will respond with the following string:
``request_writer_token/<base64-encoded 16 bytes>/\0``.

* ``client_writer_token/<base64-encoded 16 bytes>/\0``

This tells the server to treat the connection as a reconnection of a specific
replay writer. After providing the token, the original connection header should
still be provided. If the server finds the corresponding writer, it will respond
with the following string: ``client_writer_token/<decimal number string>\0``.
The returned number indicates the replay offset (same as in ``read_offset``)
from which the client should resume writing. If the server does not find the
corresponding writer, it will close the connection.
