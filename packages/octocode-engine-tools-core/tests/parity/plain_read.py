"""Exercise the real Rust CLI, including cursor rejection and lossless draining."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def main():
    binary = str(Path(sys.argv[1]).resolve())
    report_path = Path(sys.argv[2]).resolve()
    checks = []
    with tempfile.TemporaryDirectory(prefix="octo-plain-read-") as directory:
        root = Path(directory)
        home = root / "home"
        home.mkdir()
        source = root / "source.txt"
        content = "".join(f"line {n}: 🧪 research\n" for n in range(1, 24))
        source.write_text(content)
        env = {
            "PATH": "/usr/bin:/bin",
            "HOME": str(home),
            "OCTOCODE_HOME": str(home / "octocode"),
            "WORKSPACE_ROOT": str(root),
            "ALLOWED_PATHS": str(root),
            "ENABLE_LOCAL": "true",
            "NO_COLOR": "1",
        }

        def run(*args, override=None):
            return subprocess.run(
                [binary, *args], cwd=root, env=env | (override or {}),
                capture_output=True, text=True, timeout=70,
            )

        def check(name, condition):
            checks.append({"name": name, "passed": bool(condition)})
            assert condition, name

        def token(result):
            match = re.search(r"Continue: octo next (\S+)", result.stderr)
            assert match, result.stderr
            return match.group(1)

        for mode, limit in [("lines", "3"), ("bytes", "41")]:
            first = run("read", str(source), "--chunk", mode, "--limit", limit)
            check(f"{mode}: partial and executable next", first.returncode == 6)
            joined = first.stdout
            page = first
            seen = set()
            while page.returncode == 6:
                next_token = token(page)
                check(f"{mode}: unique cursor {len(seen)}", next_token not in seen)
                seen.add(next_token)
                page = run("next", next_token)
                joined += page.stdout
                assert len(seen) < 100
            check(f"{mode}: exact page union", page.returncode == 0 and joined == content)
            drained = run("read", str(source), "--chunk", mode, "--limit", limit, "--all")
            check(f"{mode}: all reconstructs source", drained.returncode == 0 and drained.stdout == content)
            check(f"{mode}: no protocol output", not drained.stderr)
            remaining = run("next", token(first), "--all")
            check(f"{mode}: next all reconstructs tail", remaining.returncode == 0 and first.stdout + remaining.stdout == content)

        first = run("read", str(source), "--chunk", "lines", "--limit", "2")
        original_token = token(first)
        bad = run("next", original_token[:-1] + ("a" if original_token[-1] != "a" else "b"))
        check("corrupt token rejected without content", bad.returncode == 2 and not bad.stdout)
        foreign = run("next", original_token, override={"OCTOCODE_HOME": str(home / "different")})
        check("different configuration scope rejected", foreign.returncode == 2 and not foreign.stdout)
        old_stat = source.stat()
        source.write_text(content.replace("line 1:", "LINE 1:"))
        os.utime(source, ns=(old_stat.st_atime_ns, old_stat.st_mtime_ns))
        stale = run("next", original_token)
        check("same-size same-mtime content change rejected", stale.returncode == 2 and not stale.stdout)

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps({"checks": checks, "passed": len(checks)}, indent=2) + "\n")
    print(f"{len(checks)}/{len(checks)} plain CLI checks passed")


if __name__ == "__main__":
    main()
