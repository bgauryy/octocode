// Markdown of the wiki files: one page (header, FACTS, agent notes between the markers), an orphaned page, and
// index.md (the map, ranked by summed PageRank) rendered from map.json alone so notes can update it without data.json.
import { ISOLATED, NOTES_END, NOTES_START, pathOf } from './wiki-store.mjs';
import { cell, num } from './wiki-facts.mjs';

export const HEADER = /^> \*\*Notes:\*\*.*$/m;
const FACTS_NOTE = '<!-- facts: regenerated from data.json on every build; edit only the agent notes below -->';
const NOTES_HINT = '<!-- agent notes: purpose, how it works, invariants, gotchas; every claim cites path:line. Write them with `wiki.mjs note` or `promote` (references/wiki.md); they are kept verbatim across builds. -->';

export function notesState(entry) {
  if (!entry.notes) return 'none';
  return entry.stale?.length ? 'stale' : 'fresh';
}

const files = (entry, ids) => ids.map((id) => `\`${pathOf(entry, id)}\``).join(', ');

function notesText(entry) {
  const state = notesState(entry);
  const date = entry.notes?.at?.slice(0, 10);
  if (state === 'none') return 'none yet';
  if (state === 'fresh') return `fresh (written ${date})`;
  return `stale: ${entry.stale.length} member${entry.stale.length > 1 ? 's' : ''} changed since the notes were written ${date}: ${files(entry, entry.stale)}`;
}

export function headerLine(entry, slug) {
  const orphan = entry.orphaned ? ` · **orphaned** since ${entry.orphaned.slice(0, 10)}: no community matches this page, notes kept` : '';
  return `> **Notes:** ${notesText(entry)}${orphan} · community ${entry.community ?? '–'} · ${entry.size} files · slug \`${slug}\``;
}

export function renderPage({ slug, entry, facts, notes }) {
  return [
    `# ${entry.label}`, '', headerLine(entry, slug), '',
    '## Facts', '', FACTS_NOTE, '', ...facts, '',
    '## Agent notes', '', NOTES_HINT, '', NOTES_START, ...(notes ? [notes] : []), NOTES_END, '',
  ].join('\n');
}

// A page whose community is gone: its last members stay listed so the notes can be moved to the pages that hold them now.
export function orphanFacts(entry) {
  return [`No community matches this page since ${entry.orphaned.slice(0, 10)}. Its last ${entry.members.length} members:`, '', ...entry.members.map((id) => `- \`${pathOf(entry, id)}:1\``)];
}

const summaryOf = (entry) => cell(entry.summary || entry.fact || '');

function indexRows(live) {
  return live.map(([slug, e], i) => `| ${i + 1} | [[${slug}]] | ${cell(e.label)} | ${e.size} | ${num(e.pagerank)} | ${notesState(e)} | ${summaryOf(e)} |`);
}

function attention(live) {
  const stale = live.filter(([, e]) => notesState(e) === 'stale').map(([s]) => `[[${s}]]`);
  const none = live.filter(([, e]) => notesState(e) === 'none').map(([s]) => `[[${s}]]`);
  return [
    `- **Stale notes** (a member changed since they were written: revisit): ${stale.length ? stale.join(', ') : 'none'}`,
    `- **No notes yet**, highest PageRank first: ${none.length ? none.join(', ') : 'none'}`,
  ];
}

function orphanRows(orphans) {
  if (!orphans.length) return [];
  return [
    '', '## Orphaned pages', '',
    'Their community no longer exists; the notes are kept. Move what still holds to the page that owns those files now, then clear the notes (`wiki.mjs note <out> <slug> ""`): the next build removes a page without notes.', '',
    ...orphans.map(([slug, e]) => `- [[${slug}]] — ${cell(e.label)} — orphaned since ${e.orphaned.slice(0, 10)} — ${summaryOf(e)}`),
  ];
}

export function renderIndex(map) {
  const entries = Object.entries(map.pages);
  const live = entries.filter(([s, e]) => s !== ISOLATED && !e.orphaned).toSorted((a, b) => b[1].pagerank - a[1].pagerank || a[0].localeCompare(b[0]));
  const orphans = entries.filter(([, e]) => e.orphaned).toSorted((a, b) => a[0].localeCompare(b[0]));
  const iso = map.pages[ISOLATED];
  return [
    '# Code wiki', '',
    `> Map of \`${map.root || '.'}\` built ${map.builtAt ?? ''} from data.json: ${live.length} community pages ranked by summed PageRank · [[${ISOLATED}]] (${iso?.size ?? 0} files in no community, notes ${iso ? notesState(iso) : 'none'}) · history [[log]]. Notes: none, fresh, or stale (a member changed since they were written).`, '',
    '| # | page | label | files | PageRank | notes | summary |', '|---|---|---|---|---|---|---|', ...indexRows(live), '',
    '## Needs attention', '', ...attention(live), ...orphanRows(orphans), '',
  ].join('\n');
}
