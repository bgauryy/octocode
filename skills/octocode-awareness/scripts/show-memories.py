#!/usr/bin/env python3
"""show-memories — visualize the awareness store as smart, sortable HTML.

Reads the ONE shared store (~/.octocode/memory/awareness.sqlite3, or
OCTOCODE_MEMORY_HOME), which holds memories, refinements, notifications,
intents, and locks in a single file. Renders five sortable panels with per-row
delete buttons where deletion is supported. Each row is correlated to ONE file
(or "general" when none). Stdlib only; no external deps.

Modes:
  serve (default): start a localhost server with working delete buttons and open a browser.
  --no-serve --out FILE: write a static read-only HTML snapshot (delete buttons show the CLI command).

Delete buttons call back into awareness.py (forget / refine-delete / notify-prune)
so deletes go through the canonical path (FTS cleanup, read-cursor cleanup, etc.).
"""
from __future__ import annotations

import argparse
import json
import os
import secrets
import sqlite3
import subprocess
import sys
import webbrowser
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent
AWARENESS = HERE / "awareness.py"
TEMPLATE = HERE / "show-memories.template.html"
DB_NAME = "awareness.sqlite3"


def memory_db_path(arg: str | None) -> Path:
    if arg:
        return Path(arg).expanduser().resolve(strict=False)
    home = os.environ.get("OCTOCODE_MEMORY_HOME")
    base = Path(home).expanduser() if home else Path.home() / ".octocode" / "memory"
    return (base / DB_NAME).resolve(strict=False)


def refine_db_path(workspace: str | None, override: str | None) -> tuple[Path, Path]:
    # Refinements + notifications now live in the ONE shared store (the same file
    # as memories), so default to it. `workspace` is kept only for the display
    # header; an explicit --workspace-db still wins (tests/isolation).
    ws = Path(workspace).expanduser().resolve(strict=False) if workspace else Path.cwd().resolve()
    if override:
        return Path(override).expanduser().resolve(strict=False), ws
    return memory_db_path(None), ws


def _rows(db: Path, table: str) -> list[sqlite3.Row]:
    if not db.exists():
        return []
    conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    try:
        names = {r["name"] for r in conn.execute(f"PRAGMA table_info({table})").fetchall()}
        if not names:
            return []
        return conn.execute(f"SELECT * FROM {table}").fetchall()
    except sqlite3.Error:
        return []
    finally:
        conn.close()


def load_memories(db: Path) -> list[dict]:
    out = []
    for r in _rows(db, "agent_memories"):
        k = r.keys()
        out.append({
            "memory_id": r["memory_id"],
            "agent_id": r["agent_id"],
            "task_context": r["task_context"],
            "observation": r["observation"],
            "importance_score": r["importance_score"],
            "state": r["state"] if "state" in k else "ACTIVE",
            "label": r["label"] if "label" in k and r["label"] else "OTHER",
            "file": r["file"] if "file" in k else None,
            "tags": json.loads(r["tags_json"]) if "tags_json" in k and r["tags_json"] else [],
            "failure_signature": r["failure_signature"] if "failure_signature" in k else None,
            "created_at": r["created_at"],
            "updated_at": r["updated_at"] if "updated_at" in k else None,
        })
    return out


def load_intents(db: Path) -> list[dict]:
    """Harness / verify lane: pre-flight intents + whether their declared
    test-plan was actually VERIFIED (derived from intent_events)."""
    events: dict[str, dict] = {}
    for e in _rows(db, "intent_events"):
        ev = events.setdefault(e["intent_id"], {"count": 0, "verified": False})
        ev["count"] += 1
        if (e["event_type"] or "").upper() == "VERIFIED":
            ev["verified"] = True
    out = []
    for r in _rows(db, "agent_intents"):
        k = r.keys()
        ev = events.get(r["intent_id"], {"count": 0, "verified": False})
        out.append({
            "intent_id": r["intent_id"],
            "agent_id": r["agent_id"],
            "rationale": r["rationale"],
            "test_plan": r["test_plan"],
            "status": r["status"],
            "verified": ev["verified"],
            "events": ev["count"],
            "plan_doc_ref": r["plan_doc_ref"] if "plan_doc_ref" in k else None,
            "created_at": r["created_at"],
            "updated_at": r["updated_at"] if "updated_at" in k else None,
        })
    return out


def load_locks(db: Path) -> list[dict]:
    """Files-awareness: who currently holds which file."""
    out = []
    for r in _rows(db, "file_locks"):
        out.append({
            "lock_id": r["lock_id"],
            "file": r["file_path"],
            "agent_id": r["agent_id"],
            "lock_type": r["lock_type"],
            "intent_id": r["intent_id"],
            "acquired_at": r["acquired_at"],
            "expires_at": r["expires_at"],
        })
    return out


def load_refinements(db: Path) -> list[dict]:
    out = []
    for r in _rows(db, "refinements"):
        files = json.loads(r["files_json"]) if r["files_json"] else []
        out.append({
            "refinement_id": r["refinement_id"],
            "agent_id": r["agent_id"],
            "repo": r["repo"],
            "ref": r["ref"],
            "file": files[0] if files else None,
            "files": files,
            "reasoning": r["reasoning"],
            "remember": r["remember"],
            "quality": r["quality"],
            "state": r["state"],
            "created_at": r["created_at"],
            "updated_at": r["updated_at"],
        })
    return out


def load_notifications(db: Path) -> list[dict]:
    out = []
    for r in _rows(db, "notifications"):
        k = r.keys()
        files = json.loads(r["files_json"]) if "files_json" in k and r["files_json"] else []
        out.append({
            "notification_id": r["notification_id"],
            "from_agent": r["from_agent"],
            "to_agent": r["to_agent"] if "to_agent" in k else None,
            "kind": r["kind"],
            "subject": r["subject"],
            "body": r["body"] if "body" in k else None,
            "file": files[0] if files else None,
            "files": files,
            "thread_id": r["thread_id"] if "thread_id" in k else None,
            "importance": r["importance"] if "importance" in k else None,
            "status": r["status"] if "status" in k else "open",
            "repo": r["repo"],
            "ref": r["ref"],
            "created_at": r["created_at"],
        })
    return out


def build_data(mem_db: Path, ref_db: Path, workspace: Path, serve: bool, csrf: str = "") -> dict:
    return {
        "config": {
            "serve": serve,
            "csrf": csrf,
            "memoryDb": str(mem_db),
            "refineDb": str(ref_db),
            "workspace": str(workspace),
            "generatedAt": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        },
        "memories": load_memories(mem_db),
        "refinements": load_refinements(ref_db),
        "notifications": load_notifications(ref_db),
        "intents": load_intents(mem_db),
        "locks": load_locks(mem_db),
    }


def render(data: dict) -> str:
    tpl = TEMPLATE.read_text(encoding="utf-8")
    # Embed inside a <script> block safely: cross-agent content (observations,
    # reasoning, file paths) is untrusted. json.dumps does NOT neutralize
    # "</script>" or "<", so escape the HTML-significant chars to \uXXXX — valid
    # JS string escapes that the HTML parser cannot mistake for markup. Prevents
    # stored XSS from a poisoned memory breaking out of the data block.
    payload = json.dumps(data)
    for ch, esc in (("<", "\\u003c"), (">", "\\u003e"), ("&", "\\u0026"),
                    (" ", "\\u2028"), (" ", "\\u2029")):
        payload = payload.replace(ch, esc)
    return tpl.replace("__AWARENESS_DATA__", payload)


def run_delete(cmd: list[str]) -> tuple[int, dict]:
    done = subprocess.run(cmd, text=True, capture_output=True, check=False)
    try:
        payload = json.loads(done.stdout) if done.stdout.strip() else {"ok": done.returncode == 0}
    except json.JSONDecodeError:
        payload = {"ok": False, "error": done.stderr.strip() or "non-JSON output"}
    return done.returncode, payload


def make_handler(mem_db: Path, ref_db: Path, workspace: Path, csrf: str):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):  # quiet
            pass

        def _send(self, code: int, body: bytes, ctype: str):
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _json(self, code: int, obj: dict):
            self._send(code, json.dumps(obj).encode(), "application/json")

        def do_GET(self):
            if self.path in ("/", "/index.html"):
                html = render(build_data(mem_db, ref_db, workspace, True, csrf))
                self._send(200, html.encode(), "text/html; charset=utf-8")
            elif self.path == "/api/data":
                self._json(200, build_data(mem_db, ref_db, workspace, True, csrf))
            else:
                self._json(404, {"ok": False, "error": "not found"})

        def do_POST(self):
            # CSRF guard: only same-page JS (which read the token from the served
            # HTML) can delete; a cross-origin page cannot read it.
            if self.headers.get("X-CSRF-Token", "") != csrf:
                return self._json(403, {"ok": False, "error": "bad or missing CSRF token"})
            length = int(self.headers.get("Content-Length", 0))
            try:
                body = json.loads(self.rfile.read(length) or b"{}")
            except json.JSONDecodeError:
                return self._json(400, {"ok": False, "error": "bad json"})
            mid = str(body.get("id", "")).strip()
            if not mid:
                return self._json(400, {"ok": False, "error": "missing id"})
            if self.path == "/api/delete-memory":
                code, payload = run_delete(
                    [sys.executable, str(AWARENESS), "--db", str(mem_db), "forget", "--memory-id", mid]
                )
            elif self.path == "/api/delete-refinement":
                code, payload = run_delete(
                    [sys.executable, str(AWARENESS), "--db", str(ref_db), "refine-delete", "--refinement-id", mid]
                )
            elif self.path == "/api/delete-notification":
                code, payload = run_delete(
                    [sys.executable, str(AWARENESS), "--db", str(ref_db), "notify-prune", "--notification-id", mid]
                )
            else:
                return self._json(404, {"ok": False, "error": "not found"})
            self._json(200 if code == 0 else 500, payload)

    return Handler


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="Render the awareness stores as sortable HTML.")
    p.add_argument("--memory-db", help="Global memory DB path (default: OCTOCODE_MEMORY_HOME or ~/.octocode/memory).")
    p.add_argument("--workspace", help="Workspace root for refinements (default: cwd).")
    p.add_argument("--workspace-db", help="Override the refinement DB path directly.")
    p.add_argument("--host", default="127.0.0.1", help="Bind host (default localhost only).")
    p.add_argument("--port", type=int, default=8787)
    p.add_argument("--allow-remote", action="store_true",
                   help="Required to bind a non-loopback --host (exposes delete endpoints to the network).")
    p.add_argument("--no-serve", action="store_true", help="Write a static snapshot instead of serving.")
    p.add_argument("--out", help="Output HTML path for --no-serve (default: ./awareness.html).")
    p.add_argument("--no-open", action="store_true", help="Do not open a browser.")
    args = p.parse_args(argv)

    mem_db = memory_db_path(args.memory_db)
    ref_db, workspace = refine_db_path(args.workspace, args.workspace_db)

    if args.no_serve:
        out = Path(args.out).expanduser().resolve(strict=False) if args.out else Path.cwd() / "awareness.html"
        out.write_text(render(build_data(mem_db, ref_db, workspace, False)), encoding="utf-8")
        print(json.dumps({"ok": True, "mode": "static", "out": str(out),
                          "memoryDb": str(mem_db), "refineDb": str(ref_db)}, indent=2))
        if not args.no_open:
            webbrowser.open(out.as_uri())
        return 0

    loopback = {"127.0.0.1", "::1", "localhost"}
    if args.host not in loopback and not args.allow_remote:
        print(json.dumps({"ok": False, "error": f"refusing to bind non-loopback host '{args.host}' "
                          "(delete endpoints would be network-exposed); pass --allow-remote to override."}, indent=2))
        return 2

    csrf = secrets.token_hex(16)
    httpd = ThreadingHTTPServer((args.host, args.port), make_handler(mem_db, ref_db, workspace, csrf))
    url = f"http://{args.host}:{args.port}/"
    print(json.dumps({"ok": True, "mode": "serve", "url": url,
                      "memoryDb": str(mem_db), "refineDb": str(ref_db),
                      "note": "Ctrl-C to stop"}, indent=2))
    if not args.no_open:
        webbrowser.open(url)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print(json.dumps({"ok": True, "stopped": True}))
    finally:
        httpd.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
