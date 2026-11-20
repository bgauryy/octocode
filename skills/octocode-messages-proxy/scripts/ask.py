"""Use when sending one message to a live Octocode peer through a small proxy and returning its reply in one call."""
import json
import os
import queue
import signal
import subprocess
import sys
import tempfile
import threading
import time
from uuid import uuid4

# Presence a joined operator keeps between renewals (the store's maximum is 10 minutes).
PRESENCE_MS = 600_000
WAIT_SLICE_MS = 15_000
STOP_GRACE_SECONDS = 5


def fail(message):
    raise SystemExit(message)


class Comm:
    """The octocode-agents-communication CLI, bound to one workspace and store."""

    def __init__(self, base, workspace, database=None):
        self.base, self.workspace, self.database = base, workspace, database

    def call(self, argv, payload=None, session=None):
        command = [*self.base, *argv]
        if payload is not None:
            command.append("-")
        if session:
            command += ["--session", session]
        command.append("--json")
        if self.database:
            command += ["--database", self.database]
        result = subprocess.run(command, input=None if payload is None else json.dumps(payload),
                                capture_output=True, text=True, cwd=self.workspace)
        if result.returncode:
            fail(f"{' '.join(argv)}: {result.stderr.strip() or result.stdout.strip() or result.returncode}")
        return json.loads(result.stdout)

    def items(self, command, session, payload=None):
        page_input = dict(payload or {})
        while True:
            page = self.call([command], page_input, session)
            yield from page.get("items", [])
            if "next" not in page:
                return
            page_input = page["next"]["input"]


def preflight(comm):
    """Refuse an installed package whose messages predate the parts schema this skill sends."""
    schema = comm.call(["schema", "send_message"]).get("inputSchema", {})
    if "parts" not in schema.get("required", []):
        fail("The installed octocode-agents-communication predates the parts message schema this skill sends "
             f"({' '.join(comm.base)}). Put a current build first on PATH as octocode-agents-communication.")


def resolve_worker(comm, operator_id, target):
    matches = {peer["id"]: peer for peer in comm.items("peers", operator_id)
               if target in (peer["id"], peer.get("name"), peer.get("vendorSession")) and peer["id"] != operator_id}
    if len(matches) != 1:
        fail(f"Expected one live peer named or identified {target!r}, found {len(matches)}. Run `comm peers` to list them.")
    return matches.popitem()[1]


def watch(process, log):
    """Copy proxy output to the log and report its ready session, vendor errors and exit."""
    events = queue.Queue()

    def read():
        for line in process.stdout:
            log.write(line)
            log.flush()
            if line.startswith("Vendor error"):
                events.put(("error", line.strip()))
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if isinstance(event, dict) and event.get("type") == "ready":
                events.put(("ready", event["session"]))
        events.put(("exit", process.wait()))

    threading.Thread(target=read, daemon=True).start()
    return events


def stop(process):
    if process.poll() is not None:
        return
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(STOP_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait()


def proxy_failure(events, log_path):
    """A proxy error or exit seen so far, as a message, or None."""
    while True:
        try:
            kind, value = events.get_nowait()
        except queue.Empty:
            return None
        if kind in ("error", "exit"):
            return f"Proxy {'failed: ' + value if kind == 'error' else f'exited {value}'}; log: {log_path}"


def ask(args, comm, operator, plan_proxy, proxy_command):
    """Relay one message to a worker through a fresh proxy; return the process exit code."""
    preflight(comm)
    deadline = time.monotonic() + args.timeout
    suffix = uuid4().hex[:8]
    joined = not args.session
    if joined:
        if not operator["vendor"]:
            fail("No agent host found in the process tree: pass --operator-vendor or --session.")
        operator_id = comm.call(["join"], {"name": f"message-operator-{suffix}", "vendor": operator["vendor"]})["id"]
    else:
        binding = comm.call(["binding"], session=args.session)
        if not binding.get("active"):
            fail("--session is not a live binding in this workspace and store.")
        operator_id = args.session
    process = log = None
    try:
        renewed = 0.0

        def keep_present():
            nonlocal renewed
            if joined and time.monotonic() - renewed > 30:
                comm.call(["heartbeat"], {"ttlMs": PRESENCE_MS}, operator_id)
                renewed = time.monotonic()

        keep_present()
        worker = resolve_worker(comm, operator_id, args.to)
        vendor, model = plan_proxy(worker["vendor"])
        plan = {"operator": {**operator, "id": operator_id}, "worker": {k: worker.get(k) for k in ("id", "name", "vendor")},
                "proxy": {"vendor": vendor, "model": model}}
        if args.dry_run:
            print(json.dumps(plan))
            return 0
        task = (f"Operator: {operator_id}. Authorized worker: {worker['id']}. Relay only between these identities "
                "and return the worker's actual replies and outcomes.")
        log = tempfile.NamedTemporaryFile("w", prefix="octocode-proxy-", suffix=".log", delete=False)
        print(f"Starting {vendor} proxy with {model}; log: {log.name}", file=sys.stderr, flush=True)
        process = subprocess.Popen(proxy_command(vendor, model, f"octocode-messages-proxy-{suffix}", task),
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                   cwd=comm.workspace, start_new_session=True)
        events = watch(process, log)
        while True:
            try:
                kind, value = events.get(timeout=max(0.0, deadline - time.monotonic()))
            except queue.Empty:
                fail(f"Proxy not ready within {args.timeout}s; log: {log.name}")
            if kind == "ready":
                proxy_id = value
                break
            fail(f"Proxy {'failed: ' + value if kind == 'error' else f'exited {value}'} before it was ready; log: {log.name}")
        plan["proxy"]["id"] = proxy_id
        context = args.context or f"ask-{suffix}"
        receipt = comm.call(["send_message"], {
            "to": proxy_id, "parts": [{"data": {"to": worker["id"], "parts": [{"text": args.text}]}}],
            "reasoning": "Relay one message and return the worker's reply", "replyRequired": True,
            "key": args.key or f"ask-{suffix}", "contextId": context}, operator_id)
        while True:
            answer = next((item for item in comm.items("inbox", operator_id)
                           if item.get("replyTo") == receipt["id"] and "outcome" in item), None)
            if answer:
                break
            if failure := proxy_failure(events, log.name):
                fail(f"{failure}. Message {receipt['id']} stays pending.")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                fail(f"No reply within {args.timeout}s to message {receipt['id']}; log: {log.name}")
            keep_present()
            comm.call(["inbox-wait"], {"timeoutMs": int(min(remaining * 1000, WAIT_SLICE_MS))}, operator_id)
        comm.call(["complete"], {"message": answer["id"]}, operator_id)
        print(json.dumps({**plan, "reply": answer.get("parts", []), "outcome": answer["outcome"],
                          "sent": receipt["id"], "answer": answer["id"], "contextId": context}, ensure_ascii=False))
        log.close()
        os.unlink(log.name)
        return 0 if answer["outcome"] == "completed" else 1
    finally:
        if process:
            stop(process)
        if log:
            log.close()
        if joined:
            try:
                comm.call(["leave"], {}, operator_id)
            except SystemExit as error:
                print(f"Could not leave operator binding {operator_id}: {error}", file=sys.stderr)
