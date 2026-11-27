// Markdown of the wiki files: one page (header, FACTS, agent notes between the markers), an orphaned page, and
// index.md (the map, ranked by summed PageRank) rendered from map.json alone so notes can update it without data.json.
import {
  ISOLATED,
  NOTES_END,
  NOTES_START,
  pathOf,
  type WikiEntry,
  type WikiMap,
} from './wiki-store.ts';
import { cell, num } from './wiki-facts.ts';

/** A page whose community is gone, since `orphaned`. */
export type OrphanedEntry = WikiEntry & { orphaned: string };

export const HEADER = /^> \*\*Notes:\*\*.*$/m;
const FACTS_NOTE =
  '<!-- facts: regenerated from data.json on every build; edit only the agent notes below -->';
const NOTES_HINT =
  '<!-- Optional notes survive rebuilds. Use `octocode-graph wiki note` or `promote` to record a finding and its source. -->';

export function notesState(entry: Pick<WikiEntry, 'notes' | 'stale'>) {
  if (!entry.notes) return 'none';
  return entry.stale?.length ? 'stale' : 'fresh';
}

const files = (entry: WikiEntry, ids: readonly string[]) =>
  ids.map(id => `\`${pathOf(entry, id)}\``).join(', ');

function notesText(entry: WikiEntry) {
  const state = notesState(entry);
  const date = entry.notes?.at?.slice(0, 10);
  if (state === 'none') return 'none yet';
  if (state === 'fresh') return `fresh (written ${date})`;
  return `stale: ${entry.stale.length} member${entry.stale.length > 1 ? 's' : ''} changed since the notes were written ${date}: ${files(entry, entry.stale)}`;
}

export function headerLine(entry: WikiEntry, slug: string) {
  const orphan = entry.orphaned
    ? ` · **orphaned** since ${entry.orphaned.slice(0, 10)}: no community matches this page, notes kept`
    : '';
  return `> **Notes:** ${notesText(entry)}${orphan} · community ${entry.community ?? '–'} · ${entry.size} files · slug \`${slug}\``;
}

export function renderPage({
  slug,
  entry,
  facts,
  notes,
}: {
  slug: string;
  entry: WikiEntry;
  facts: readonly string[];
  notes: string;
}) {
  return [
    `# ${entry.label}`,
    '',
    headerLine(entry, slug),
    '',
    '## Facts',
    '',
    FACTS_NOTE,
    '',
    ...facts,
    '',
    '## Agent notes',
    '',
    NOTES_HINT,
    '',
    NOTES_START,
    ...(notes ? [notes] : []),
    NOTES_END,
    '',
  ].join('\n');
}

// A page whose community is gone: its last members stay listed so the notes can be moved to the pages that hold them now.
export function orphanFacts(entry: OrphanedEntry) {
  return [
    `No community matches this page since ${entry.orphaned.slice(0, 10)}. Its last ${entry.members.length} members:`,
    '',
    ...entry.members.map(id => `- \`${pathOf(entry, id)}:1\``),
  ];
}

const summaryOf = (entry: WikiEntry) => cell(entry.summary || entry.fact || '');

type Page<E extends WikiEntry = WikiEntry> = [slug: string, entry: E];

function indexRows(live: readonly Page[]) {
  return live.map(
    ([slug, e], i) =>
      `| ${i + 1} | [[${slug}]] | ${cell(e.label)} | ${e.size} | ${num(e.pagerank)} | ${notesState(e)} | ${summaryOf(e)} |`
  );
}

function attention(live: readonly Page[]) {
  const stale = live
    .filter(([, e]) => notesState(e) === 'stale')
    .map(([s]) => `[[${s}]]`);
  const none = live
    .filter(([, e]) => notesState(e) === 'none')
    .map(([s]) => `[[${s}]]`);
  return [
    `- **Stale notes** (a member changed since they were written: revisit): ${stale.length ? stale.join(', ') : 'none'}`,
    `- **No notes yet**, highest PageRank first: ${none.length ? none.join(', ') : 'none'}`,
  ];
}

function orphanRows(orphans: readonly Page<OrphanedEntry>[]) {
  if (!orphans.length) return [];
  return [
    '',
    '## Orphaned pages',
    '',
    'Their community no longer exists; the notes are kept. Move what still holds to the page that owns those files now, then clear the notes (`octocode-graph wiki note <out> <slug> ""`): the next build removes a page without notes.',
    '',
    ...orphans.map(
      ([slug, e]) =>
        `- [[${slug}]] — ${cell(e.label)} — orphaned since ${e.orphaned.slice(0, 10)} — ${summaryOf(e)}`
    ),
  ];
}

const isOrphaned = (page: Page): page is Page<OrphanedEntry> =>
  Boolean(page[1].orphaned);

export function renderIndex(map: WikiMap) {
  const entries = Object.entries(map.pages);
  const live = entries
    .filter(([s, e]) => s !== ISOLATED && !e.orphaned)
    .toSorted(
      (a, b) => b[1].pagerank - a[1].pagerank || a[0].localeCompare(b[0])
    );
  const orphans = entries
    .filter(isOrphaned)
    .toSorted((a, b) => a[0].localeCompare(b[0]));
  const iso = map.pages[ISOLATED];
  return [
    '# Code wiki',
    '',
    `> Map of \`${map.root || '.'}\` built ${map.builtAt ?? ''} from data.json: ${live.length} community pages ranked by summed PageRank · [[${ISOLATED}]] (${iso?.size ?? 0} files in no community, notes ${iso ? notesState(iso) : 'none'}) · history [[log]]. Notes: none, fresh, or stale (a member changed since they were written).`,
    '',
    '| # | page | label | files | PageRank | notes | summary |',
    '|---|---|---|---|---|---|---|',
    ...indexRows(live),
    '',
    '## Needs attention',
    '',
    ...attention(live),
    ...orphanRows(orphans),
    '',
  ].join('\n');
}
