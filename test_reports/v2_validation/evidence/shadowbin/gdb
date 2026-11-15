#!/bin/sh
if [ "$1" = "--version" ]; then exec /usr/bin/gdb "$@"; fi
/usr/bin/gdb "$@" & G=$!
sleep 0.7
kill -9 $G 2>/dev/null
wait $G 2>/dev/null
exit 137
