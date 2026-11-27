Load when attaching LLVM control-flow or dominator graphs (`llvm-graphviz`) and tracing basic blocks.

# LLVM control flow and dominance

## LLVM control flow and dominance

Compile a translation unit to LLVM IR with block names. Run the two LLVM passes on the **same** IR, then convert their DOT outputs to Graphviz JSON. For a function named `choose`:

```sh
clang -S -emit-llvm -fno-discard-value-names -O0 choose.c -o choose.ll
opt -passes='dot-cfg-only,dot-dom-only' -disable-output choose.ll
dot -Tjson0 .choose.dot -o cfg.json
dot -Tjson0 domonly.choose.dot -o dom.json
```

Pass each JSON file as `--provider llvm-graphviz <file.json> --module-file choose.ll`: the manifest records the module's `moduleSha256` beside the file's digest. The importer qualifies each block by module digest, function name and block name. The control-flow and dominator printers may assign different Graphviz node IDs.

From `llvm:<moduleSha256>`, trace `DEFINES CONTAINS` to list block IDs. From a block, trace `BRANCHES_TO+` for possible successor blocks or `IMMEDIATELY_DOMINATES+` for blocks it strictly dominates. Edges cite the supplying graph artifact. These are compiler basic blocks for the supplied IR and optimization settings. Instruction-level data flow requires separate facts.
