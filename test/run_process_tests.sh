#!/bin/bash
# See src/util/process.rs.
# Now, cargo build --test builds execuables with some random suffix, so I can't
# just spawn them in a test. Too bad!
# Instead, run this script after executing `cargo build --features process_tests`.

./target/debug/test_process_panic 2>/dev/null
if [ ! $? -eq 1 ] ; then
	echo "Panic test failed"
else
	echo "Panic test succeeded"
fi

exec 3< <(./target/debug/test_sigint)
# $! doesn't work for some reason, oh well
PROCESS=$(pgrep test_sigint)
sleep 0.1
kill -sINT $PROCESS 2>/dev/null
sleep 0.1
kill -sKILL $PROCESS 2>/dev/null
if ! grep -q "Received a sigint" <&3 ; then
	echo "Sigint test failed"
else
	echo "Sigint test succeeded"
fi
	
