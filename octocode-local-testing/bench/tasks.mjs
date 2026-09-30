// Benchmark task definitions with ground truth fixed in advance.
// PR tasks: every ground-truth (GT) file has marker strings that must appear
// in the text an arm actually saw for that file. Local tasks: same, keyed by
// repo-relative path. Symbol-task usage lists live in ground-truth.json
// (frozen by `bench.mjs --freeze-gt`, re-verified on every run).

export const PR_TASKS = [
  {
    id: 'ts-51387',
    owner: 'microsoft', repo: 'TypeScript', number: 51387,
    shape: '656 files, merged; checker.ts and other huge files have no patch',
    question: 'This PR converts TypeScript to modules. Which build files switch the bundler to esbuild? Show the lines that add, import, or invoke esbuild.',
    // Candidate filter an agent can apply from paths alone (build/tooling files, not compiler sources or baselines).
    candidate: (p) => !/^(src|tests)\//.test(p),
    // gh lean: files whose patch matches, printing only matching lines.
    leanFileRe: 'esbuild', leanLineRe: 'esbuild',
    // octocode without clasify: literal filter on patch windows.
    matchString: 'esbuild',
    direct: [{ content: { patches: { mode: 'all' } }, matchString: 'esbuild', matchContext: 0 }],
    // octocode + clasify: semantic screen per changed file.
    semantic: 'This patch makes the build use the esbuild bundler (adds, imports, or invokes esbuild).',
    gt: {
      'Herebyfile.mjs': ['import esbuild from "esbuild";', 'esbuild.build('],
      'package.json': ['"esbuild": "^0.15.13"'],
    },
  },
  {
    id: 'rust-157558',
    owner: 'rust-lang', repo: 'rust', number: 157558,
    shape: '429 files, rollup of 25 PRs, 7 renames (3 without a patch)',
    question: 'In this rollup, which crates rename their `errors` module to `diagnostics`? List the renamed files (old -> new path) and show the crate-root `mod` declaration change.',
    candidate: (p, f) => f?.status === 'renamed' || /\/src\/lib\.rs$/.test(p),
    leanFileRe: null, // custom select below
    leanSelect: '(.status == "renamed") or ((.filename | endswith("/lib.rs")) and ((.patch // "") | test("mod (errors|diagnostics);")))',
    leanLineRe: '^[-+].*mod (errors|diagnostics);',
    matchString: 'mod diagnostics',
    direct: [
      { content: { changedFiles: true }, fileFilter: { status: ['renamed'] } },
      { content: { patches: { mode: 'all' } }, fileFilter: { paths: ['**/src/lib.rs'] }, matchString: 'mod diagnostics', matchContext: 0 },
    ],
    semantic: "This patch renames the crate's `errors` module to `diagnostics` (changes the `mod` declaration).",
    gt: {
      'compiler/rustc_ast_lowering/src/diagnostics.rs': ['compiler/rustc_ast_lowering/src/errors.rs'],
      'compiler/rustc_ast_passes/src/diagnostics.rs': ['compiler/rustc_ast_passes/src/errors.rs'],
      'compiler/rustc_builtin_macros/src/diagnostics.rs': ['compiler/rustc_builtin_macros/src/errors.rs'],
      'compiler/rustc_ast_lowering/src/lib.rs': ['+mod diagnostics;'],
      'compiler/rustc_ast_passes/src/lib.rs': ['+mod diagnostics;'],
      'compiler/rustc_builtin_macros/src/lib.rs': ['+mod diagnostics;'],
    },
    // Renamed files carry no patch; the rename itself is the evidence.
    metaOnly: ['compiler/rustc_ast_lowering/src/diagnostics.rs', 'compiler/rustc_ast_passes/src/diagnostics.rs', 'compiler/rustc_builtin_macros/src/diagnostics.rs'],
  },
  {
    id: 'ts-61986',
    owner: 'microsoft', repo: 'TypeScript', number: 61986,
    shape: '12 files, 22k changed lines; dom.generated.d.ts (and 3 others) have no patch from GitHub',
    question: "This DOM lib update changes the 'click' event type from MouseEvent to PointerEvent. Which files show that change? Show the changed lines, including the lib declaration.",
    candidate: () => true,
    leanFileRe: 'PointerEvent', leanLineRe: '^[-+].*(MouseEvent|PointerEvent)',
    matchString: 'PointerEvent',
    direct: [
      { content: { patches: { mode: 'all' } }, matchString: 'PointerEvent', matchContext: 0 },
      { content: { changedFiles: true }, fileFilter: { paths: ['src/lib/**'] } },
    ],
    semantic: "This patch changes a click event handler's event type from MouseEvent to PointerEvent.",
    // Files without a patch that the task makes relevant: the DOM lib sources.
    followup: {
      filter: (p) => /^src\/lib\/.*\.generated\.d\.ts$/.test(p),
      grep: '"click": ',
      prefilter: ['"click"'],
      locate: "Where is the event type for the 'click' event declared in the element event map?",
    },
    gt: {
      'src/lib/dom.generated.d.ts': ['"click": PointerEvent'],
      'tests/baselines/reference/correlatedUnions.js': ['callback: (ev: PointerEvent) => void'],
      'tests/baselines/reference/correlatedUnions.types': ['callback: (ev: PointerEvent) => void'],
      'tests/baselines/reference/reverseMappedTypeContextualTypesPerElementOfTupleConstraint.types': ['listener: (event: PointerEvent) => void'],
    },
  },
  {
    id: 'tokio-8156',
    owner: 'tokio-rs', repo: 'tokio', number: 8156,
    shape: '37 files, medium review',
    question: 'This PR enables Miri for TCP tests. Which tests stay disabled under Miri with a stated reason? Show the `cfg_attr(miri, ignore = "...")` lines.',
    candidate: () => true,
    leanFileRe: 'miri, ignore = "', leanLineRe: 'miri, ignore = "',
    matchString: 'miri, ignore = "',
    direct: [{ content: { patches: { mode: 'all' } }, matchString: 'miri, ignore = "', matchContext: 0 }],
    semantic: 'This patch keeps a test ignored under Miri and states the reason in the ignore attribute.',
    gt: {
      'tokio/tests/tcp_shutdown.rs': ['#[cfg_attr(miri, ignore = "Miri doesn\'t support `SO_LINGER`")]'],
      'tokio/tests/tcp_socket.rs': ['ignore = "Miri doesn\'t support binding before connecting"', 'ignore = "Miri only supports `TCP_NODELAY` on connected sockets"'],
      'tokio/tests/tcp_stream.rs': ['#[cfg_attr(miri, ignore = "Miri doesn\'t support `SO_LINGER`")]'],
    },
  },
];

// kind: symbol = definition + usages; how = "how does X work" across 2-3 files;
// unknown = behavior/config whose symbol name is not given.
// pattern/ci/exclude are the discovery search every arm starts from.
export const LOCAL_TASKS = [
  // ---- typescript (excalidraw, .ts/.tsx) ----
  { id: 'ts-symbol', repo: 'tsx', lang: 'typescript', kind: 'symbol', path: 'packages',
    question: 'Where is newElementWith defined, and which files use it?',
    symbol: 'newElementWith', defPattern: 'export const newElementWith\\b',
    gt: { 'packages/element/src/mutateElement.ts': ['export const newElementWith = <TElement extends ExcalidrawElement>('] } },
  { id: 'ts-how', repo: 'tsx', lang: 'typescript', kind: 'how', path: 'packages', exclude: ['*.test.*', '**/tests/**'],
    question: 'How does undo work: where is a history entry applied, and where does a store delta apply its element changes?',
    pattern: 'applyTo\\(',
    questions: ['Where does History undo/redo apply a history entry delta to the elements?', 'Where does a store delta apply its element changes to the scene elements?'],
    gt: { 'packages/excalidraw/history.ts': ['historyDelta.applyTo('], 'packages/element/src/store.ts': ['delta.elements.applyTo('] } },
  { id: 'ts-unknown', repo: 'tsx', lang: 'typescript', kind: 'unknown', path: 'packages', exclude: ['*.test.*', '**/tests/**'],
    question: 'How close (in pixels, before zoom) must an element be to snap to another, and where is that set?',
    pattern: 'snap.?(distance|threshold)', ci: true,
    questions: ['Where is the base snapping distance constant (in pixels) defined?'],
    gt: { 'packages/excalidraw/snapping.ts': ['const SNAP_DISTANCE = 8;'] } },

  // ---- rust (tokio) ----
  { id: 'rust-symbol', repo: 'rust', lang: 'rust', kind: 'symbol', path: 'tokio/src',
    question: 'Where is the free function tokio::task::spawn_blocking defined, and which files reference spawn_blocking?',
    symbol: 'spawn_blocking', defPattern: 'pub fn spawn_blocking<',
    gt: { 'tokio/src/task/blocking.rs': ['pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>'] } },
  { id: 'rust-how', repo: 'rust', lang: 'rust', kind: 'how', path: 'tokio/src/sync/mpsc',
    question: 'How does the bounded mpsc channel apply backpressure: where does a sender wait for capacity, and where is capacity returned after a receive?',
    pattern: 'acquire|add_permit',
    questions: ['Where does the bounded sender wait for a semaphore permit (capacity) before sending?', 'Where does the receiver return a permit to the semaphore after taking a value?'],
    gt: { 'tokio/src/sync/mpsc/bounded.rs': ['semaphore.acquire(n).await'], 'tokio/src/sync/mpsc/chan.rs': ['self.inner.semaphore.add_permit();'] } },
  { id: 'rust-unknown', repo: 'rust', lang: 'rust', kind: 'unknown', path: 'tokio/src/runtime',
    question: 'How many threads can the blocking pool spawn by default, and where is that default set?',
    pattern: 'blocking.?threads', ci: true,
    questions: ['Where is the default maximum number of blocking-pool threads set?'],
    gt: { 'tokio/src/runtime/builder.rs': ['max_blocking_threads: 512'] } },

  // ---- go (prometheus) ----
  { id: 'go-symbol', repo: 'go', lang: 'go', kind: 'symbol', path: '.',
    question: 'Where is promql.NewEngine defined, and which files reference it?',
    symbol: 'NewEngine', defPattern: '^func NewEngine\\(',
    gt: { 'promql/engine.go': ['func NewEngine(opts EngineOpts) *Engine {'] } },
  { id: 'go-how', repo: 'go', lang: 'go', kind: 'how', path: 'scrape', exclude: ['*_test.go'],
    question: 'How does a scrape enforce sample_limit: where is the sample count checked against the limit, and where does the scrape loop handle that error?',
    pattern: 'sample.?limit', ci: true,
    questions: ['Where is the per-scrape sample count compared against the limit and an error returned?', 'Where does the scrape loop recognize the sample-limit error while appending?'],
    gt: { 'scrape/target.go': ['return 0, errSampleLimit'], 'scrape/scrape.go': ['case errors.Is(err, errSampleLimit)'] } },
  { id: 'go-unknown', repo: 'go', lang: 'go', kind: 'unknown', path: 'promql', exclude: ['*_test.go'],
    question: 'How far back does a PromQL query look for the latest sample by default, and where is that set?',
    pattern: 'lookback', ci: true,
    questions: ['Where is the default lookback delta duration defined?'],
    gt: { 'promql/engine.go': ['defaultLookbackDelta = 5 * time.Minute'] } },

  // ---- python (django) ----
  { id: 'py-symbol', repo: 'python', lang: 'python', kind: 'symbol', path: '.',
    question: 'Where is get_object_or_404 defined, and which files reference it?',
    symbol: 'get_object_or_404', defPattern: '^def get_object_or_404\\(',
    gt: { 'django/shortcuts.py': ['def get_object_or_404(klass, *args, **kwargs):'] } },
  { id: 'py-how', repo: 'python', lang: 'python', kind: 'how', path: 'django/db/models',
    question: 'How does QuerySet.get() signal zero or multiple matches: where is the error raised, and where is the per-model exception class created?',
    pattern: 'DoesNotExist|MultipleObjectsReturned',
    questions: ['Where does QuerySet.get() raise when multiple rows match?', 'Where is the model-specific DoesNotExist exception class created for each model?'],
    gt: { 'django/db/models/query.py': ['raise self.model.MultipleObjectsReturned('], 'django/db/models/base.py': ['subclass_exception(', '"DoesNotExist"'] } },
  { id: 'py-unknown', repo: 'python', lang: 'python', kind: 'unknown', path: 'django',
    question: 'Where does Django reject a request that submits too many form fields, and what is the default limit?',
    pattern: 'too.?many.?fields|max.?number.?fields', ci: true,
    questions: ['Where is the default maximum number of request fields set?', 'Where is a request with too many fields rejected?'],
    gt: { 'django/conf/global_settings.py': ['DATA_UPLOAD_MAX_NUMBER_FIELDS = 1000'], 'django/http/request.py': ['raise TooManyFieldsSent('], 'django/http/multipartparser.py': ['raise TooManyFieldsSent('] } },

  // ---- java (guava) ----
  { id: 'java-symbol', repo: 'java', lang: 'java', kind: 'symbol', path: 'guava/src',
    question: 'Where is MoreObjects.firstNonNull defined, and which files reference it?',
    symbol: 'firstNonNull', defPattern: 'public static <T> T firstNonNull',
    gt: { 'guava/src/com/google/common/base/MoreObjects.java': ['public static <T> T firstNonNull(@Nullable T first, @Nullable T second) {'] } },
  { id: 'java-how', repo: 'java', lang: 'java', kind: 'how', path: 'guava/src/com/google/common/cache',
    question: 'How does a size-bounded Guava cache evict: where is the maximum size stored, and where are entries evicted while over the limit?',
    pattern: 'maximumSize|evictEntries',
    questions: ['Where does CacheBuilder store the configured maximum size?', 'Where does the cache segment evict entries while its total weight exceeds the limit?'],
    gt: { 'guava/src/com/google/common/cache/CacheBuilder.java': ['this.maximumSize = maximumSize;'], 'guava/src/com/google/common/cache/LocalCache.java': ['while (totalWeight > maxSegmentWeight) {'] } },
  { id: 'java-unknown', repo: 'java', lang: 'java', kind: 'unknown', path: 'guava/src/com/google/common/cache',
    question: 'How many segments does a CacheBuilder cache use by default, and where is that default defined?',
    pattern: 'concurrency.?level', ci: true,
    questions: ['Where is the default concurrency level constant defined?'],
    gt: { 'guava/src/com/google/common/cache/CacheBuilder.java': ['DEFAULT_CONCURRENCY_LEVEL = 4'] } },
];
