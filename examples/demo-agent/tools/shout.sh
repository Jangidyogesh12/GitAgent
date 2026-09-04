#!/bin/sh
# Declarative tool demo: args arrive as JSON on stdin; stdout is the result.
# Uppercases the whole payload (no dependencies beyond POSIX sh).
input=$(cat)
echo "$input" | tr '[:lower:]' '[:upper:]'
