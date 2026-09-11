#!/usr/bin/env python3
"""Minimal direct JSON-RPC/LSP client used by the semantic baseline lanes."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time


class LspError(RuntimeError):
    pass


def _frame(message: dict[str, object]) -> bytes:
    body = json.dumps(message, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    return f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body


def _read_message(stream, timeout: float, buffer: bytearray) -> dict[str, object]:
    deadline = time.monotonic() + timeout
    while True:
        header_end = buffer.find(b"\r\n\r\n")
        if header_end >= 0:
            headers = buffer[:header_end].decode("ascii", errors="replace").split("\r\n")
            lengths = [line.split(":", 1)[1].strip() for line in headers if line.lower().startswith("content-length:")]
            if not lengths:
                raise LspError("LSP response has no Content-Length")
            length = int(lengths[-1])
            body_start = header_end + 4
            if len(buffer) >= body_start + length:
                body = bytes(buffer[body_start:body_start + length])
                del buffer[:body_start + length]
                return json.loads(body)
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise LspError("timed out waiting for LSP response")
        readable, _, _ = select.select([stream], [], [], remaining)
        if not readable:
            raise LspError("timed out waiting for LSP response")
        chunk = os.read(stream.fileno(), 65536)
        if not chunk:
            raise LspError("language server closed stdout")
        buffer.extend(chunk)


def _request(process: subprocess.Popen[bytes], request_id: int, method: str, params: object, timeout: float, buffer: bytearray) -> dict[str, object]:
    assert process.stdin is not None and process.stdout is not None
    process.stdin.write(_frame({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}))
    process.stdin.flush()
    while True:
        message = _read_message(process.stdout, timeout, buffer)
        if message.get("id") == request_id:
            return message


def _notify(process: subprocess.Popen[bytes], method: str, params: object) -> None:
    assert process.stdin is not None
    process.stdin.write(_frame({"jsonrpc": "2.0", "method": method, "params": params}))
    process.stdin.flush()


def run(server: list[str], root: Path, method: str, params: object, timeout: float) -> dict[str, object]:
    process = subprocess.Popen(
        server, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        cwd=root, start_new_session=True,
    )
    buffer = bytearray()
    try:
        initialize = _request(process, 1, "initialize", {
            "processId": os.getpid(), "rootUri": root.resolve().as_uri(),
            "capabilities": {}, "workspaceFolders": [{"uri": root.resolve().as_uri(), "name": root.name}],
        }, timeout, buffer)
        if "error" in initialize:
            raise LspError(f"initialize failed: {initialize['error']}")
        _notify(process, "initialized", {})
        response = _request(process, 2, method, params, timeout, buffer)
        shutdown = _request(process, 3, "shutdown", None, timeout, buffer)
        _notify(process, "exit", None)
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
        return {"initialize": initialize, "response": response, "shutdown": shutdown, "serverArgv": server}
    finally:
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        if process.stderr is not None:
            process.stderr.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--method", required=True)
    parser.add_argument("--params-json", required=True)
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--server", nargs=argparse.REMAINDER, required=True)
    args = parser.parse_args()
    server = args.server[1:] if args.server[:1] == ["--"] else args.server
    if not server:
        parser.error("--server requires a language-server command")
    try:
        result = run(server, args.root, args.method, json.loads(args.params_json), args.timeout_seconds)
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
        return 1 if "error" in result["response"] else 0
    except (OSError, ValueError, json.JSONDecodeError, LspError) as exc:
        print(json.dumps({"error": str(exc)}), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
