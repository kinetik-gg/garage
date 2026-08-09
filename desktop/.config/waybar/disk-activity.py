#!/usr/bin/env python3
"""Stream NVMe throughput as Waybar JSON without a monitoring daemon."""

import json
import math
import os
import subprocess
import time
from collections import deque
from pathlib import Path

def detect_block_device():
    override = os.environ.get("WAYBAR_DISK_DEVICE")
    if override:
        return override.removeprefix("/dev/")

    try:
        source = subprocess.run(
            ["findmnt", "--noheadings", "--output", "SOURCE", "/"],
            check=True,
            capture_output=True,
            text=True,
            timeout=2,
        ).stdout.strip().split("[", 1)[0]
        name = Path(source).name
        sys_path = Path("/sys/class/block") / name
        if (sys_path / "partition").exists():
            return sys_path.resolve().parent.name
        if sys_path.exists():
            return name
    except (OSError, subprocess.SubprocessError):
        pass

    candidates = sorted(Path("/sys/class/block").glob("nvme*n*"))
    disks = [path.name for path in candidates if not (path / "partition").exists()]
    if not disks:
        raise OSError("no NVMe block device found")
    return disks[0]


DEVICE = detect_block_device()
STAT = Path(f"/sys/class/block/{DEVICE}/stat")
SECTOR_SIZE = int(Path(f"/sys/class/block/{DEVICE}/queue/hw_sector_size").read_text())
BLOCKS = "▁▂▃▄▅▆▇█"
history = deque([BLOCKS[0]] * 4, maxlen=4)


def counters():
    fields = [int(value) for value in STAT.read_text().split()]
    return fields[2], fields[6]


def block_for(rate_mib):
    level = min(7, round(math.log1p(rate_mib) / math.log1p(2048) * 7))
    return BLOCKS[level]


previous_read, previous_write = counters()
previous_time = time.monotonic()

while True:
    time.sleep(2)
    current_read, current_write = counters()
    current_time = time.monotonic()
    elapsed = current_time - previous_time

    read_mib = max(0, current_read - previous_read) * SECTOR_SIZE / elapsed / 1048576
    write_mib = max(0, current_write - previous_write) * SECTOR_SIZE / elapsed / 1048576
    total_mib = read_mib + write_mib
    history.append(block_for(total_mib))

    print(json.dumps({
        "text": f"SSD {''.join(str(value) for value in history)}",
        "tooltip": (
            f"{DEVICE}\n"
            f"↓ Read   {read_mib:,.1f} MiB/s\n"
            f"↑ Write  {write_mib:,.1f} MiB/s\n"
            f"Total    {total_mib:,.1f} MiB/s"
        ),
        "class": "active" if total_mib >= 1 else "idle",
    }), flush=True)

    previous_read, previous_write = current_read, current_write
    previous_time = current_time
