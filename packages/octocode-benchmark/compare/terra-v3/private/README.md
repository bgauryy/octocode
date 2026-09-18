# Terra v3 private-suite custody

This directory contains the public verification half of the independently curated Terra v3
held-out suite. The private prompts, deterministic anchors, and answer material are not
stored in the repository. They are encrypted into
[`../suite/private-envelopes.cms`](../suite/private-envelopes.cms), while
[`../suite/private-manifest.json`](../suite/private-manifest.json) exposes only opaque case
IDs, taxonomy lanes, repository scope, and SHA-256 commitments.

The bundle is bound to these exact corpus commits:

- `langchain-ai/langchain@67ee6cb63dd9ae7f3a4dfedc3095652bce15a125`
- `vercel/next.js@d155ba9ebfffe4742efefda8d68c2e0e8e490924`

The checked-in certificate verifies the curator signature and is the CMS encryption
recipient. The private key must remain outside the repository and outside any model context
used to implement or tune a candidate.

## Lifecycle vocabulary

The manifest keeps four states separate:

- `curated=complete`: the 20 cases and deterministic anchors were independently authored and
  materialized against the pinned source bytes.
- `sealed=complete`: each canonical case envelope has a SHA-256 commitment, the complete
  envelope set is CMS-encrypted, and the public manifest is signed.
- `ready=private-suite-ready`: this private artifact is internally usable. It does **not**
  imply that public oracles, the host, arms, receipts, or the overall campaign are ready.
- `executed=not-executed`: no provider/model run or benchmark result is represented by this
  artifact.

Do not change `executed` during curation. Execution state belongs in a validator-backed
campaign artifact with real provider and sensor receipts.

## Public verification

Anyone can verify the signature, ciphertext digest, hash-only shape, case count, and
lifecycle without decrypting the cases:

```bash
python3 packages/octocode-benchmark/compare/terra-v3/private/curator_bundle.py verify \
  --manifest packages/octocode-benchmark/compare/terra-v3/suite/private-manifest.json \
  --bundle packages/octocode-benchmark/compare/terra-v3/suite/private-envelopes.cms \
  --certificate packages/octocode-benchmark/compare/terra-v3/private/curator-cert.pem
```

This proves integrity and curator provenance relative to the checked-in certificate. It does
not prove case quality, campaign readiness, or execution.

## Curator-only materialization

On the custody host, decrypt into an ignored `tmp/` directory immediately before the frozen
campaign:

```bash
python3 packages/octocode-benchmark/compare/terra-v3/private/curator_bundle.py materialize \
  --manifest packages/octocode-benchmark/compare/terra-v3/suite/private-manifest.json \
  --bundle packages/octocode-benchmark/compare/terra-v3/suite/private-envelopes.cms \
  --certificate packages/octocode-benchmark/compare/terra-v3/private/curator-cert.pem \
  --private-key /absolute/private/custody/curator-key.pem \
  --output packages/octocode-benchmark/compare/terra-v3/tmp/private-cases.json
```

Materialization verifies the signed manifest, decrypts with the private key, validates the
pinned commits, opaque IDs, lane distribution, and pre-execution lifecycle, then recomputes
all 20 envelope commitments. A wrong key, changed ciphertext, changed manifest, or changed
plaintext fails closed.

Only the isolated runner receives one decrypted prompt and its permitted grading material at
a time. Candidate authors and runner contexts must never receive the private key, the full
decrypted suite, another case's anchor, or a reference answer. Remove the materialized file
after the campaign; retain the encrypted bundle, manifest, campaign receipts, and curator
custody copy.

## Resealing and versioning

Resealing is a curator-only operation. Start from a curator-held JSON source whose lifecycle
is exactly `curated=complete`, `sealed=pending`, `ready=pending`, and
`executed=not-executed`, then run:

```bash
python3 packages/octocode-benchmark/compare/terra-v3/private/curator_bundle.py seal \
  --source /absolute/private/custody/source.json \
  --manifest packages/octocode-benchmark/compare/terra-v3/suite/private-manifest.json \
  --bundle packages/octocode-benchmark/compare/terra-v3/suite/private-envelopes.cms \
  --certificate packages/octocode-benchmark/compare/terra-v3/private/curator-cert.pem \
  --signing-key /absolute/private/custody/curator-key.pem
```

Never reseal during a campaign. A case, anchor, normalizer, commitment scheme, repository
commit, certificate, or grader correction creates a new suite version and invalidates partial
results from the previous version.

## Deterministic checks

```bash
python3 -m unittest discover \
  -s packages/octocode-benchmark/compare/terra-v3/private \
  -p 'test_*.py' -v
```

The tests use ephemeral keys and synthetic fixture cases. They never reveal or execute the
held-out cases and never fabricate provider receipts or benchmark results.
