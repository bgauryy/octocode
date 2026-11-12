"""Use when launching a small Claude Code or Codex CLI message proxy."""
import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from urllib.parse import urlsplit
from uuid import uuid4

HOSTS = ("claude", "codex")


def fail(message):
    raise SystemExit(message)


def relay_prompt(local=False):
    path = Path(__file__).resolve().parents[1] / "SKILL.md"
    try:
        source = path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"Cannot read proxy instructions from {path}: {error}")
    blocks = []
    for name in (("common", "local") if local else ("common",)):
        pattern = rf"<!-- relay-prompt:{name} -->\s*```text\n(.*?)\n```\s*<!-- /relay-prompt:{name} -->"
        matches = re.findall(pattern, source, re.DOTALL)
        if len(matches) != 1 or not matches[0].strip():
            fail(f"Expected one nonempty {name} prompt block in {path}.")
        blocks.append(matches[0].strip())
    return "\n\n".join(blocks)


def probe(argv):
    return subprocess.run(argv, capture_output=True, text=True, check=False)


def signed_in(host):
    if host == "codex":
        return probe([host, "login", "status"]).returncode == 0
    result = probe([host, "auth", "status"])
    if result.returncode:
        return False
    try:
        status = json.loads(result.stdout)
    except ValueError:
        return False
    return isinstance(status, dict) and status.get("loggedIn") is True


def small_codex():
    result = probe(["codex", "debug", "models"])
    if result.returncode:
        return None
    try:
        models = json.loads(result.stdout)["models"]
    except (ValueError, KeyError, TypeError):
        return None
    if not isinstance(models, list):
        return None
    for model in models:
        if not isinstance(model, dict) or model.get("visibility") != "list":
            continue
        slug = model.get("slug")
        if isinstance(slug, str) and re.search(r"(?:^|-)(?:mini|luna|nano)(?:-|$)", slug):
            return slug
    return None


def host_for(vendor, model, cloud):
    candidates = HOSTS if vendor == "auto" else (vendor,)
    for host in candidates:
        if cloud and host != "claude":
            continue
        if not shutil.which(host):
            continue
        if not signed_in(host):
            continue
        selected = model or ("haiku" if host == "claude" else small_codex())
        if selected:
            return host, selected
    if cloud:
        fail("Cloud mode needs a signed-in Claude CLI. Codex Cloud does not expose this relay interface.")
    fail("No eligible signed-in CLI and small model. Run detect or specify --vendor and --model.")


def cards_for(entries):
    cards = {}
    for entry in entries:
        name, separator, url = entry.partition("=")
        parsed = urlsplit(url)
        local = parsed.hostname in {"localhost", "127.0.0.1", "::1"}
        if (not separator or not name or name in cards or not parsed.hostname or
                parsed.username is not None or parsed.password is not None or
                (parsed.scheme != "https" and not (local and parsed.scheme == "http"))):
            fail("Each --card needs a unique NAME=HTTPS_CARD_URL (HTTP is supported on loopback).")
        cards[name] = url
    return cards


def comm_command():
    binary = shutil.which("octocode-agents-communication")
    if binary:
        return [binary, "/cli"]
    if shutil.which("npx"):
        return ["npx", "-y", "@octocodeai/octocode-agents-communication", "/cli"]
    fail("Install @octocodeai/octocode-agents-communication or provide npx on PATH.")


def run_json(argv, payload=None):
    result = subprocess.run(argv, input=json.dumps(payload) if payload is not None else None,
                            capture_output=True, text=True)
    if result.returncode:
        fail(result.stderr.strip() or result.stdout.strip() or f"{argv[0]} exited {result.returncode}")
    return json.loads(result.stdout)


def registered_send(args, vendor, address, text):
    comm = comm_command()
    binding = ["--session", args.session, "--json"]
    if args.database:
        binding += ["--database", args.database]
    sender = run_json([*comm, "binding", *binding])
    if not sender.get("active") or (args.source and sender["vendor"] != args.source):
        fail("Use the sending agent's own live Octocode binding in this workspace.")

    matches = set()
    page_input = {}
    while True:
        page = run_json([*comm, "peers", "-", *binding], page_input)
        for peer in page["items"]:
            if peer["vendor"] == vendor and address in (peer["id"], peer.get("vendorSession")):
                matches.add(peer["id"])
        if "next" not in page:
            break
        page_input = page["next"]["input"]
    if len(matches) != 1:
        fail(f"Expected one live {vendor} peer with that ID, found {len(matches)}. Check the recipient's binding and shared store.")
    target = matches.pop()
    if target == sender["id"]:
        fail("The resolved recipient is the sender.")
    key = args.key or str(uuid4())
    message = {"to": target, "key": key, "reasoning": "Operator-directed peer message",
               "replyRequired": args.reply_required, "parts": [{"text": text}]}
    if args.context:
        message["contextId"] = args.context
    print(f"Octocode send key {key}", file=sys.stderr, flush=True)
    receipt = run_json([*comm, "send_message", "-", *binding], message)
    print(json.dumps({"transport": "octocode", "from": sender["id"], "to": target,
                      "key": key, "receipt": receipt}))


def native_command(host, model, prompt):
    if host == "claude":
        return ["claude", "--model", model, "--strict-mcp-config", "--tools", "ListAgents,SendMessage,ToolSearch",
                "--", relay_prompt() + "\nUse ListAgents and SendMessage for Claude peers.\n" + prompt]
    return ["codex", "-m", model, "-s", "read-only", "-c", 'model_reasoning_effort="low"', "--",
            relay_prompt() + "\nYou are a separate Codex session. Use codex queue with a verified "
            "independent thread UUID; do not assume membership of the operator's team.\n" + prompt]


def send(args, cli):
    vendor, sep, address = args.to.partition(":")
    if not sep or vendor not in HOSTS or not address:
        cli.error("--to needs claude:ID or codex:ID")
    if not args.session and args.source != vendor:
        cli.error("Cross-vendor delivery requires --session with your Octocode binding. For a native send, specify its matching --from vendor.")
    if not args.session and (args.database or args.key or args.context or args.reply_required):
        cli.error("Database selection, idempotency keys and correlated replies require --session.")
    data = sys.stdin.buffer.read() if args.file == "-" else Path(args.file).read_bytes()
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        cli.error("The message must be valid UTF-8.")
    if not text.strip():
        cli.error("Supply a nonempty UTF-8 message file or pipe it on stdin.")
    os.chdir(args.workspace)
    if args.session:
        return registered_send(args, vendor, address, text)
    if vendor == "codex":
        command = ["codex", "queue", "--thread", address, f"--message={text}"]
    elif address.startswith(("session_", "cse_")):
        result = subprocess.run(["claude", "-p", "--output-format", "json", "--cloud", address], input=data)
        raise SystemExit(result.returncode)
    else:
        from claude_send import deliver
        print(json.dumps(deliver(address, text, run_json(["claude", "agents", "--json"]))))
        return
    os.execvp(command[0], command)


def a2a_command(host, model, cards, args):
    uv = shutil.which("uv")
    client = shutil.which("a2a")
    if not uv or not client:
        fail("A2A mode needs the official a2a CLI and uv on PATH.")
    adapter = Path(__file__).with_name("a2a_tools.py").resolve()
    server = {"command": uv, "args": ["run", "--script", str(adapter), client, json.dumps(cards)]}
    prompt = relay_prompt() + "\n\nOperator request:\n" + args.task
    if args.deliver:
        peer, path = args.deliver
        payload = path.read_bytes()
        if not payload.decode("utf-8").strip():
            fail("The message file must contain UTF-8 text.")
        server["args"].extend([peer, str(path), sha256(payload).hexdigest()])
        prompt += f"\nDiscover the peers, call deliver once for the pre-addressed message to {peer}, and return its reply."
    if host == "claude":
        return ["claude", "--model", model, "--name", args.name, "--strict-mcp-config",
                "--mcp-config", json.dumps({"mcpServers": {"relay": server}}),
                "--tools", "", "--allowedTools", "mcp__relay__*", "--disable-slash-commands",
                *(["--print"] if args.once else []), "--", prompt]
    config = [arg for key, value in server.items()
              for arg in ("-c", f"mcp_servers.relay.{key}={json.dumps(value)}")]
    # A run-scoped stdio server. The ordinary CLI keeps the user's authentication.
    return ["codex", *(["exec", "--skip-git-repo-check"] if args.once else []), "-m", model,
            "-s", "read-only", "-c", 'model_reasoning_effort="low"',
            "-c", "features.shell_tool=false", "-c", "features.multi_agent=false",
            "-c", 'mcp_servers.relay.default_tools_approval_mode="approve"',
            *config, "--", prompt]


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="command", required=True)
    commands.add_parser("detect", help="show installed CLI paths without starting a model")
    direct = commands.add_parser("send", help="deliver one message to an existing peer")
    direct.add_argument("--to", required=True, metavar="VENDOR:ID")
    direct.add_argument("--from", dest="source", choices=HOSTS, help="native sender vendor, or check a shared binding")
    direct.add_argument("--session", help="sender's own Octocode binding for durable delivery")
    direct.add_argument("--workspace", default=os.getcwd())
    direct.add_argument("--database")
    direct.add_argument("--file", default="-", help="UTF-8 message file, or - for stdin")
    direct.add_argument("--reply-required", action="store_true")
    direct.add_argument("--key", help="stable Octocode key for unchanged input")
    direct.add_argument("--context", help="Octocode conversation context")
    start = commands.add_parser("start")
    start.add_argument("--vendor", choices=("auto", *HOSTS), default="auto")
    start.add_argument("--model", help="explicit small model ID for the chosen provider")
    start.add_argument("--dry-run", action="store_true", help="show the resolved command without starting a proxy")
    start.add_argument("--name", default="octocode-messages-proxy")
    start.add_argument("--workspace", default=os.getcwd())
    start.add_argument("--database", help="existing local coordination database or an isolated test database")
    route = start.add_mutually_exclusive_group()
    route.add_argument("--native", action="store_true", help="use this vendor's own messaging tools")
    route.add_argument("--cloud", action="store_true", help="create a Claude cloud session")
    route.add_argument("--card", action="append", default=[], metavar="NAME=URL", help="A2A peer, repeatable")
    start.add_argument("--once", action="store_true", help="finish the A2A CLI session after this request")
    job = start.add_mutually_exclusive_group(required=True)
    job.add_argument("--task", default="", help="scope, authorized recipients and message or relay instructions")
    job.add_argument("--deliver", nargs=2, metavar=("PEER", "TEXT_FILE"), help="A2A: send a fixed UTF-8 message to one configured peer")
    args = cli.parse_args()
    if args.command == "detect":
        print(json.dumps({name: shutil.which(name) for name in (*HOSTS, "a2a", "uv", "octocode-agents-communication", "npx")}))
        return
    if args.command == "send":
        return send(args, cli)
    if args.native and args.vendor == "auto":
        cli.error("--native needs the vendor of the intended peers")
    cards = cards_for(args.card)
    if args.deliver:
        peer, path = args.deliver
        if peer not in cards:
            cli.error("--deliver needs a peer named by --card")
        args.deliver = (peer, Path(path).resolve(strict=True))
    if args.model and args.vendor == "auto":
        cli.error("select --vendor with a provider-specific --model")
    if args.once and not cards:
        cli.error("--once applies to A2A sessions")
    if args.database and (cards or args.cloud or args.native):
        cli.error("--database applies to local coordination")
    os.chdir(args.workspace)
    host, model = host_for(args.vendor, args.model, args.cloud)
    if cards:
        command = a2a_command(host, model, cards, args)
    elif args.native:
        command = native_command(host, model, args.task)
        if host == "claude":
            command[1:1] = ["--name", args.name]
    elif args.cloud:
        command = ["claude", "--model", model, "--name", args.name, "--cloud",
                   relay_prompt() + "\nUse the account's remote session tools.\n\nOperator request:\n" + args.task]
    else:
        command = [*comm_command(), "run", "--vendor", host, "--model", model, "--name", args.name,
                   "--tools", "messaging",
                   "--prompt", relay_prompt(local=True) + "\n\nOperator request:\n" + args.task]
        if args.database:
            command.extend(["--database", args.database])
    if args.dry_run:
        route = "a2a" if cards else "native" if args.native else "cloud" if args.cloud else "local"
        print(json.dumps({"vendor": host, "model": model, "route": route,
                          "workspace": os.getcwd(), "argv": command}))
        return
    print(f"Starting {host} relay with {model}", file=sys.stderr, flush=True)
    os.execvp(command[0], command)


if __name__ == "__main__":
    main()
