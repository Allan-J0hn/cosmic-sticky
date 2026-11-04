#!/bin/bash
# THROWAWAY: start spike, screenshot, measure RSS and idle CPU ticks, stop.
# usage: tools/measure.sh <tag> <spike args...>
cd "$(dirname "$0")/.."
tag=$1; shift
./target/debug/notes-spike "$@" > "logs/$tag.log" 2>&1 &
pid=$!
sleep "${SETTLE:-3}"
./target/debug/wlshot shots "$tag-"
rss=$(awk '/VmRSS/{print $2}' /proc/$pid/status)
t0=$(awk '{print $14+$15}' /proc/$pid/stat)
sleep 10
t1=$(awk '{print $14+$15}' /proc/$pid/stat)
echo "$tag: RSS=${rss}kB idle CPU ticks over 10s=$((t1-t0)) (100 ticks = 1 cpu-second)"
if [ -z "$KEEP" ]; then kill $pid; else echo "left running pid=$pid"; echo $pid > logs/$tag.pid; fi
