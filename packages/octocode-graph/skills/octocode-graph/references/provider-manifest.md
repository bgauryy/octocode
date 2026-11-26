Load when writing or debugging the `enrich` manifest: provider options, digests, revision labels, source roots.

# Write the enrich manifest

## Write the manifest

Choose artifacts collected from the same source revision and build configuration, then write the manifest with one `--provider` per artifact; each option after a `--provider` belongs to it:

```sh
octocode-graph enrich --manifest-from data.json manifest.json --revision build-42 \
  --provider llvm-cov coverage.json --test-id addition --source-root /build/project
```

It records the SHA-256 of the export (`baseSha256`) and of each file (`sha256`) as they are on disk, names each file relative to the manifest, refuses to replace an existing manifest, and prints the `enrich` command to run next. Provider options: `--test-id`, `--workload`, `--source-root`, `--working-directory`, `--module-file`. The importer checks both digests and the revision labels. The producer attests which revision the files describe. The manifest it writes:

```json
{
  "revision": "build-42",
  "baseSha256": "<digest of data.json>",
  "providers": [
    {
      "format": "llvm-cov",
      "file": "coverage.json",
      "sha256": "<digest of coverage.json>",
      "revision": "build-42",
      "sourceRoot": "/build/project",
      "testId": "addition"
    }
  ]
}
```

Artifact paths resolve relative to the manifest. Set `sourceRoot` to the absolute repository root on the machine that produced the measurement if it differs from the export's `section`. Keep separate output names for separate manifests. The importer refuses to replace an existing output.
