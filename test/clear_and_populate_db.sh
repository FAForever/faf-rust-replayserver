#!/bin/bash

set -e -u -o pipefail
shopt -s inherit_errexit

if [ $# -eq 0 ] ; then
	echo "This script passes args through to the mysql process."
	echo "Use the usual '-h 127.0.0.1 -P 3306 --user=root --password=banana faf'."
	exit 1
fi

# Sanity check so we never kill production DB with this.
# inherit_errexit only works when not part of another command
GAME_COUNT="$(mysql $@ -s -e "SELECT count(*) from game_stats" | tail -n 1)"
if [ "$GAME_COUNT" -ge 100 ] ; then
	tput setaf 1 || true
	tput bold || true
	cat << EOF
                ######################################
                # YOU ARE DOING SOMETHING VERY WRONG #
                ######################################
This script is used to drop ALL data from a database and load test data. It's
only used for unit testing the replay server. I detected that the game_stats
table has at least 100 rows, so I stopped.

DOUBLE-CHECK WHAT DATABASE YOU'RE USING. THE DATABASE FROM FAF-STACK SHOULD'VE
RAN THIS WITHOUT ERRORS.
EOF
	exit 1
fi

# Clear DB. Query inline so nobody's tempted to use this directly.
DELETE_STATEMENTS=$(mysql $@ -s -e " \
        SELECT CONCAT('DELETE FROM ', table_schema, '.', table_name, ';') \
        FROM information_schema.tables \
        WHERE table_type = 'BASE TABLE' \
	AND table_schema = 'faf'" | tail -n +1)

mysql $@ -e "SET FOREIGN_KEY_CHECKS=0; \
	     ${DELETE_STATEMENTS} \
             SET FOREIGN_KEY_CHECKS=1;"

# Populate with test data.
mysql $@ < test/db_unit_test_data.sql
