#!/bin/sh
for a in "$@"; do case "$a" in --xml-file=*) out="${a#--xml-file=}";; esac; done
printf 'this is not xml at all <<<<>>>>' > "${out:-/dev/null}"
exit 0
