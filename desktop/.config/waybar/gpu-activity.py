#!/usr/bin/env python3
"""Stream compact NVIDIA activity as Waybar JSON."""

import json
import signal
import subprocess
import time
from collections import deque

BLOCKS = "▁▂▃▄▅▆▇█"
signal.signal(signal.SIGPIPE, signal.SIG_DFL)
history = deque([BLOCKS[0]] * 4, maxlen=4)


def run(command):
    return subprocess.run(
        command,
        check=True,
        capture_output=True,
        text=True,
        timeout=2,
    ).stdout.strip()


def gpu_values():
    output = run([
        "nvidia-smi",
        "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu",
        "--format=csv,noheader,nounits",
    ])
    utilization, used_mib, total_mib, temperature = output.splitlines()[0].split(", ")
    return int(utilization), int(used_mib), int(total_mib), int(temperature)


while True:
    try:
        gpu, used_mib, total_mib, temperature = gpu_values()
        history.append(BLOCKS[min(7, max(0, round(gpu / 100 * 7)))])

        used_gib = used_mib / 1024
        total_gib = total_mib / 1024
        active = gpu >= 10

        print(json.dumps({
            "text": (
                f"GPU {''.join(history)}  "
                f"{used_gib:.1f}G  {temperature}°"
            ),
            "tooltip": (
                f"NVIDIA GPU       {gpu}%\n"
                f"VRAM             {used_gib:.1f} / {total_gib:.1f} GiB\n"
                f"Temperature      {temperature}°C"
            ),
            "class": "active" if active else "idle",
        }), flush=True)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(json.dumps({
            "text": "GPU unavailable",
            "tooltip": str(error),
            "class": "error",
        }), flush=True)

    time.sleep(2)
