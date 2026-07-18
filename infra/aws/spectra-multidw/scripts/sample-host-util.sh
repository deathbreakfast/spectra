#!/usr/bin/env bash
# Sample CPU/memory for ~DURATION_SECS into OUT_JSON (role stamped).
# Usage: sample-host-util.sh <role> <duration_secs> <out.json>
set -euo pipefail

ROLE="${1:?role}"
DURATION="${2:?duration_secs}"
OUT="${3:?out.json}"
INTERVAL=2
SAMPLES=$((DURATION / INTERVAL))
SAMPLES=$((SAMPLES < 1 ? 1 : SAMPLES))

cpu_sum=0
cpu_peak=0
mem_sum=0
mem_avail_sum=0
count=0

for _ in $(seq 1 "$SAMPLES"); do
  # Idle% from top (batch); CPU used ≈ 100 - idle
  idle="$(top -bn1 | awk '/Cpu\(s\)/{for(i=1;i<=NF;i++) if($i ~ /id/){gsub(/[^0-9.]/,"",$(i-1)); print $(i-1); exit}}')"
  idle="${idle:-0}"
  cpu="$(awk -v idle="$idle" 'BEGIN{c=100-idle; if(c<0)c=0; if(c>100)c=100; printf "%.1f", c}')"
  mem_line="$(free -m | awk '/Mem:/{print $3,$7,$2}')"
  mem_used="$(echo "$mem_line" | awk '{print $1}')"
  mem_avail="$(echo "$mem_line" | awk '{print $2}')"
  mem_total="$(echo "$mem_line" | awk '{print $3}')"
  mem_pct="$(awk -v u="$mem_used" -v t="$mem_total" 'BEGIN{if(t+0==0)print 0; else printf "%.1f", 100*u/t}')"

  cpu_sum="$(awk -v a="$cpu_sum" -v b="$cpu" 'BEGIN{printf "%.1f", a+b}')"
  mem_sum="$(awk -v a="$mem_sum" -v b="$mem_pct" 'BEGIN{printf "%.1f", a+b}')"
  mem_avail_sum="$(awk -v a="$mem_avail_sum" -v b="$mem_avail" 'BEGIN{printf "%.1f", a+b}')"
  cpu_peak="$(awk -v p="$cpu_peak" -v c="$cpu" 'BEGIN{print (c>p)?c:p}')"
  count=$((count + 1))
  sleep "$INTERVAL"
done

cpu_avg="$(awk -v s="$cpu_sum" -v n="$count" 'BEGIN{printf "%.1f", s/n}')"
mem_avg="$(awk -v s="$mem_sum" -v n="$count" 'BEGIN{printf "%.1f", s/n}')"
mem_avail_avg="$(awk -v s="$mem_avail_sum" -v n="$count" 'BEGIN{printf "%.1f", s/n}')"

mkdir -p "$(dirname "$OUT")"
cat >"$OUT" <<EOF
{
  "role": "${ROLE}",
  "cpu_avg_pct": ${cpu_avg},
  "cpu_peak_pct": ${cpu_peak},
  "mem_used_pct": ${mem_avg},
  "mem_available_mb": ${mem_avail_avg}
}
EOF
echo "Wrote ${OUT}"
