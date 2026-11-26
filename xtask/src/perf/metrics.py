"""Linux process and DRM samples and the independent resource gate."""
from pathlib import Path
import os
import time


PAGE = os.sysconf("SC_PAGE_SIZE")
METRICS = {
    # The process scheduler clock reports ns and includes exited threads.
    "cpu_ns": 1,
    "steady_rss_bytes": PAGE,
    "peak_rss_bytes": PAGE,
    # DRM reports ns, but the driver API does not promise ns precision.
    "gpu_ns": None,
}
# Below this, scheduler noise exceeds any ratio limit, so both legs pass as negligible.
CPU_FLOOR_NS = 1_000_000


def process_cpu_ns(pid):
    """Read the kernel's process-wide CPUCLOCK_SCHED clock (see clock_getcpuclockid(3))."""
    return time.clock_gettime_ns((~pid << 3) | 2)


def process_stat(path):
    fields = path.read_text().rsplit(")", 1)[1].split()
    pid = int(path.parent.name)
    return {
        "identity": int(fields[19]),
        "cpu_ns": process_cpu_ns(pid),
        "rss_bytes": int(fields[21]) * PAGE,
    }


def drm_client(body):
    fields = dict(line.split(":", 1) for line in body.splitlines() if ":" in line)
    if "drm-driver" not in fields:
        return None
    driver = fields["drm-driver"].strip()
    device = fields.get("drm-pdev", "").strip()
    client = fields.get("drm-client-id", "").strip()
    if not device or not client:
        raise ValueError("DRM fdinfo has no PCI device or client identity")
    engines = {}
    for name, raw in fields.items():
        if name.startswith("drm-engine-") and not name.startswith("drm-engine-capacity-"):
            value, unit = raw.split()
            if unit != "ns" or not value.isdecimal():
                raise ValueError(f"unsupported DRM engine counter {name}")
            engines[name.removeprefix("drm-engine-")] = int(value)
    return ":".join([driver, device, client]), engines


def sample(pid, child_pid):
    proc = Path("/proc") / str(pid)
    values = process_stat(proc / "stat")
    values["time_ns"] = time.monotonic_ns()
    for row in (proc / "status").read_text().splitlines():
        if row.startswith("VmHWM:"):
            values["peak_rss_bytes"] = int(row.split()[1]) * 1024
            break
    if child_pid is not None:
        try:
            values["child"] = process_stat(Path("/proc") / str(child_pid) / "stat")
        except OSError:
            values["child"] = None
    clients = {}
    nodes = set()
    try:
        for fd in (proc / "fdinfo").iterdir():
            try:
                target = os.readlink(proc / "fd" / fd.name)
                if target.startswith("/dev/dri/renderD"):
                    nodes.add(target)
                record = drm_client(fd.read_text())
            except FileNotFoundError:
                continue
            if record is not None:
                key, engines = record
                current = clients.setdefault(key, {})
                for name, value in engines.items():
                    current[name] = max(current.get(name, 0), value)
        values["drm"] = clients
    except (OSError, ValueError) as error:
        values["drm"] = None
        values["drm_error"] = str(error)
    values["render_nodes"] = sorted(nodes)
    return values


def gpu_delta(samples, from_launch):
    """Accept only continuous, comparable, busy-time DRM counters."""
    observations = [sample["drm"] for sample in samples]
    if any(value is None for value in observations):
        return None, None
    first = next((i for i, value in enumerate(observations) if value), len(observations))
    if first == len(observations) or (first != 0 and not from_launch):
        return None, None
    clients = observations[first].keys()
    if any(value.keys() != clients for value in observations[first + 1:]):
        return None, None
    # Drivers omit an engine until the client first uses it, so a missing engine is 0 ns.
    names = {client: set().union(*(value[client] for value in observations[first:]))
             for client in clients}
    if not any(names.values()):
        return None, None
    observations[first:] = [
        {client: {name: value[client].get(name, 0) for name in names[client]} for client in clients}
        for value in observations[first:]
    ]
    previous = observations[first]
    maxima = {client: engines.copy() for client, engines in previous.items()}
    for current in observations[first + 1:]:
        for client, engines in current.items():
            for engine, value in engines.items():
                maxima[client][engine] = max(maxima[client][engine], value)
        previous = current
    if previous != maxima:  # Driver has not caught up by the end of the interval.
        return None, None
    deltas = {}
    for client, engines in maxima.items():
        driver_device = client.rsplit(":", 1)[0]
        by_engine = deltas.setdefault(driver_device, {})
        for engine, value in engines.items():
            initial = 0 if from_launch else observations[first][client][engine]
            by_engine[engine] = by_engine.get(engine, 0) + value - initial
    if len(deltas) != 1:
        return None, None
    return sum(next(iter(deltas.values())).values()), deltas


def summarize(samples, from_launch=False, complete=True):
    empty = dict.fromkeys(METRICS)
    if not complete or len(samples) < 2 or len({item["identity"] for item in samples}) != 1:
        return empty, None
    cpu_start = 0 if from_launch else samples[0]["cpu_ns"]
    cpu = samples[-1]["cpu_ns"] - cpu_start
    rss = sorted(item["rss_bytes"] for item in samples if item["rss_bytes"] > 0)
    gpu, engines = gpu_delta(samples, from_launch)
    return {
        "cpu_ns": cpu if cpu >= 0 else None,
        "steady_rss_bytes": rss[len(rss) // 2] if rss else None,
        "peak_rss_bytes": max(rss[-1], samples[-1].get("peak_rss_bytes", 0))
        if rss and from_launch else (rss[-1] if rss else None),
        "gpu_ns": gpu,
    }, engines


def compare(pairs, metric, gpu_resolution=None, limit=(7, 5)):
    numerator, denominator = limit
    resolution = METRICS[metric] if metric != "gpu_ns" else gpu_resolution
    intervals = []
    ratios = []
    for pair in pairs:
        candidate, baseline = pair["candidate"], pair["baseline"]
        if not candidate["valid"] or not baseline["valid"]:
            return {"status": "unverified", "reason": "workload evidence incomplete"}
        a, b = candidate["metrics"][metric], baseline["metrics"][metric]
        if a is None or b is None or resolution is None:
            return {"status": "unverified", "reason": "counter or its resolution unavailable"}
        if metric == "gpu_ns":
            a_engines, b_engines = candidate["engines"], baseline["engines"]
            # Engine lists may differ: an unused engine is not reported.
            if a_engines.keys() != b_engines.keys():
                return {"status": "unverified", "reason": "GPU counter topology differs"}
        if metric == "cpu_ns" and max(a, b) < CPU_FLOOR_NS:
            ratios.append(a / b if b else None)
            intervals.append((0, 0))
            continue
        if resolution > 0 and b <= resolution:
            return {"status": "unverified", "reason": "baseline at or below counter resolution"}
        ratios.append(a / b if b else None)
        # Compare integer bounds to the limit, including the exact boundary.
        intervals.append((
            denominator * max(0, a - resolution) - numerator * (b + resolution),
            denominator * (a + resolution) - numerator * max(0, b - resolution),
        ))
    # Gate on the median pair. For an even count, take the less favourable middle pair.
    status = "unverified"
    lowers = sorted(lower for lower, _ in intervals)
    uppers = sorted(upper for _, upper in intervals)
    if pairs and uppers[len(uppers) // 2] <= 0:
        status = "passed"
    elif pairs and lowers[(len(lowers) - 1) // 2] > 0:
        status = "failed"
    return {"status": status, "ratios": ratios, "resolution": resolution}
