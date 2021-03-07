Introduction
============

SCFA has a replay watching feature. When a game starts, the FA process can
connect to a replay server. As the game progresses, it sends the game's replay
to that server. Another player using FAF can then watch the game by connecting
to the same server. Additionally, the server saves replays server-side, so anyone can
watch them later.

Back in GPG times, things were simple - you picked a game, you picked a player,
you received his replay data. The server just forwarded data streams between
players. I don't remember whether it even saved replays to GPG's servers. Since
then, we have FAF, and the replay server needs to do a bit more work:

* First of all, we want to save replays to our server. We don't have the space
  to save each game from every player's perspective, so we should only save one.
  Furthermore, we shouldn't require the replay watcher to pick the stream he
  wants to watch. There's nothing more frustrating than picking a replay which
  ends halfway through because the player we picked got killed and left early.
* Additionally, the replay data we send to a player should be delayed by a few
  minutes, otherwise one could trivially cheat by watching a replay while
  playing the game. It's still possible to cheat regardless, but this makes it
  a little bit harder.
* Finally, the server needs to talk with the database a bit and save the game
  with a small json header at the start. This makes it easier to display a bunch
  of replays in a list.

The last two issues are pretty easy to solve, and to solve the first, the replay
server merges the incoming replays to produce a "canonical replay" for each
game. The details on that can be found in the "Architecture" section.

For information on deploying and using the server, see "Usage".
