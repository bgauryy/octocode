# Archived pre-v4 Terra v3 independent assessment receipt

> This diagnostic predates public suite v4, the refreshed workspace CLI, and the current
> public-oracle gate. Preserve it as historical evidence only. The live contract validator
> and `compare/terra-v3/` documentation are authoritative; comparative performance and token
> usage remain unscored.

Date: 2026-09-11 (Asia/Jerusalem)

## Verdict

No performance or winner claim is valid from this host. The checked-in harness and its
deterministic checks are sound enough to gate a future campaign, but this assessment has
only raw diagnostic smoke evidence. A valid Terra campaign additionally needs a fresh,
receipt-matching workspace candidate, sealed private cases, a Linux cgroup-v2 host, and
the missing native baseline services.

## Fixed corpus evidence

Command:

```sh
git -C /Users/bgaryy/.octocode/tmp/clone/langchain-ai/langchain/master__host_4f9820f36e499d38 rev-parse HEAD
git -C /Users/bgaryy/.octocode/tmp/clone/vercel/next.js/canary__host_87c33434e817364e rev-parse HEAD
```

Result: `67ee6cb63dd9ae7f3a4dfedc3095652bce15a125` (LangChain) and
`d155ba9ebfffe4742efefda8d68c2e0e8e490924` (Next.js).

Command:

```sh
python3 packages/octocode-benchmark/compare/bin/terra_v3_preflight.py lock-existing \
  --langchain-path /Users/bgaryy/.octocode/tmp/clone/langchain-ai/langchain/master__host_4f9820f36e499d38 \
  --langchain-sha 67ee6cb63dd9ae7f3a4dfedc3095652bce15a125 \
  --nextjs-path /Users/bgaryy/.octocode/tmp/clone/vercel/next.js/canary__host_87c33434e817364e \
  --nextjs-sha d155ba9ebfffe4742efefda8d68c2e0e8e490924 \
  --fixture-manifest packages/octocode-benchmark/compare/terra-v3/suite/public-cases.json \
  --output /tmp/terra-v3-assessment.uW0CiM/CORPUS.lock.json
```

Result: success; corpus lock digest
`aeaf6d4ece9b9f21f7af2e76af0055e905a1ae60ad706734fb75ad9cc75b5660`.
Content-tree digests: LangChain `d8afa75b8a37fb67ac294b155fcbd9bff8475e98b98444ace9627c1c0ee16159`
(3,125 files, 42,652,203 bytes); Next.js
`89a2f0784ca124e3e0b68a66ce159873da61e33f360a5b4996bcf5cf22f11081`
(31,063 files, 147,691,374 bytes).

## Harness checks

Commands:

```sh
python3 -m unittest discover -s packages/octocode-benchmark/compare/bin -p 'test_*.py' -v
python3 packages/octocode-benchmark/compare/bin/terra_v3_contracts.py \
  --contracts packages/octocode-benchmark/compare/terra-v3/contracts \
  --suite packages/octocode-benchmark/compare/terra-v3/suite
```

Results: all 21 Python tests passed. Contract validation was valid with digest
`b3724d47a018b2a2396c395a2f4e05de0cf45074dc2fdd2b1c5d3fd09d83d630`.
It correctly reports `campaignReady: false`: all 20 private slots are only
`slots-reserved`, not curator-sealed held-out cases. This alone prevents a valid campaign.

## Candidate and host preflight

Command:

```sh
uname -sm
command -v src pyright-langserver typescript-language-server docker podman
test -f /sys/fs/cgroup/cgroup.controllers && echo yes || echo no
```

Result: `Darwin arm64`; none of `src`, `pyright-langserver`,
`typescript-language-server`, Docker, or Podman is installed; cgroup v2 is absent.

Therefore these lanes are unavailable or invalid here:

- Indexed/end-to-end Sourcegraph: no `src` client or self-hosted server, and no exact-commit index.
- Direct Pyright and TypeScript semantic controls: their frozen language-server executables are absent.
- Strict resource/performance measurements: Darwin and no cgroup v2. The harness labels macOS diagnostic-only.
- Container-backed isolation: no Docker or Podman.

The workspace CLI originally identified itself as `octocode v19.1.0`, but its artifact
predated current benchmark-relevant sources. The engine build was refreshed, but tools-core
and CLI rebuild are intentionally blocked until the local sibling core resolution is
authorized; no stale CLI result is a candidate comparison result.

Commands:

```sh
yarn workspace @octocodeai/octocode-engine build:dev
python3 -c '... build_workspace_receipt(...); validate_workspace_receipt(...) ...'
```

Results: the engine development build succeeded. The receipt diagnostic returned
`stale CLI artifact predates benchmark-relevant source or native artifact` (candidate
receipt digest `61cee44c09345ef812c90aca9696934ec2e917cd121e5e09ba54cf4a91eeadcf`).
Accordingly, no Octocode lane was run as a valid benchmark. Earlier workspace-CLI
`--version` and schema/catalog probes were preflight-only and are invalidated by this
freshness result.

## Raw diagnostic smoke lanes (not comparable performance data)

Commands, from the locked LangChain clone:

```sh
rg --json -n -g '**/*.py' '^(class|type) RunnableConfig\\b' libs/core
ast-grep run -p 'async def ainvoke(self, $$$ARGS): $$$BODY' -l python --json=stream libs/core
ast-grep run -p 'async def ainvoke(self, $$$ARGS) -> $RET: $$$BODY' -l python --json=stream libs/core
```

Results:

- Raw ripgrep 15.2.0 returned one p01 declaration: `libs/core/langchain_core/runnables/config.py:57`.
- Raw ast-grep 0.45.0 returned zero matches for the checked-in p05 oracle pattern, including zero in known file `libs/core/langchain_core/tools/base.py`.
- The return-annotation-aware variant returned 24 `ainvoke(self, ...)` methods (first five at
  `tools/simple.py:45`, `tools/structured.py:115`, `prompts/base.py:239`,
  `tools/base.py:767`, and `retrievers.py:237`).

This exposes a public-suite defect: p05's exact ast-grep pattern omits the return annotation
and therefore produces an empty oracle despite its prompt asking to enumerate those methods.
Do not use p05 to grade an arm until its anchor is re-frozen against the locked corpus between
experiments; do not silently change it during a campaign.

## Next valid run

Use a Linux cgroup-v2 host with native raw baselines and self-hosted Sourcegraph, seal the
private envelopes, authorize/build the local workspace dependency chain, run `prepare`,
materialize public oracles from the frozen corpus lock, then run three blind passes with fresh
Terra contexts. Re-check p05 before admitting it to that frozen suite.
