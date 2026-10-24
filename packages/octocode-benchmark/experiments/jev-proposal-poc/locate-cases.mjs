import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { hash, records } from '../../local/e2e/harness/util.mjs';

// Development cases, not a holdout. The snapshots are unmodified contiguous
// source; existing comments/tests remain. Gold names production statements only.
// Every direct-search literal also appears verbatim in the broad question, so
// the direct route receives no extra identifier or answer-derived search term.
// Snapshot hashes deliberately fail closed when source changes: review the gold
// before updating a hash, and preserve earlier run snapshots for comparisons.
const definitions = [
  {
    id: 'process-cancellation-and-output',
    path: 'tools/bounded_process.rs', startLine: 1, endLine: 178,
    sha256: '190608e512491fc5966eea0809e48fc11b801bd15884c5b5f5e39643e4f871af',
    broad: 'How does run_bounded respond to cancel.check() failure while waiting and to stdout exceeding max_stdout?',
    atoms: [
      {
        ask: 'What result does the waiting loop in run_bounded produce when cancel.check() reports cancellation?',
        evidence: 'The waiting-loop cancellation result.',
        gold: [85, 86],
      },
      {
        ask: 'How does run_bounded respond when captured stdout exceeds max_stdout?',
        evidence: 'The captured-output overflow result.',
        gold: [121, 122, 123, 124],
      },
    ],
    direct: { matchString: ['cancel.check()', 'max_stdout'], contextLines: 3 },
  },
  {
    id: 'directory-read-priority',
    path: 'tools/root_read.rs', startLine: 1, endLine: 119,
    sha256: '2fe1c8cda9e5f90a8c7b0a5d094740e16bb9979a8e001df6b32955f3fe1175a1',
    broad: 'How does read_rank prioritize a source filename mentioned in words, and how does depth affect its readme lead?',
    atoms: [
      {
        ask: 'Which files receive the highest read_rank priority when words names a file stem?',
        evidence: 'The eligibility and priority of an explicitly named source stem.',
        gold: [20, 21, 22, 23, 24, 25, 26, 27],
      },
      {
        ask: 'When can read_rank assign its README read lead, considering directory depth and filename matching?',
        evidence: 'The directory-depth and filename conditions for a README lead.',
        gold: [36, 40, 41],
      },
    ],
    direct: { matchString: ['words', 'depth', 'readme'], contextLines: 3 },
  },
  {
    id: 'repository-scope-and-archives',
    path: 'tools/gh_search_repo/query.rs', startLine: 1, endLine: 139,
    sha256: 'c6613dc3d6de0c59356a4b07314804f356eaf76d06a0ecf710978e9f88f4eb6b',
    broad: 'How does repositories scope an owner with or without repo, and how do include_archived and archive_qualifier control its default archive filter?',
    atoms: [
      {
        ask: 'How does repositories scope its query when both owner and repo are provided, compared with only owner?',
        evidence: 'The owner-and-repository scope branch and its fallback.',
        gold: [26, 27, 29, 30, 32],
      },
      {
        ask: 'Under what conditions does repositories add its default archive exclusion, using include_archived and archive_qualifier?',
        evidence: 'The archive-qualifier recognition and default-filter condition.',
        gold: [63, 65, 66, 68, 69],
      },
    ],
    direct: { matchString: ['owner', 'include_archived', 'archive_qualifier'], contextLines: 3 },
  },
  {
    id: 'ast-scan-continuation',
    path: 'tools/ast_search/scan_window.rs', startLine: 1, endLine: 160,
    sha256: '7377a828a2f518feb6a11b048d5a34548da4261d88c65788f27d32f81f6e99e3',
    broad: 'How does expand_scan use truncated and max_files to decide whether to emit a ScanWindow and choose its scan_offset?',
    atoms: [
      {
        ask: 'How does expand_scan handle a scan whose truncated flag is false?',
        evidence: 'The completed-scan continuation decision.',
        gold: [45, 46],
      },
      {
        ask: 'For a truncated scan with an explicit max_files limit, how does expand_scan decide and construct the next ScanWindow?',
        evidence: 'The explicit-limit admission and next-window calculation.',
        gold: [48, 56, 57, 58, 59, 61],
      },
    ],
    direct: { matchString: ['truncated', 'max_files', 'scan_offset'], contextLines: 3 },
  },
  {
    id: 'cursor-advance-and-reason-deduplication',
    path: 'tools/paging.rs', startLine: 1, endLine: 125,
    sha256: '906d9efe88911d1f7cb33f29584a0a61adf6ccf2ef0f4e5b55e243af326df7a8',
    broad: 'How does admitted_query use query_schema_max when advancing current and applying resets, and how does mark_partial avoid a duplicate reason in partialReasons?',
    atoms: [
      {
        ask: 'What conditions allow admitted_query to advance current and apply the requested resets?',
        evidence: 'The cursor admission, checked advance, and dependent reset updates.',
        gold: [23, 36, 38, 39, 40, 41],
      },
      {
        ask: 'How does mark_partial handle a reason that already appears in an object row\'s partialReasons array?',
        evidence: 'The existing-reason check before changing the partial-reason list.',
        gold: [77, 78, 79, 80, 81, 83],
      },
    ],
    direct: { matchString: ['query_schema_max', 'current', 'resets', 'reason', 'partialReasons'], contextLines: 3 },
  },
  {
    id: 'import-absence-proof',
    path: 'tools/lsp_search/import_stop.rs', startLine: 1, endLine: 161,
    sha256: 'ff5452475351ed53760e9d43e54247106b64b105680a27f5f2037d76adc6183d',
    broad: 'How does proven_missing interpret PathPolicy.validate errors, and which specifier forms can resolves_nowhere check through module_candidates?',
    atoms: [
      {
        ask: 'Which PathPolicy validation failures does proven_missing accept as proof that a path is absent?',
        evidence: 'The policy-error classification used as absence evidence.',
        gold: [24, 25],
      },
      {
        ask: 'Under which conditions can resolves_nowhere conclude that a module specifier names no file?',
        evidence: 'Specifier admissibility, containing-directory availability, and candidate-missing aggregation.',
        gold: [126, 127, 128, 129, 131, 133, 134, 136, 137],
      },
    ],
    direct: { matchString: ['validate', 'specifier', 'module_candidates'], contextLines: 3 },
  },
];

export async function locateCases(root) {
  return Promise.all(definitions.map(async definition => {
    const { id, path, startLine, endLine, sha256, broad, atoms, direct } = definition;
    const originalPath = resolve(root, 'packages/octocode-native/crates/runtime/src', path);
    const original = await readFile(originalPath, 'utf8');
    const lines = records(original);
    const selected = lines.slice(startLine - 1, endLine);
    const source = selected.join('');
    const digest = hash(source);
    if (selected.length !== endLine - startLine + 1 || selected.length < 90 || selected.length > 250) {
      throw new Error(`Invalid source extent for ${id}`);
    }
    if (digest !== sha256) throw new Error(`Source snapshot changed for ${id}; review gold before updating the case`);
    if (atoms.length !== 2 || atoms.some(atom => !atom.gold.length || atom.gold.some(line => !Number.isInteger(line) || line < 1 || line > selected.length))) {
      throw new Error(`Invalid atomic evidence for ${id}`);
    }
    if (direct.matchString.some(literal => !broad.includes(literal))) {
      throw new Error(`Direct-search information is missing from the broad question for ${id}`);
    }
    return {
      id, source,
      origin: { path: originalPath, startLine, endLine, sha256: digest },
      broad, atoms: structuredClone(atoms), direct: structuredClone(direct),
    };
  }));
}
