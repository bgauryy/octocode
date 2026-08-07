#!/usr/bin/env bash
# Phase 0 prerequisites check for the octocode benchmark (all arms across matchups).
# Usage:  bash skills/octocode-benchmark/scripts/check-prereqs.sh [octocode-version]
#   octocode-version  optional npm version to pin arm A (default: latest published)
# Exit 0 = all required checks pass; exit 1 = at least one required check failed.
# Prints one PASS/FAIL line per check and pins the versions it found (record these
# in the report). A failed check means the run is invalid — fix before Phase 1.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OCTO_VER="${1:-latest}"
HR_ROOT="$PKG_ROOT/compare/octocode-vs-gh-headroom"
BIN_ROOT="$PKG_ROOT/compare/bin"
NULL=/dev/null

fail=0
pass(){ printf '  \033[32mPASS\033[0m %s\n' "$1"; }
bad(){  printf '  \033[31mFAIL\033[0m %s\n' "$1"; fail=1; }
warn(){ printf '  \033[33mWARN\033[0m %s\n' "$1"; }
have(){ command -v "$1" >"$NULL" 2>&1; }

echo "== Phase 0 preflight — octocode benchmark (all arms) =="
echo "package root: $PKG_ROOT"
echo

# ---- Arm A: Octocode ---------------------------------------------------------
echo "Arm A — Octocode (npx octocode@$OCTO_VER)"
if have npx; then
  v="$(npx -y "octocode@$OCTO_VER" --version 2>"$NULL" | tail -1)"
  [ -n "$v" ] && pass "octocode CLI reachable: $v  (note: --version string may lag the package version)" || bad "octocode CLI did not report a version"
  probe="$(npx -y "octocode@$OCTO_VER" tools ghSearchRepos --queries '[{"keywords":["is"],"owner":"sindresorhus","limit":1}]' 2>&1)"
  printf '%s' "$probe" | grep -qiE 'sindresorhus|repositories' && pass "octocode tools probe returned data" || bad "octocode tools probe failed: $(printf '%s' "$probe" | head -1)"
else
  bad "npx not on PATH (need Node.js)"
fi
echo

# ---- Arm B: gh + RTK ---------------------------------------------------------
echo "Arm B — gh + RTK"
if have gh; then
  pass "gh: $(gh --version 2>"$NULL" | head -1)"
  if gh auth status >"$NULL" 2>&1; then pass "gh authenticated"; else bad "gh not authenticated (gh auth login)"; fi
else
  bad "gh not on PATH"
fi
if have rtk; then
  pass "rtk: $(rtk --version 2>"$NULL" | head -1)"
  if rtk gh search repos octocode --limit 1 >"$NULL" 2>&1; then pass "rtk gh probe ok"; else warn "rtk gh probe did not return cleanly (check gh auth / network)"; fi
else
  bad "rtk not on PATH"
fi
echo

# ---- Arm C: gh + Headroom ----------------------------------------------------
echo "Arm C — gh + Headroom"
if have headroom; then
  pass "headroom: $(headroom --version 2>"$NULL" | head -1)"
else
  bad "headroom not on PATH (uv tool install --python 3.13 'headroom-ai[all]')"
fi
export HR_PY="${HR_PY:-$HOME/.local/share/uv/tools/headroom-ai/bin/python}"
if [ -x "$HR_PY" ]; then
  pass "HR_PY interpreter: $HR_PY"
  if [ -f "$BIN_ROOT/preflight.py" ]; then
    # stderr carries HF/model-load noise -> discard; parse stdout JSON robustly.
    # one retry covers a cold model download on first invocation.
    hr_ok=""
    for _try in 1 2; do
      out="$("$HR_PY" "$BIN_ROOT/preflight.py" --warmup 2>&1)"
      hr_ok="$(printf '%s' "$out" | "$HR_PY" "$SCRIPT_DIR/_hr_check.py" 2>"$NULL")"
      [ "$hr_ok" = "OK" ] && break
    done
    if [ "$hr_ok" = "OK" ]; then
      pass "headroom preflight --warmup OK (compression active, failures: [])"
    else
      bad "headroom preflight --warmup did not pass (${hr_ok:-no-output} — compression OFF / model unavailable); measurement is invalid until it passes"
    fi
  else
    warn "headroom preflight.py not found at $BIN_ROOT/preflight.py"
  fi
  if "$HR_PY" "$SCRIPT_DIR/_hr_import.py" >"$NULL" 2>&1; then pass "headroom library importable"; else warn "headroom library import failed under HR_PY"; fi
else
  bad "HR_PY not executable: $HR_PY (set HR_PY or install headroom-ai via uv)"
fi
echo

# ---- Questions + arm primers -------------------------------------------------
echo "Inputs"
qn="$(ls "$PKG_ROOT"/compare/github-questions/Q*.md 2>"$NULL" | wc -l | tr -d ' ')"
[ "$qn" -gt 0 ] && pass "question set present: $qn questions in compare/github-questions/" || bad "no questions found in compare/github-questions/"
prim="$PKG_ROOT/skills/octocode-benchmark/references/RUNNER_TOOL_CONTEXT.md"
if [ -f "$prim" ] && grep -q "Octocode arm" "$prim" && grep -Eq "RTK arm" "$prim" && grep -Eq "Headroom arm" "$prim"; then
  pass "all three arm primers present in RUNNER_TOOL_CONTEXT.md"
else
  bad "arm primers missing in RUNNER_TOOL_CONTEXT.md (need Octocode/gh+RTK/gh+Headroom)"
fi
echo

if [ "$fail" -eq 0 ]; then
  printf '\033[32m== ALL REQUIRED CHECKS PASSED — safe to start Phase 1 ==\033[0m\n'
else
  printf '\033[31m== PREFLIGHT FAILED — fix the FAIL items before running (the campaign is invalid otherwise) ==\033[0m\n'
fi
exit "$fail"
