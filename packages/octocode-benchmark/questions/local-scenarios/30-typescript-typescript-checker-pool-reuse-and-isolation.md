# TypeScript checker-pool reuse and isolation

## Goal

Investigate the following scenario: The TypeScript checkout reuses checker instances across compilations with different options or file graphs.

## Questions

- How does the TypeScript checkout reuse checker instances across compilations?
- Which state depends on program identity, compiler options, or file graphs?
- Can reuse leak diagnostics or symbols between compilations, and what correctness and performance trade-offs govern the design?
