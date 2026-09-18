#!/usr/bin/env python3
"""Run one command and append a complete v3 process/resource measurement record."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import signal
import subprocess
import sys
import time
import uuid

from response_summary import response_failure, summarize_response


HEX64 = set("0123456789abcdef")


def _valid_digest(value: str | None) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(char in HEX64 for char in value)


def _environment_receipt() -> dict[str, object]:
    selected = {key: os.environ.get(key) for key in ("LANG", "LC_ALL", "TZ", "NODE_OPTIONS", "PATH")}
    return {
        "platform": platform.platform(), "machine": platform.machine(),
        "python": sys.version.split()[0], "cwd": str(Path.cwd().resolve()),
        "selectedEnvironment": selected,
    }


def _linux_process_sample(root_pid: int) -> tuple[int, int, int, int]:
    pending = [root_pid]
    seen: set[int] = set()
    rss = read_bytes = write_bytes = faults = 0
    while pending:
        pid = pending.pop()
        if pid in seen:
            continue
        seen.add(pid)
        proc = Path("/proc") / str(pid)
        try:
            for line in (proc / "status").read_text(encoding="utf-8", errors="replace").splitlines():
                if line.startswith("VmRSS:"):
                    rss += int(line.split()[1]) * 1024
            io_values = {
                line.split(":", 1)[0]: int(line.split(":", 1)[1])
                for line in (proc / "io").read_text(encoding="utf-8", errors="replace").splitlines()
                if ":" in line
            }
            read_bytes += io_values.get("read_bytes", 0)
            write_bytes += io_values.get("write_bytes", 0)
            stat = (proc / "stat").read_text(encoding="utf-8", errors="replace").split()
            faults += int(stat[9]) + int(stat[11])
            children = proc / "task" / str(pid) / "children"
            pending.extend(int(value) for value in children.read_text().split())
        except (FileNotFoundError, ProcessLookupError, PermissionError, ValueError, IndexError):
            continue
    return rss, read_bytes, write_bytes, faults


def _darwin_process_sample(root_pid: int) -> tuple[int, int, int, int]:
    result = subprocess.run(["ps", "-axo", "pid=,ppid=,rss="], text=True, capture_output=True, check=False)
    rows: list[tuple[int, int, int]] = []
    for line in result.stdout.splitlines():
        try:
            rows.append(tuple(int(value) for value in line.split()))
        except ValueError:
            continue
    family = {root_pid}
    changed = True
    while changed:
        changed = False
        for pid, ppid, _ in rows:
            if ppid in family and pid not in family:
                family.add(pid); changed = True
    return sum(rss_kib * 1024 for pid, _, rss_kib in rows if pid in family), 0, 0, 0


def _process_sample(root_pid: int) -> tuple[int, int, int, int, str]:
    if sys.platform.startswith("linux") and Path("/proc").is_dir():
        return (*_linux_process_sample(root_pid), "available")
    if sys.platform == "darwin":
        return (*_darwin_process_sample(root_pid), "unsupported")
    return 0, 0, 0, 0, "unsupported"


def _cgroup_v2(pid: int) -> dict[str, object]:
    if not sys.platform.startswith("linux") or not Path("/sys/fs/cgroup/cgroup.controllers").is_file():
        return {"status": "unsupported", "identity": None, "memoryCurrentBytes": None, "memoryPeakBytes": None}
    try:
        unified = next(
            line for line in (Path("/proc") / str(pid) / "cgroup").read_text().splitlines()
            if line.startswith("0::")
        )
        relative = unified.split("::", 1)[1].lstrip("/")
        root = Path("/sys/fs/cgroup") / relative
        peak_path = root / "memory.peak"
        return {
            "status": "shared", "identity": "/" + relative, "isolated": False,
            "memoryCurrentBytes": int((root / "memory.current").read_text().strip()),
            "memoryPeakBytes": int(peak_path.read_text().strip()) if peak_path.is_file() else None,
        }
    except (OSError, StopIteration, ValueError):
        return {"status": "missing", "identity": None, "isolated": False, "memoryCurrentBytes": None, "memoryPeakBytes": None}


def _create_measurement_cgroup(parent: Path) -> Path:
    if not sys.platform.startswith("linux"):
        raise OSError("measurement cgroups are supported only on Linux")
    parent = parent.resolve(strict=True)
    if not (parent / "cgroup.controllers").is_file():
        raise OSError(f"not a cgroup v2 parent: {parent}")
    group = parent / f"octocode-v3-{os.getpid()}-{uuid.uuid4().hex}"
    group.mkdir(mode=0o700)
    return group


def _read_cgroup(group: Path) -> tuple[dict[str, object], dict[str, object], dict[str, object]]:
    events = {
        key: int(value) for key, value in
        (line.split() for line in (group / "cgroup.events").read_text().splitlines())
    }
    populated = bool(events.get("populated", 0))
    memory = {
        "status": "available", "identity": str(group), "isolated": True,
        "memoryCurrentBytes": int((group / "memory.current").read_text().strip()),
        "memoryPeakBytes": int((group / "memory.peak").read_text().strip()), "populated": populated,
    }
    read_bytes = write_bytes = 0
    for line in (group / "io.stat").read_text().splitlines():
        for field in line.split()[1:]:
            key, _, value = field.partition("=")
            if key == "rbytes": read_bytes += int(value)
            elif key == "wbytes": write_bytes += int(value)
    io_record = {
        "status": "available", "read_bytes": read_bytes, "write_bytes": write_bytes,
        "complete_process_tree": not populated, "sensor": "isolated-cgroup-v2",
    }
    cpu_values = {
        key: int(value) for key, value in
        (line.split() for line in (group / "cpu.stat").read_text().splitlines())
    }
    cpu_record = {
        "user_ms": round(cpu_values.get("user_usec", 0) / 1000, 3),
        "system_ms": round(cpu_values.get("system_usec", 0) / 1000, 3),
        "usage_ms": round(cpu_values.get("usage_usec", 0) / 1000, 3),
        "sensor": "isolated-cgroup-v2", "complete_process_tree": not populated,
    }
    return memory, io_record, cpu_record


def _infer_result_count(raw: bytes) -> int | None:
    return summarize_response(raw)["returned_result_count"]


def _classify_outcome(
    exit_code: int | None, result_count: int | None, empty_classification: str | None,
    *, attempt_index: int = 1, raw: bytes = b"",
) -> str:
    if exit_code != 0:
        lowered = raw.decode("utf-8", errors="replace").lower()
        if any(marker in lowered for marker in ("schema", "unknown field", "invalid_type", "validation error")):
            return "schema-invalid"
        return "runtime-failed"
    failure = response_failure(summarize_response(raw))
    if failure:
        return failure
    if result_count is None:
        return "runtime-failed"
    if result_count == 0:
        return "expected-empty" if empty_classification == "expected-absence" else "unproductive-empty"
    return "first-valid" if attempt_index == 1 else "productive-success"


def _append_json(path: Path, record: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = (json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(descriptor, payload); os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _validate_v3(record: dict[str, object], strict: bool, *, platform_name: str | None = None) -> list[str]:
    if not strict:
        return []
    errors: list[str] = []
    platform_name = platform_name or sys.platform
    if not platform_name.startswith("linux"):
        errors.append("strict v3 publication measurement requires Linux cgroup v2")
    for key in ("corpus_digest", "workspace_receipt_digest", "fixture_manifest_digest", "contracts_digest"):
        if not _valid_digest(record.get(key)):
            errors.append(f"missing or invalid {key}")
    memory = record.get("memory", {})
    if int(memory.get("peak_process_tree_rss_bytes", 0)) <= 0:
        errors.append("missing peak process-tree RSS sensor")
    if platform_name.startswith("linux") and not memory.get("cgroup_v2", {}).get("isolated"):
        errors.append("missing isolated Linux cgroup v2 memory sensor")
    if platform_name.startswith("linux") and memory.get("cgroup_v2", {}).get("populated"):
        errors.append("isolated Linux cgroup still populated after command exit")
    if platform_name.startswith("linux") and memory.get("cgroup_v2", {}).get("cleanupError"):
        errors.append("isolated Linux cgroup cleanup failed")
    cpu = record.get("cpu", {})
    if not all(isinstance(cpu.get(key), (int, float)) for key in ("user_ms", "system_ms")):
        errors.append("missing CPU sensor")
    if platform_name.startswith("linux") and not cpu.get("complete_process_tree"):
        errors.append("missing complete process-tree CPU sensor")
    io_record = record.get("io", {})
    if platform_name.startswith("linux") and not io_record.get("complete_process_tree"):
        errors.append("missing complete process-tree I/O sensor")
    if not record.get("logical_call_id") or int(record.get("attempt_index", 0)) < 1:
        errors.append("missing logical call identity")
    result_count = record.get("result_count")
    if record.get("exit_code") == 0 and (
        type(result_count) is not int or result_count < 0
    ):
        errors.append("successful call has no non-negative integer result count")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log", required=True)
    parser.add_argument("--artifact-dir", required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--cache-cohort", choices=("cold", "warm-process", "warm-index"), default="cold")
    parser.add_argument("--corpus-digest")
    parser.add_argument("--workspace-receipt-digest")
    parser.add_argument("--fixture-manifest-digest")
    parser.add_argument("--contracts-digest")
    parser.add_argument("--tool-receipt-digest")
    parser.add_argument("--timeout-seconds", type=float)
    parser.add_argument("--sample-interval-ms", type=float, default=5.0)
    parser.add_argument("--attempt-index", type=int, default=1)
    parser.add_argument("--logical-call-id")
    parser.add_argument("--call-outcome", choices=("first-valid", "schema-invalid", "runtime-failed", "productive-success", "expected-empty", "unproductive-empty"))
    parser.add_argument("--empty-classification", choices=("expected-absence", "scope-empty", "provider-incomplete", "query-miss"))
    parser.add_argument("--result-count", type=int)
    parser.add_argument("--cgroup-parent", type=Path)
    parser.add_argument("--strict-v3", action="store_true")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a command is required after --")

    command_text = " ".join(command)
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    environment = _environment_receipt()
    artifact_dir = Path(args.artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    artifact = artifact_dir / f"{time.time_ns()}-{os.getpid()}-command.bin"
    started_ns = time.monotonic_ns()
    artifact_handle = artifact.open("xb")
    isolated_group: Path | None = None
    launch_command = command
    gate_write: int | None = None
    pass_fds: tuple[int, ...] = ()
    if args.cgroup_parent:
        isolated_group = _create_measurement_cgroup(args.cgroup_parent)
        gate_read, gate_write = os.pipe()
        os.set_inheritable(gate_read, True)
        launch_command = [sys.executable, str(Path(__file__).resolve()), "--_gated-exec-fd", str(gate_read), "--", *command]
        pass_fds = (gate_read,)
    process = subprocess.Popen(launch_command, stdout=artifact_handle, stderr=subprocess.STDOUT, start_new_session=True, pass_fds=pass_fds)
    if isolated_group is not None:
        os.close(gate_read)
        (isolated_group / "cgroup.procs").write_text(str(process.pid))
        os.write(gate_write, b"1"); os.close(gate_write)
    peak_rss = max_read = max_write = max_faults = 0
    process_sensor = "unsupported"
    cgroup = _cgroup_v2(process.pid)
    timed_out = False
    deadline = time.monotonic() + args.timeout_seconds if args.timeout_seconds else None
    while process.poll() is None:
        rss, read_bytes, write_bytes, faults, status = _process_sample(process.pid)
        peak_rss = max(peak_rss, rss); max_read = max(max_read, read_bytes)
        max_write = max(max_write, write_bytes); max_faults = max(max_faults, faults)
        process_sensor = status
        if deadline is not None and time.monotonic() >= deadline:
            timed_out = True
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=0.25)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            break
        time.sleep(max(args.sample_interval_ms, 1.0) / 1000)
    process.wait()
    artifact_handle.close()
    raw = artifact.read_bytes()
    elapsed_ms = round((time.monotonic_ns() - started_ns) / 1_000_000, 3)
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    user_ms = round(max(0.0, usage_after.ru_utime - usage_before.ru_utime) * 1000, 3)
    system_ms = round(max(0.0, usage_after.ru_stime - usage_before.ru_stime) * 1000, 3)
    usage_peak = int(usage_after.ru_maxrss * (1 if sys.platform == "darwin" else 1024))
    if peak_rss == 0:
        # RUSAGE_CHILDREN is cumulative on several platforms; use it only when a
        # very short-lived process escaped the process-tree sampler.
        peak_rss = usage_peak
    io_record = {
        "status": "sampled-live-incomplete" if sys.platform.startswith("linux") else "unsupported",
        "read_bytes": max_read, "write_bytes": max_write, "complete_process_tree": False,
        "sensor": "live-proc-sampling" if sys.platform.startswith("linux") else None,
    }
    cpu_record = {"user_ms": user_ms, "system_ms": system_ms, "sensor": "rusage-children", "complete_process_tree": False}
    if isolated_group is not None:
        cgroup, io_record, cpu_record = _read_cgroup(isolated_group)
        if cgroup.get("populated") and (isolated_group / "cgroup.kill").is_file():
            (isolated_group / "cgroup.kill").write_text("1")
            for _ in range(50):
                if "populated 0" in (isolated_group / "cgroup.events").read_text():
                    break
                time.sleep(0.01)
        try:
            isolated_group.rmdir()
        except OSError as exc:
            cgroup["cleanupError"] = str(exc)

    text = raw.decode("utf-8", errors="replace")
    signal_number = -process.returncode if process.returncode is not None and process.returncode < 0 else None
    response_summary = summarize_response(raw)
    result_count = args.result_count if args.result_count is not None else response_summary["returned_result_count"]
    inferred_outcome = args.call_outcome or _classify_outcome(
        process.returncode, result_count, args.empty_classification,
        attempt_index=args.attempt_index, raw=raw,
    )
    record: dict[str, object] = {
        "measurement_schema_version": 3, "cmd": args.label, "argv": command,
        "char_unit": "unicode_code_points", "bytes": len(raw), "stdout_stderr_bytes": len(raw),
        "model_out_chars": len(command_text), "model_in_chars": len(text),
        "total_chars": len(command_text) + len(text), "sha256": hashlib.sha256(raw).hexdigest(),
        "exit_code": process.returncode, "signal": signal_number, "timed_out": timed_out,
        "elapsed_ms": elapsed_ms, "wall_time_ms": elapsed_ms,
        "cpu": cpu_record,
        "memory": {"peak_process_tree_rss_bytes": peak_rss, "process_tree_sensor": process_sensor, "cgroup_v2": cgroup},
        "io": io_record,
        "page_faults": {"sampled_process_tree": max_faults, "minor": max(0, usage_after.ru_minflt - usage_before.ru_minflt), "major": max(0, usage_after.ru_majflt - usage_before.ru_majflt)},
        "cache_cohort": args.cache_cohort, "corpus_digest": args.corpus_digest,
        "workspace_receipt_digest": args.workspace_receipt_digest, "tool_receipt_digest": args.tool_receipt_digest,
        "fixture_manifest_digest": args.fixture_manifest_digest, "contracts_digest": args.contracts_digest,
        "environment_receipt_digest": hashlib.sha256(json.dumps(environment, sort_keys=True).encode()).hexdigest(),
        "logical_call_id": args.logical_call_id or args.label,
        "attempt_index": args.attempt_index, "call_outcome": inferred_outcome,
        "empty_classification": args.empty_classification, "result_count": result_count, "artifact": str(artifact),
        "response_summary": response_summary,
    }
    record["sensor_validation_errors"] = _validate_v3(record, args.strict_v3)
    _append_json(Path(args.log), record)
    sys.stdout.write(text)
    if record["sensor_validation_errors"]:
        return 125
    if timed_out:
        return 124
    return int(process.returncode or 0)


if __name__ == "__main__":
    if len(sys.argv) > 3 and sys.argv[1] == "--_gated-exec-fd":
        descriptor = int(sys.argv[2]); separator = sys.argv.index("--")
        os.read(descriptor, 1); os.close(descriptor)
        os.execvp(sys.argv[separator + 1], sys.argv[separator + 1:])
    raise SystemExit(main())
