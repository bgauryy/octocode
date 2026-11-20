"""Use when finding which agent CLI runs this process and which agent CLIs run on this machine."""
import os
from pathlib import Path
import re
import shutil
import subprocess

AGENTS = ("claude", "codex", "cursor", "octocode", "grok", "pi")
# Octocode coding agents title their process `octocode-<role>-<id>`; a path (the research CLI) never matches.
OCTOCODE_TITLE = re.compile(r"^octocode-(?:[A-Za-z0-9._]+-[A-Za-z0-9._-]+|main|rpc)(?:\s|$)")
EXECUTABLES = {"claude": "claude", "codex": "codex", "cursor-agent": "cursor", "grok": "grok", "pi": "pi"}
RUNTIMES = {"node", "bun", "deno"}
# Script or install paths that identify an agent run by a JavaScript runtime or a versioned binary.
PATH_MARKERS = {"@anthropic-ai/claude-code/": "claude", "@openai/codex/": "codex", "/cursor-agent/": "cursor",
                "pi-coding-agent/": "pi", "/.grok/": "grok"}
# Processes named like an agent that host no agent session.
HELPER_ARGS = ("--chrome-native-host", "mcp-server", "exec-server")


def process_table():
    """pid -> (ppid, executable, command line), read from ps; empty where ps is unavailable."""
    def column(field):
        try:
            output = subprocess.run(["ps", "-A", "-ww", "-o", f"pid=,ppid=,{field}="],
                                    capture_output=True, text=True, check=False).stdout
        except OSError:
            return {}
        rows = {}
        for line in output.splitlines():
            parts = line.split(None, 2)
            if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
                rows[int(parts[0])] = (int(parts[1]), parts[2].strip() if len(parts) == 3 else "")
        return rows

    executables, commands = column("comm"), column("args")
    return {pid: (ppid, executable, commands.get(pid, (ppid, ""))[1])
            for pid, (ppid, executable) in executables.items()}


def agent_of(executable, command):
    """The agent vendor a process runs, or None."""
    if OCTOCODE_TITLE.match(command) or OCTOCODE_TITLE.match(executable):
        return "octocode"
    if any(arg in HELPER_ARGS for arg in command.split()):
        return None
    name = os.path.basename(executable)
    if name in EXECUTABLES:
        return EXECUTABLES[name]
    searched = executable if name not in RUNTIMES else f"{executable} {command}"
    for marker, vendor in PATH_MARKERS.items():
        if marker in searched:
            return vendor
    return None


def operator(table, pid=None):
    """The nearest agent among this process's ancestors; a Cursor editor ancestor counts only when none is found."""
    pid = os.getpid() if pid is None else pid
    seen, editor = set(), None
    while pid in table and pid not in seen:
        seen.add(pid)
        ppid, executable, command = table[pid]
        vendor = agent_of(executable, command)
        if vendor:
            return {"vendor": vendor, "pid": pid, "via": "process"}
        if editor is None and "/Cursor.app/" in executable:
            editor = {"vendor": "cursor", "pid": pid, "via": "editor"}
        pid = ppid
    return editor or {"vendor": None, "pid": None, "via": "none"}


def working_directories(pids):
    """pid -> current directory, where the operating system reveals it."""
    if not pids:
        return {}
    if shutil.which("lsof"):
        output = subprocess.run(["lsof", "-a", "-d", "cwd", "-Fpn", "-p", ",".join(map(str, pids))],
                                capture_output=True, text=True, check=False).stdout
        found, current = {}, None
        for line in output.splitlines():
            if line.startswith("p") and line[1:].isdigit():
                current = int(line[1:])
            elif line.startswith("n") and current is not None:
                found[current] = line[1:]
        return found
    found = {}
    for pid in pids:
        try:
            found[pid] = os.readlink(f"/proc/{pid}/cwd")
        except OSError:
            continue
    return found


def running(table, workspace, self_pid=None):
    """Top-level agent sessions by process: a wrapper and the binary it starts count once."""
    sessions = {}
    for pid, (ppid, executable, command) in table.items():
        vendor = agent_of(executable, command)
        parent = table.get(ppid)
        if vendor and not (parent and agent_of(parent[1], parent[2]) == vendor):
            sessions[pid] = vendor
    directories = working_directories(sorted(sessions))
    root = Path(workspace).resolve()
    rows = []
    for pid, vendor in sorted(sessions.items(), key=lambda item: (item[1], item[0])):
        cwd = directories.get(pid)
        rows.append({"vendor": vendor, "pid": pid, "cwd": cwd, "self": pid == self_pid,
                     "inWorkspace": None if cwd is None else Path(cwd).resolve().is_relative_to(root)})
    return rows
