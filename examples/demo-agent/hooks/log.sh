#!/bin/sh
# No-op hook demo: drain stdin (avoids EPIPE noise), print nothing, exit 0.
# Empty stdout parses as `allow`, so every tool call proceeds.
cat > /dev/null
exit 0
