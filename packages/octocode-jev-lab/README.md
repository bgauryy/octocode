# @octocodeai/jev-lab

Private development probe for sending a small semantic experiment directly to
the TypeSafe System One API. It is intentionally separate from Octocode's
runtime adapter so provider behavior and latency can be measured without a
build.

```bash
yarn jev:probe --input packages/octocode-jev-lab/examples/multi-file.json
```

Add `--repeat 5 --concurrency 2` for a small latency sample, or `--compact` for
one-line JSON. `--input -` reads a manifest from stdin.

The probe uses the same trusted global Octocode environment loader as the rest
of the workspace. A shell `OCTOCODE_JEV_KEY` wins; otherwise the key may come
from the global Octocode `.env`. Project `.env` files are intentionally ignored.

## Manifest

Use `state` to send provider-ready JSON unchanged:

```json
{
  "state": { "facts": ["one", "two"] },
  "questions": {
    "supported": { "type": "noul", "instructions": "Is the claim supported?" }
  }
}
```

Or use `resources` to load several UTF-8 files into one structured state. Paths
are resolved relative to the manifest and are not sent to the provider.

```json
{
  "resources": [
    { "id": "contract", "path": "./contract.md" },
    { "id": "notes", "value": { "status": "draft" } }
  ],
  "questions": {
    "route": {
      "type": "choice",
      "instructions": "Which action fits?",
      "criteria": { "keep": "Ready", "revise": "Needs changes" }
    }
  }
}
```

Each resource is capped at 80,000 characters. Split larger inputs explicitly so
page boundaries remain visible in the experiment.

The output keeps every provider payload unchanged in `samples[].response`.
Local receipts, latency, HTTP status, and aggregate token counts are adjacent
metadata; they never replace or reinterpret Noul, Choice, or Score fields.
