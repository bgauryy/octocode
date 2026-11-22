# Attach measurements to a saved graph

Run `octocode-graph enrich <data.json> <manifest.json> <new-data.json>` from an installed graph package. The new export carries the original graph and a separate evidence layer. Query it with `octocode-graph-query <new-data.json> "<query>" --json`.

Choose artifacts collected from the same source revision and build configuration. Set the manifest's `baseSha256` to the digest of the exact graph export and each provider's `sha256` to the digest of its original bytes. Obtain them with `shasum -a 256 <file>`. The importer checks both digests and the revision labels. The producer attests which revision the files describe.

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

| Artifact | What can be queried |
| --- | --- |
| `scip-json` | A decoded SCIP Index in protobuf JSON form. Document occurrences yield file-to-symbol `DEFINES`, `REFERENCES`, `READS` and `WRITES`. Document symbol relationships yield `IMPLEMENTS`, `REFERENCES`, `DEFINITION` and `TYPE_DEFINITION`. Document-local identifiers include the file name. Binary SCIP needs decoding first. |
| `llvm-cov` | Use `llvm-cov export` for one test per input. Set `testId`. The query `trace test:addition COVERS` returns files with positive, counted, non-gap segments. Each witness retains the segment coordinates and counts. |
| `otlp-json` | Supply OTLP/JSON trace export requests with `resourceSpans` and a `workload` name. `trace workload:checkout OBSERVED PARENT_OF EXECUTES_FILE` follows an observed parent to a child with a source-file attribute. Elapsed nanoseconds remain decimal strings. Spans without source attributes can still establish parentage. |

The result's `steps` carry the artifact name, relationship and available source coordinates. Test coverage describes the measured test. Runtime observations describe the supplied workload. Missing files, uninstrumented spans and unindexed documents limit conclusions about absence.

## LLVM control flow and dominance

Compile a translation unit to LLVM IR with block names. Run the two LLVM passes on the **same** IR, then convert their DOT outputs to Graphviz JSON. For a function named `choose`:

```sh
clang -S -emit-llvm -fno-discard-value-names -O0 choose.c -o choose.ll
opt -passes='dot-cfg-only,dot-dom-only' -disable-output choose.ll
dot -Tjson0 .choose.dot -o cfg.json
dot -Tjson0 domonly.choose.dot -o dom.json
```

Add one provider entry per JSON file with `format` set to `llvm-graphviz`, its file digest, revision, `moduleFile` pointing to `choose.ll` and `moduleSha256` holding that file's digest. The importer qualifies each block by module digest, function name and block name. The control-flow and dominator printers may assign different Graphviz node IDs.

From `llvm:<moduleSha256>`, trace `DEFINES CONTAINS` to list block IDs. From a block, trace `BRANCHES_TO+` for possible successor blocks or `IMMEDIATELY_DOMINATES+` for blocks it strictly dominates. Edges cite the supplying graph artifact. These are compiler basic blocks for the supplied IR and optimization settings. Instruction-level data flow requires separate facts.
