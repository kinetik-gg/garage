#!/usr/bin/env python3
"""Stream CPU, memory, or network activity as Waybar JSON."""

import json
import sys
import time
from collections import deque
from pathlib import Path

BLOCKS = "▁▂▃▄▅▆▇█"
mode = sys.argv[1]
history = deque([BLOCKS[0]] * 4, maxlen=4)


def emit(text, tooltip, active):
    print(json.dumps({
        "text": text,
        "tooltip": tooltip,
        "class": "active" if active else "idle",
    }), flush=True)


def linear_block(percent):
    return BLOCKS[min(7, max(0, round(percent / 100 * 7)))]


def cpu_counters():
    values = [int(value) for value in Path("/proc/stat").read_text().splitlines()[0].split()[1:]]
    idle = values[3] + values[4]
    return sum(values), idle


def memory_values():
    values = {}
    for line in Path("/proc/meminfo").read_text().splitlines():
        key, value = line.split(":", 1)
        values[key] = int(value.split()[0])
    total = values["MemTotal"]
    available = values["MemAvailable"]
    used = total - available
    return used, total, used / total * 100


def default_interface():
    for line in Path("/proc/net/route").read_text().splitlines()[1:]:
        fields = line.split()
        if fields[1] == "00000000" and int(fields[3], 16) & 2:
            return fields[0]
    return ""


def network_counters(interface):
    root = Path(f"/sys/class/net/{interface}/statistics")
    return int((root / "rx_bytes").read_text()), int((root / "tx_bytes").read_text())


if mode == "cpu":
    previous_total, previous_idle = cpu_counters()
    while True:
        time.sleep(2)
        total, idle = cpu_counters()
        total_delta = total - previous_total
        idle_delta = idle - previous_idle
        percent = 100 * (1 - idle_delta / total_delta) if total_delta else 0
        history.append(linear_block(percent))
        emit(f"CPU {''.join(history)}", f"CPU usage  {percent:.1f}%", percent >= 25)
        previous_total, previous_idle = total, idle

elif mode == "memory":
    while True:
        used, total, percent = memory_values()
        history.append(linear_block(percent))
        emit(
            f"MEM {''.join(history)}",
            f"Memory  {used / 1048576:.1f} / {total / 1048576:.1f} GiB\nUsed    {percent:.1f}%",
            percent >= 70,
        )
        time.sleep(2)

elif mode == "network":
    interface = sys.argv[2] if len(sys.argv) > 2 else default_interface()
    if not interface:
        emit("net offline", "No default network route", False)
        raise SystemExit(0)
    previous_rx, previous_tx = network_counters(interface)
    previous_time = time.monotonic()
    while True:
        time.sleep(2)
        rx, tx = network_counters(interface)
        now = time.monotonic()
        elapsed = now - previous_time
        down_mib = max(0, rx - previous_rx) / elapsed / 1048576
        up_mib = max(0, tx - previous_tx) / elapsed / 1048576
        total_mib = down_mib + up_mib
        def compact(rate):
            return f"{rate:.1f}M" if rate >= 1 else f"{rate * 1024:.0f}K"

        emit(
            f"↓{compact(down_mib)} ↑{compact(up_mib)}",
            f"{interface}\n↓ Receive  {down_mib:,.2f} MiB/s\n↑ Send     {up_mib:,.2f} MiB/s",
            total_mib >= 0.1,
        )
        previous_rx, previous_tx, previous_time = rx, tx, now

else:
    raise SystemExit(f"unknown module: {mode}")
