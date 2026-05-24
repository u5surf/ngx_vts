#!/usr/bin/env bash
#
# Drive randomised traffic against the nginx in the docker compose stack
# so the Grafana panels have something interesting to display.
#
# Usage:
#     ./load.sh                       # default target, ~10 req/s, forever
#     ./load.sh -u http://host:port   # override target
#     ./load.sh -r 50                 # ~50 requests / second
#     ./load.sh -d 60                 # stop after 60 seconds
#
# Distribution per request (defaults):
#   85%  cached path with one of N URLs    → mix of HIT and MISS
#   10%  /bypass with X-Bypass: 1          → BYPASS
#    5%  brand-new path each time          → MISS (forces cache growth)
#
# Ctrl-C stops the loop.

set -u

TARGET="http://127.0.0.1:18080"
RATE=10           # approximate requests per second
DURATION=0        # 0 = run forever
N_CACHED_PATHS=20 # cardinality of the cached path pool

while getopts "u:r:d:n:h" opt; do
    case "$opt" in
        u) TARGET="$OPTARG" ;;
        r) RATE="$OPTARG" ;;
        d) DURATION="$OPTARG" ;;
        n) N_CACHED_PATHS="$OPTARG" ;;
        h|*)
            sed -n '2,18p' "$0"
            exit 0
            ;;
    esac
done

if ! command -v curl >/dev/null 2>&1; then
    echo "load.sh: curl is required" >&2
    exit 1
fi

# Probe the target once so we fail fast on the obvious config mistakes
# (wrong port, stack not up, /status not allowed from this host).
if ! curl -sS -o /dev/null -w '' --max-time 3 "$TARGET/status"; then
    echo "load.sh: cannot reach $TARGET/status — is the stack running?" >&2
    exit 1
fi

interval_us=$(( 1000000 / RATE ))
deadline=0
if [ "$DURATION" -gt 0 ]; then
    deadline=$(( $(date +%s) + DURATION ))
fi

echo "load.sh: hitting $TARGET at ~${RATE} req/s (Ctrl-C to stop)"

trap 'echo; echo "load.sh: stopped after $count requests"; exit 0' INT TERM

count=0
while true; do
    if [ "$deadline" -gt 0 ] && [ "$(date +%s)" -ge "$deadline" ]; then
        echo "load.sh: duration reached, stopping after $count requests"
        exit 0
    fi

    # Pick a request kind by rolling a 0..99 die.
    roll=$(( RANDOM % 100 ))
    if [ "$roll" -lt 85 ]; then
        path="/p$(( RANDOM % N_CACHED_PATHS ))"
        curl -sS -o /dev/null --max-time 5 "$TARGET$path" || true
    elif [ "$roll" -lt 95 ]; then
        curl -sS -o /dev/null --max-time 5 -H "X-Bypass: 1" "$TARGET/bypass/" || true
    else
        # Brand-new path: forces a MISS every time.  Capped at a few
        # thousand to keep the slab pool from filling up.
        path="/once-$(( RANDOM * 1000 + count % 1000 ))"
        curl -sS -o /dev/null --max-time 5 "$TARGET$path" || true
    fi

    count=$(( count + 1 ))
    # Coarse pacing — `sleep` granularity is fine for the rates we care
    # about here (~1–200 req/s).
    if command -v perl >/dev/null 2>&1; then
        perl -e "select(undef,undef,undef,$interval_us/1000000)"
    else
        sleep "$(awk "BEGIN{print $interval_us/1000000}")"
    fi
done
