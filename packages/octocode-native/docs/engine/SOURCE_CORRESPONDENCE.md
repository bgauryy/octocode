# Source correspondence

Structured AST and graph evidence can identify both the text an analyzer saw and the original bytes that contributed to it. Read `sourceOrigin` together with the response's `sourceEvidence`; a byte range alone does not identify a physical file.

## Coordinate spaces

| Record | Identifies | Coordinates |
|---|---|---|
| `sourceEvidence.snapshots` | An admitted physical source occurrence, its raw digest, byte length and encoding | Original file bytes, including any byte-order mark |
| `sourceEvidence.representations` | Immutable UTF-8 text supplied to an analyzer, its digest and producer version | UTF-8 bytes in that representation |
| `sourceEvidence.units` | An operation and adapter applied to a representation and its analysis regions | Regions within the representation |
| `sourceOrigin` | How one observed span in an analysis unit corresponds to original sources | Ordered representation segments with source branches |

`sourceRange` and `sourceDigest` identify the observed representation. Follow `sourceOrigin.analysisUnitId` to its unit, then `representationId` to the representation. A branch's `origin.sourceId` or source contributor points to a snapshot; its `rawRange` addresses that snapshot's original bytes. Ranges are half-open: `startByte` is included and `endByte` is excluded. Nonempty Unicode ranges end at scalar boundaries; zero-width anchors remain explicit.

A source occurrence includes its canonical location as well as content. Equal bytes in different repositories do not identify the same occurrence. Representations and units also include their producer or adapter identities. These IDs identify analysis evidence, not semantic language-server symbols.

Source decoding admits UTF-8, including a UTF-8 BOM, and BOM-marked UTF-16 little- or big-endian input. Malformed Unicode and binary input are refused; decoding never substitutes replacement characters. UTF-8 representation offsets exclude a stripped BOM and differ from raw UTF-16 offsets. Inspect `sourceDecoding` in `octocode schema --view full` for the running build's contract.

## Read every branch

Each origin segment describes one contiguous portion of the observed representation. Its ordered `branches` are alternative origins covering that whole segment; adjacent segments are concatenated. Branches mean:

- `copied`: bytes were copied from the recorded original range.
- `transcoded`: the same Unicode text was decoded from a different source encoding.
- `derived`: the ordered `contributors` jointly produced the text. A contributor can name source bytes or an explicitly unmapped contribution; a derived range is not an exact copy.
- `unmapped`: correspondence is unavailable, with a reason such as synthetic text or unavailable raw source.

Order and repeated contributors are evidence. Do not deduplicate them, flatten alternatives into a single range, or interpret a derived contributor as an exact replacement for the output text. `analysis: "excluded"` records text outside the unit's selected regions. It does not make that text disappear from source coordinates.

Region adapters can reparse macro bodies or supply parser context. Their matches, captures and enclosing observations retain the unit of the tree they came from. An inner match and an outer captured declaration can therefore refer to different units and representations in the same result. Synthetic parser context is not presented as copied file bytes. The installed grammar's `regionAdapters` describes available adapters; it does not promise support for every embedded language or generated file.

## Result boundaries

AST match, syntax-tree and declaration rows emit correspondence for observed spans. Captures have their own range, digest and origin. A displayed declaration header can carry paired `headerSourceRange` and `headerOrigin`; the range addresses the observed prefix, never the appended ellipsis or protected display text. Selection and recipient ranges likewise use their own paired origins.

Graph queries use the provenance captured at graph creation. Reading an old graph after editing or deleting source files preserves that graph's origins and separately reports freshness. The store does not reopen current files to invent origins for an older snapshot. Structural relationships that have no exact observed span retain their explicit absent/null evidence rather than borrowing another occurrence's range.

Ordinary text rows, remote provider results and rendered `localFetch` outlines need not carry byte origins. Line/column positions alone do not prove raw-byte correspondence. A rendered or redacted outline is not a new source representation with guessed mappings.

## Paging, limits and reuse

Each emitted origin remains atomic, with its complete descriptor closure in `sourceEvidence`. Result rows can page; origin segments, branches and contributors are never silently shortened to fit a response. Each response fragment includes only the descriptors referenced by its retained origins, including header, selection, recipient and blocker origins. Shared descriptors count once when packing that fragment; descriptor records retain their original order. Follow the returned continuation unchanged. Mapping-budget exhaustion fails explicitly with `scopeTooBroad`. An atomic origin or required descriptor closure that exceeds the response projection budget fails with `responseTooLarge`. Neither failure returns an apparently complete partial origin.

Mapping checks cancellation and deadlines and bounds retained mapping state, nesting and work. These limits govern the correspondence machinery, not total process RSS or every parser allocation. The graph reader's remaining scaling limits are documented in [Code graph architecture](CODE_GRAPH.md#choosing-a-query).

Parser caches retain immutable analysis templates. Each raw source admission binds those templates to the bytes actually read, so changing a BOM or encoding cannot reuse an earlier physical mapping merely because the decoded text is unchanged. Graph fact caches and reusable snapshots include provider/capability identity; changing the installed analysis configuration invalidates incompatible reuse.

Core owns the public schema and meanings. `engine::text::analysis` owns admission, units, maps and composition; runtime `response::source_evidence` projects validated engine evidence through generated wire types. The [language matrix](SUPPORTED_LANGUAGES_AND_FEATURES.md) owns installed grammar and operation support.
