# /// script
# requires-python = ">=3.10"
# dependencies = ["mcp>=1.26,<2"]
# ///
"""Session-scoped MCP access to the official A2A client and operator-selected peers."""
from concurrent.futures import ThreadPoolExecutor
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
from threading import Lock
from uuid import uuid4

from mcp.server.fastmcp import FastMCP

client, encoded_cards, *address = sys.argv[1:]
cards = json.loads(encoded_cards)
server = FastMCP("Octocode messaging peers")


def call(peer, *argv):
    if peer not in cards:
        raise ValueError("Choose a peer from discover.")
    result = subprocess.run([client, *argv, "-a", cards[peer], "-o", "json"],
                            check=False, capture_output=True, text=True)
    if result.returncode:
        detail = result.stdout.strip() or result.stderr.strip() or f"A2A client exited {result.returncode}"
        raise ValueError(detail)
    return json.loads(result.stdout)


@server.tool()
def discover() -> dict:
    """Fetch the Agent Card of every configured peer. Preserve per-peer failures."""
    def card(peer):
        try:
            return call(peer, "card", "get")
        except ValueError as error:
            return {"error": str(error)}

    with ThreadPoolExecutor() as workers:
        return dict(zip(cards, workers.map(card, cards)))


def send(peer: str, text: str, context_id: str | None = None, task_id: str | None = None) -> dict:
    """Send one exact text message. Continue a known task/context using its returned IDs."""
    if not text.strip():
        raise ValueError("Supply the message text.")
    message = {"role": "ROLE_USER", "messageId": str(uuid4()), "parts": [{"text": text}]}
    if context_id:
        message["contextId"] = context_id
    if task_id:
        message["taskId"] = task_id
    # The client accepts a file for a full request. Chat text stays out of argv
    # and a valid message can exceed the operating system's argument limit.
    with TemporaryDirectory() as directory:
        payload = Path(directory, "message.json")
        payload.write_text(json.dumps({"message": message}), encoding="utf-8")
        try:
            return call(peer, "send", "--request-payload", str(payload))
        except ValueError as error:
            raise ValueError(f"Message {message['messageId']}: {error}. Check delivery before retrying.") from error


def fixed_delivery(peer, filename, digest):
    lock = Lock()
    attempted = False
    result = None

    def deliver() -> dict:
        """Deliver the operator's pre-addressed text unchanged and return the peer's response."""
        nonlocal attempted, result
        with lock:
            if result is not None:
                return result
            if attempted:
                raise ValueError("Delivery is uncertain. Inspect the existing attempt before resending.")
            data = Path(filename).read_bytes()
            if sha256(data).hexdigest() != digest:
                raise ValueError("The operator's message file changed after launch.")
            attempted = True
            result = send(peer, data.decode("utf-8"))
            return result

    return deliver


if address:
    server.tool()(fixed_delivery(*address))
else:
    server.tool()(send)


@server.tool()
def get_task(peer: str, task_id: str) -> dict:
    """Read the existing task and its artifacts after an asynchronous or uncertain send."""
    if not task_id or task_id.startswith("-"):
        raise ValueError("Supply a returned task ID.")
    return call(peer, "task", "get", task_id)


if __name__ == "__main__":
    server.run()
