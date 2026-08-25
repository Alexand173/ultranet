#!/usr/bin/env bash
set -Eeuo pipefail

readonly UNIT="ultranet.service"
readonly IDENTIFIER="ultranet-memory-monitor"

log_message() {
  local priority="$1"
  local message="$2"
  systemd-cat --identifier="$IDENTIFIER" --priority="$priority" <<<"$message"
}

state="$(systemctl is-active "$UNIT" 2>/dev/null || true)"
if [[ "$state" != "active" && "$state" != "activating" ]]; then
  log_message warning "unit=$UNIT state=${state:-unknown}"
  exit 0
fi

control_group="$(systemctl show --value --property=ControlGroup "$UNIT")"
cgroup_root="/sys/fs/cgroup${control_group}"
if [[ ! -d "$cgroup_root" ]]; then
  log_message err "unit=$UNIT state=$state cgroup=${control_group:-unknown} unavailable"
  exit 0
fi

read_cgroup_value() {
  local file_name="$1"
  if [[ -r "$cgroup_root/$file_name" ]]; then
    tr -d '\n' <"$cgroup_root/$file_name"
  else
    printf 'unknown'
  fi
}

read_event_counter() {
  local event_name="$1"
  local events_file="$cgroup_root/memory.events"
  if [[ -r "$events_file" ]]; then
    awk -v name="$event_name" '$1 == name { print $2; found = 1 } END { if (!found) print 0 }' "$events_file"
  else
    printf 'unknown'
  fi
}

memory_current="$(read_cgroup_value memory.current)"
memory_peak="$(read_cgroup_value memory.peak)"
swap_current="$(read_cgroup_value memory.swap.current)"
memory_high="$(systemctl show --value --property=MemoryHigh "$UNIT")"
memory_max="$(systemctl show --value --property=MemoryMax "$UNIT")"
events_high="$(read_event_counter high)"
events_oom="$(read_event_counter oom)"
events_oom_kill="$(read_event_counter oom_kill)"

priority=info
if [[ "$memory_current" =~ ^[0-9]+$ && "$memory_high" =~ ^[0-9]+$ ]] && (( memory_current >= memory_high )); then
  priority=warning
fi
if [[ "$events_oom" =~ ^[0-9]+$ && "$events_oom_kill" =~ ^[0-9]+$ ]] && (( events_oom > 0 || events_oom_kill > 0 )); then
  priority=err
fi

log_message "$priority" \
  "unit=$UNIT state=$state memory_current_bytes=$memory_current memory_peak_bytes=$memory_peak memory_swap_current_bytes=$swap_current memory_high_bytes=$memory_high memory_max_bytes=$memory_max events_high=$events_high events_oom=$events_oom events_oom_kill=$events_oom_kill"
