#!/usr/bin/env python3
"""Render compact SVG time-series graphs for Waybar's image module."""

import fcntl
import html
import json
import math
import os
import subprocess
import sys
import time
from pathlib import Path

# These scripts emit their own markup, so the bar's stylesheet cannot reach the
# colours; they resolve the current appearance themselves.
def _theme_fg() -> str:
    # The bar publishes the colour it picked from the wallpaper. Deriving one
    # from the theme instead is what made these widgets disagree with every
    # other module in the bar.
    try:
        return (Path.home() / ".local/state/garage/generated/bar-foreground").read_text().strip()
    except OSError:
        return "#f5f5f7"


THEME_FG = _theme_fg()



POINTS = 30
MIN_SAMPLE_INTERVAL = 1.5
CACHE_DIR = Path.home() / ".cache" / "waybar-activity-graphs"

LAYOUTS = {
    "gpu": {"label": "GPU", "width": 170, "graph_x": 31, "graph_width": 48, "value_x": 85, "extra_x": 116},
    "cpu": {"label": "CPU", "width": 112, "graph_x": 30, "graph_width": 44, "value_x": 80},
    "memory": {"label": "MEM", "width": 115, "graph_x": 32, "graph_width": 44, "value_x": 82},
    "disk": {"label": "SSD", "width": 124, "graph_x": 30, "graph_width": 44, "value_x": 80},
}


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


def atomic_write(path, content):
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(content)
    os.replace(temporary, path)


def load_state(path):
    try:
        return json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return {}


def cpu_counters():
    fields = [int(value) for value in Path("/proc/stat").read_text().splitlines()[0].split()[1:]]
    return sum(fields), fields[3] + fields[4]


def memory_values():
    values = {}
    for line in Path("/proc/meminfo").read_text().splitlines():
        key, value = line.split(":", 1)
        values[key] = int(value.split()[0])
    total = values["MemTotal"]
    used = total - values["MemAvailable"]
    return used, total, used / total * 100


def gpu_values():
    output = subprocess.run(
        [
            "nvidia-smi",
            "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu",
            "--format=csv,noheader,nounits",
        ],
        check=True,
        capture_output=True,
        text=True,
        timeout=2,
    ).stdout.splitlines()[0]
    utilization, used_mib, total_mib, temperature = [part.strip() for part in output.split(",")]
    return int(utilization), int(used_mib), int(total_mib), int(temperature)


def disk_counters():
    fields = [int(value) for value in Path(f"/sys/class/block/{DEVICE}/stat").read_text().split()]
    return fields[2], fields[6]


def disk_temperature():
    sensors = sorted(Path(f"/sys/class/block/{DEVICE}/device").glob("hwmon*/temp1_input"))
    if not sensors:
        raise OSError(f"no temperature sensor found for {DEVICE}")
    return int(sensors[0].read_text()) / 1000


def compact_rate(rate_mib):
    if rate_mib >= 1024:
        return f"{rate_mib / 1024:.1f}G"
    if rate_mib >= 1:
        return f"{rate_mib:.1f}M"
    return f"{rate_mib * 1024:.0f}K"


def sample(metric, state, now):
    if metric == "gpu":
        utilization, used_mib, total_mib, temperature = gpu_values()
        used_gib = used_mib / 1024
        total_gib = total_mib / 1024
        return {
            "value": utilization,
            "display": f"{utilization}%",
            "extra": f"{used_gib:.1f}G {temperature}°",
            "tooltip": (
                f"GPU {utilization}% | VRAM {used_gib:.1f} / {total_gib:.1f} GiB | {temperature}°C | last 60s"
            ),
            "active": utilization >= 10,
        }

    if metric == "cpu":
        total, idle = cpu_counters()
        previous = state.get("counters")
        percent = 0.0
        if isinstance(previous, list) and len(previous) == 2:
            total_delta = total - previous[0]
            idle_delta = idle - previous[1]
            if total_delta > 0:
                percent = 100 * (1 - idle_delta / total_delta)
        return {
            "value": percent,
            "display": f"{percent:.0f}%",
            "tooltip": f"CPU usage {percent:.1f}% | last 60s",
            "active": percent >= 25,
            "counters": [total, idle],
        }

    if metric == "memory":
        used, total, percent = memory_values()
        return {
            "value": percent,
            "display": f"{percent:.0f}%",
            "tooltip": f"Memory {used / 1048576:.1f} / {total / 1048576:.1f} GiB ({percent:.1f}%) | last 60s",
            "active": percent >= 70,
        }

    read_sectors, write_sectors = disk_counters()
    temperature = disk_temperature()
    previous = state.get("counters")
    read_mib = 0.0
    write_mib = 0.0
    if isinstance(previous, list) and len(previous) == 3:
        elapsed = now - previous[2]
        if elapsed > 0:
            sector_size = int(Path(f"/sys/class/block/{DEVICE}/queue/hw_sector_size").read_text())
            read_mib = max(0, read_sectors - previous[0]) * sector_size / elapsed / 1048576
            write_mib = max(0, write_sectors - previous[1]) * sector_size / elapsed / 1048576
    total_mib = read_mib + write_mib
    graph_value = min(100, math.log1p(total_mib) / math.log1p(2048) * 100)
    return {
        "value": graph_value,
        "display": f"{temperature:.0f}°",
        "tooltip": (
            f"{DEVICE} | {temperature:.1f}°C | read {read_mib:,.1f} MiB/s | write {write_mib:,.1f} MiB/s "
            f"| total {compact_rate(total_mib)}/s | last 60s (log scale)"
        ),
        "active": total_mib >= 1,
        "counters": [read_sectors, write_sectors, now],
    }


def update_state(metric, state, now):
    if now - state.get("last_sample", 0) < MIN_SAMPLE_INTERVAL and state.get("history"):
        return state

    current = sample(metric, state, now)
    history = state.get("history", [])
    if not history:
        history = [current["value"]] * POINTS
    else:
        history = (history + [current["value"]])[-POINTS:]

    state.update(current)
    state["history"] = history
    state["last_sample"] = now
    return state


def render_metric(metric, state):
    layout = LAYOUTS[metric]
    graph_x = layout["graph_x"]
    graph_width = layout["graph_width"]
    graph_top = 3
    graph_height = 16
    graph_bottom = graph_top + graph_height
    history = state.get("history") or [0] * POINTS
    if len(history) == 1:
        history *= POINTS

    coordinates = []
    for index, value in enumerate(history):
        x = graph_x + index * graph_width / (len(history) - 1)
        y = graph_bottom - min(100, max(0, value)) / 100 * graph_height
        coordinates.append((x, y))

    points = " ".join(f"{x:.2f},{y:.2f}" for x, y in coordinates)
    area = (
        f"M {coordinates[0][0]:.2f} {graph_bottom} "
        + " ".join(f"L {x:.2f} {y:.2f}" for x, y in coordinates)
        + f" L {coordinates[-1][0]:.2f} {graph_bottom} Z"
    )
    opacity = "0.92" if state.get("active") else "0.62"
    display = html.escape(state.get("display", "--"))
    extra = html.escape(state.get("extra", ""))

    extra_text = ""
    if extra and "extra_x" in layout:
        extra_text = f'<text x="{layout["extra_x"]}" y="15.5" class="secondary">{extra}</text>'

    return f'''<path d="M {graph_x} {graph_top + graph_height / 2:.1f} H {graph_x + graph_width}" stroke="{THEME_FG}" stroke-opacity="0.10" stroke-width="1"/>
  <path d="M {graph_x} {graph_bottom} H {graph_x + graph_width}" stroke="{THEME_FG}" stroke-opacity="0.16" stroke-width="1"/>
  <path d="{area}" fill="{THEME_FG}" fill-opacity="0.08"/>
  <polyline points="{points}" fill="none" stroke="{THEME_FG}" stroke-opacity="{opacity}" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
  <text x="0" y="15.5">{layout["label"]}</text>
  <text x="{layout["value_x"]}" y="15.5">{display}</text>
  {extra_text}'''


def render_svg(metric, state):
    width = LAYOUTS[metric]["width"]
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="22" viewBox="0 0 {width} 22">
  <style>
    text {{ font-family: "Plus Jakarta Sans", sans-serif; font-size: 11.5px; font-weight: 600; fill: {THEME_FG}; fill-opacity: 1.0; }}
    .secondary {{ fill-opacity: 0.75; }}
  </style>
  {render_metric(metric, state)}
</svg>
'''


def render_all_svg(states):
    gap = 16
    width = sum(layout["width"] for layout in LAYOUTS.values()) + gap * (len(LAYOUTS) - 1)
    groups = []
    offset = 0
    for metric, layout in LAYOUTS.items():
        groups.append(f'<g transform="translate({offset} 0)">{render_metric(metric, states[metric])}</g>')
        offset += layout["width"] + gap
    body = "\n  ".join(groups)
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="22" viewBox="0 0 {width} 22">
  <style>
    text {{ font-family: "Plus Jakarta Sans", sans-serif; font-size: 11.5px; font-weight: 600; fill: {THEME_FG}; fill-opacity: 1.0; }}
    .secondary {{ fill-opacity: 0.75; }}
  </style>
  {body}
</svg>
'''


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in (*LAYOUTS, "all"):
        raise SystemExit("usage: activity-graph.py all|gpu|cpu|memory|disk")

    metric = sys.argv[1]
    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    if metric == "all":
        svg_path = CACHE_DIR / "activity.svg"
        lock_path = CACHE_DIR / "activity.lock"
        states = {}
        with lock_path.open("a+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            for name in LAYOUTS:
                state_path = CACHE_DIR / f"{name}.json"
                state = load_state(state_path)
                try:
                    state = update_state(name, state, time.time())
                except (OSError, ValueError, subprocess.SubprocessError) as error:
                    state.update({
                        "display": "--",
                        "extra": "",
                        "tooltip": f"{name.upper()} unavailable: {error}",
                        "active": False,
                    })
                    state.setdefault("history", [0] * POINTS)
                atomic_write(state_path, json.dumps(state))
                states[name] = state
            atomic_write(svg_path, render_all_svg(states))

        print(svg_path)
        print(" • ".join(state.get("tooltip", name.upper()) for name, state in states.items()))
        return

    state_path = CACHE_DIR / f"{metric}.json"
    svg_path = CACHE_DIR / f"{metric}.svg"
    lock_path = CACHE_DIR / f"{metric}.lock"

    with lock_path.open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        state = load_state(state_path)
        try:
            state = update_state(metric, state, time.time())
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            state.update({"display": "--", "extra": "", "tooltip": f"{metric.upper()} unavailable: {error}", "active": False})
            state.setdefault("history", [0] * POINTS)
        atomic_write(state_path, json.dumps(state))
        atomic_write(svg_path, render_svg(metric, state))

    print(svg_path)
    print(state.get("tooltip", metric.upper()))


if __name__ == "__main__":
    main()
