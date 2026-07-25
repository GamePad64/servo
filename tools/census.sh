#!/usr/bin/env bash
# Census of thread spawns and blocking receives across the spawn sites that a
# headless wasip2 build must convert to tokio tasks.
#
# Why this exists: on wasm32-wasip2 `thread::spawn` fails at *runtime*
# (os error 58), and a blocking `recv()` inside a task on a single-threaded
# runtime deadlocks the whole runtime. So the cost of G1 is not the number of
# spawn sites — it is the number of blocking receives behind them, and how many
# of those wait on a *different* actor (the deadlock-prone kind) rather than
# draining their own inbox (benign).
#
# Output feeds the G1 go/no-go decision.
set -uo pipefail
cd "$(dirname "$0")/.."

SITES=(
  components/constellation/constellation.rs
  components/script/script_thread.rs
  components/net/resource_thread.rs
  components/net/connector.rs
  components/fonts/system_font_service.rs
  components/paint/refresh_driver.rs
)

printf '%-48s %6s %6s %6s %6s %6s\n' FILE lines spawn recv try_recv select
printf '%-48s %6s %6s %6s %6s %6s\n' '---' '---' '---' '---' '---' '---'

tot_spawn=0; tot_recv=0; tot_try=0; tot_sel=0
for f in "${SITES[@]}"; do
  if [ ! -f "$f" ]; then
    printf '%-48s %6s\n' "$f" MISSING
    continue
  fi
  lines=$(wc -l < "$f")
  # thread creation
  spawn=$(grep -cE 'thread::Builder::new|thread::spawn' "$f")
  # blocking receives — the ones that must become `.await`
  recv=$(grep -cE '\.recv\(\)|\.recv_timeout\(' "$f")
  # non-blocking receives — already safe on a single-threaded runtime
  try=$(grep -cE '\.try_recv\(\)' "$f")
  # multi-channel waits — each arm needs converting too
  sel=$(grep -cE 'select!|select_biased!' "$f")
  printf '%-48s %6s %6s %6s %6s %6s\n' "$f" "$lines" "$spawn" "$recv" "$try" "$sel"
  tot_spawn=$((tot_spawn + spawn)); tot_recv=$((tot_recv + recv))
  tot_try=$((tot_try + try));       tot_sel=$((tot_sel + sel))
done

printf '%-48s %6s %6s %6s %6s %6s\n' '---' '---' '---' '---' '---' '---'
printf '%-48s %6s %6s %6s %6s %6s\n' TOTAL '' "$tot_spawn" "$tot_recv" "$tot_try" "$tot_sel"

echo
echo "Repo-wide, for scale:"
printf '  %-40s %s\n' 'thread::Builder::new / thread::spawn' \
  "$(grep -rEc 'thread::Builder::new|thread::spawn' --include=*.rs components/ 2>/dev/null | awk -F: '{s+=$2} END {print s}')"
printf '  %-40s %s\n' 'blocking .recv() / .recv_timeout()' \
  "$(grep -rEc '\.recv\(\)|\.recv_timeout\(' --include=*.rs components/ 2>/dev/null | awk -F: '{s+=$2} END {print s}')"
printf '  %-40s %s\n' 'select! sites' \
  "$(grep -rEc 'select!|select_biased!' --include=*.rs components/ 2>/dev/null | awk -F: '{s+=$2} END {print s}')"

echo
echo "Files outside the six sites with the most blocking receives:"
grep -rEc '\.recv\(\)|\.recv_timeout\(' --include=*.rs components/ 2>/dev/null \
  | awk -F: '$2 > 0 {print $2"\t"$1}' | sort -rn | head -12
