#!/usr/bin/env bash
# Validate the selected runner and real MCP schemas before spending on questions.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
fail=0
ok() { printf '  ok    %s\n' "$1"; }
bad() { printf '  FAIL  %s\n' "$1"; fail=1; }
RUNNER_KIND="${RUNNER:-claude}"
command -v "$RUNNER_KIND" >/dev/null && ok "$RUNNER_KIND installed" || bad "$RUNNER_KIND not found"
gh auth status >/dev/null 2>&1 && ok "GitHub authenticated" || bad "GitHub auth unavailable"
command -v pi >/dev/null && ok "pi judge installed" || bad "pi judge not found"
for arm in ${1//,/ }; do
  case "$arm" in
    mcp|mcp-*|local-mcp|local-mcp-*)
      ARM_CMD_VAR="OCTO_MCP_CMD_${arm//-/_}"
      read -r -a CMD <<< "${!ARM_CMD_VAR:-${OCTO_MCP_CMD:-node $REPO/packages/octocode-mcp/dist/index.js}}"
      export BENCH_MCP_CMD="${CMD[*]}" BENCH_ARM="$arm" BENCH_REPO="$REPO" BENCH_BIN="$HERE"
      if python3 - <<'PY'
import json, os, shlex, sys, tempfile
from pathlib import Path
sys.path.insert(0,os.environ['BENCH_BIN'])
from mcp_probe import probe
arm=os.environ['BENCH_ARM']; enabled='clasify' in arm
with tempfile.TemporaryDirectory(prefix='octocode-preflight-') as td:
    base=Path(td); home=base/'home'; home.mkdir(); lab=base/'lab';lab.mkdir()
    sentinel=base/'private.txt'; sentinel.write_text('BENCHMARK_PRIVATE_CANARY')
    root=Path(os.environ.get('LOCAL_REPOS',str(Path(os.environ['BENCH_REPO'])/'packages/octocode-benchmark/local/repos'))) if arm.startswith('local-') else lab
    cmd=shlex.split(os.environ['BENCH_MCP_CMD'])
    env={'OCTOCODE_HOME':str(home),'WORKSPACE_ROOT':str(root.resolve()),'ALLOWED_PATHS':'',
         'OCTOCODE_ENABLE_LOCAL':'true','OCTOCODE_BETA':'true','OCTOCODE_RANKING_ENABLED':str(enabled).lower()}
    if arm.startswith('local-'):
        env['TOOLS_TO_RUN']='localSearch,localFetch,structureSearch,astSearch,lspSearch'+(',clasify' if enabled else '')
    if not enabled: env['OCTOCODE_CLASSIFICATION_API']=''
    s={'command':cmd[0],'args':cmd[1:],'env':env}
    r=probe(s,enabled,enabled,cwd=lab,forbidden=sentinel)
    print(json.dumps({k:r[k] for k in ('ok','problems','toolNames','classifiersTools','catalogChars','catalogSha256')}))
    raise SystemExit(0 if r['ok'] else 2)
PY
      then ok "$arm catalog, feature gates and root denial"; else bad "$arm MCP preflight"; fi ;;
    gh-lean|gh) ok "$arm read-only guard" ;;
    rtk) command -v rtk >/dev/null && ok rtk || bad 'rtk missing' ;;
    headroom) [ -x "${HR_PY:-$HOME/.local/share/uv/tools/headroom-ai/bin/python}" ] && ok headroom || bad 'headroom missing' ;;
    local-shell) ok 'local shell guards' ;;
    *) bad "unknown arm $arm" ;;
  esac
done
exit "$fail"
