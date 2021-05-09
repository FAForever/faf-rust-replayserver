#!/bin/bash
# See src/util/process.rs.
# Now, cargo build --test builds execuables with some random suffix, so I can't
# just spawn them in a test. Too bad!

cargo run --bin test_process_panic --features process_tests
if [ ! $? -eq 1 ] ; then
	echo "Panic test failed"
	exit 1
else
	echo "Panic test succeeded"
fi

exec 3< <(cargo run --bin test_sigint --features process_tests)
exec 4< <(tee /dev/stdout <&3 &)

while read line <&4 ; do
	if [ "$line" = "Waiting for sigint" ] ; then
		break
	fi
done

# $! doesn't work for some reason, oh well
PROCESS=$(pgrep test_sigint)

sleep 0.1
kill -sINT $PROCESS 2>/dev/null
sleep 0.1
kill -sKILL $PROCESS 2>/dev/null

if ! grep -q "Received a sigint" <&4 ; then
	echo "Sigint test failed"
	exit 1
else
	echo "Sigint test succeeded"
	exit 0
fi
	
