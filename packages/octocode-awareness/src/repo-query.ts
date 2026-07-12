import { join } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { AWARENESS_QUERY_VIEWS, AwarenessQueryParams, AwarenessQueryResult, AwarenessQueryRow, AwarenessQuerySection, AwarenessQueryView, boundedRows, limitOf, normalizeFormat, normalizeView, stringList, utcNow } from './repo-model.js';
import { activityRows, fileRows, repoProfileRows } from './repo-files.js';
import { memoryRows, planRows, runRows, taskRows } from './repo-plans.js';
import { agentRows, developerReviewRows, lockRows, refinementRows, signalRows } from './repo-coordination.js';
import { workboardRows } from './repo-workboard.js';
import { scopeFromParams, withScope } from './repo-scope.js';
import { renderDeveloperReviewDoc } from './repo-docs.js';
import { escapeHtml, renderHtmlSection, toCsv, toMarkdown, toTable } from './repo-formats.js';
import { atomicWriteText, resolveWorkspaceOutputPath } from './repo-projection.js';

const DEVELOPER_REVIEW_EXPORT_MAX_LINES = 200;

export function rowsForView(db: DatabaseSync, view: AwarenessQueryView, params: AwarenessQueryParams): AwarenessQueryRow[] {
  switch (view) {
    case 'repo-profile': return repoProfileRows(db, params);
    case 'memories': return memoryRows(db, params);
    case 'gotchas': return memoryRows(db, params, { gotchas: true });
    case 'lessons': return memoryRows(db, params, { lessons: true });
    case 'plans': return planRows(db, params);
    case 'tasks': return taskRows(db, params);
    case 'runs': return runRows(db, params);
    case 'locks': return lockRows(db, params);
    case 'agents': return agentRows(db, params);
    case 'signals': return signalRows(db, params);
    case 'refinements': return refinementRows(db, params);
    case 'files': return fileRows(db, params);
    case 'activity': return activityRows(db, params);
    case 'workboard': return workboardRows(db, params);
    case 'developer-review': return developerReviewRows(db, params);
    case 'all': return [];
  }
}

export function queryAwareness(db: DatabaseSync, params: AwarenessQueryParams = {}): AwarenessQueryResult {
  const view = normalizeView(params.view);
  const scope = scopeFromParams(params);
  const generatedAt = utcNow();
  const requestedLimit = limitOf(params.limit, 50, 500);
  const filters = {
    query: params.query ?? null,
    limit: requestedLimit,
    agent_id: params.agentId ?? params.agent_id ?? null,
    state: stringList(params.state),
    label: stringList(params.label),
    file: params.file ?? null,
    since: params.since ?? null,
  };

  if (view === 'all') {
    const sections: Record<string, AwarenessQuerySection> = {};
    for (const section of AWARENESS_QUERY_VIEWS) {
      if (section === 'all') continue;
      const probeLimit = section === 'workboard' ? requestedLimit : Math.min(501, requestedLimit + 1);
      const completeness = boundedRows(
        section,
        rowsForView(db, section, withScope(params, { limit: probeLimit })),
        requestedLimit,
      );
      sections[section] = { count: completeness.rows.length, ...completeness };
    }
    const rows = Object.entries(sections).map(([name, section]) => ({
      section: name,
      count: section.count,
      total: section.total,
      omitted_count: section.omitted_count,
      is_partial: section.is_partial,
      continuation: section.continuation,
    }));
    const isPartial = Object.values(sections).some(section => section.is_partial);
    const knownTotals = Object.values(sections).map(section => section.total);
    const total = knownTotals.some(value => value == null)
      ? null
      : knownTotals.reduce<number>((sum, value) => sum + Number(value), 0);
    const omittedCounts = Object.values(sections).map(section => section.omitted_count);
    const omittedCount = omittedCounts.some(value => value == null)
      ? null
      : omittedCounts.reduce<number>((sum, value) => sum + Number(value), 0);
    return {
      ok: true,
      view,
      generated_at: generatedAt,
      workspace_path: scope.workspacePath,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      count: rows.length,
      rows,
      total,
      omitted_count: omittedCount,
      is_partial: isPartial,
      continuation: isPartial ? 'inspect section completeness and follow its targeted continuation' : null,
      sections,
      filters,
    };
  }

  const completeness = boundedRows(
    view,
    rowsForView(db, view, withScope(params, {
      limit: view === 'workboard' ? requestedLimit : Math.min(501, requestedLimit + 1),
    })),
    requestedLimit,
  );
  return {
    ok: true,
    view,
    generated_at: generatedAt,
    workspace_path: scope.workspacePath,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    count: completeness.rows.length,
    rows: completeness.rows,
    total: completeness.total,
    omitted_count: completeness.omitted_count,
    is_partial: completeness.is_partial,
    continuation: completeness.continuation,
    filters,
  };
}

/**
 * Developer-review digest for the CLI `reflect developer-review` command, including
 * an explicit bounded Markdown export.
 */
export function developerReviewDoc(
  db: DatabaseSync,
  params: AwarenessQueryParams = {},
): { rows: AwarenessQueryRow[]; open: number; resolved: number; markdown: string } {
  const rows = developerReviewRows(db, params);
  const open = rows.filter(row => String(row['state']) !== 'done').length;
  return {
    rows,
    open,
    resolved: rows.length - open,
    markdown: renderDeveloperReviewDoc(rows, DEVELOPER_REVIEW_EXPORT_MAX_LINES),
  };
}

export function formatAwarenessQueryResult(result: AwarenessQueryResult, format: string | null | undefined): string {
  const normalized = normalizeFormat(format);
  if (normalized === 'json') return JSON.stringify(result, null, 2);
  if (normalized === 'csv') {
    const rows = result.rows.length > 0 ? result.rows : [{}];
    return toCsv(rows.map(row => ({
      ...row,
      __awareness_is_partial: result.is_partial,
      __awareness_total: result.total,
      __awareness_omitted_count: result.omitted_count,
      __awareness_continuation: result.continuation,
    })));
  }
  if (normalized === 'table') return `${completenessText(result)}\n${toTable(result.rows)}`;
  if (normalized === 'html') return renderAwarenessHtml(result);
  return toMarkdown(result);
}

export function completenessText(result: AwarenessQueryResult): string {
  const total = result.total == null ? 'unknown' : String(result.total);
  const omitted = result.omitted_count == null ? 'unknown' : String(result.omitted_count);
  const continuation = result.continuation ? `; next: ${result.continuation}` : '';
  return `Completeness: ${result.is_partial ? 'partial' : 'complete'}; visible=${result.count}; total=${total}; omitted=${omitted}${continuation}`;
}

export function renderAwarenessHtml(result: AwarenessQueryResult): string {
  const title = `Octocode Awareness: ${result.view}`;
  const sectionNames = result.sections ? Object.keys(result.sections) : [result.view];
  const sections = result.sections
    ? Object.entries(result.sections).map(([name, section]) => renderHtmlSection(name, section.rows)).join('\n')
    : renderHtmlSection(result.view, result.rows);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(title)}</title>
  <style>
    :root { color-scheme: light dark; font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; background: Canvas; color: CanvasText; }
    header { padding: 24px 28px 12px; border-bottom: 1px solid color-mix(in srgb, CanvasText 16%, transparent); }
    .controls { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 16px; align-items: center; }
    .controls input, .controls select, .controls label { font: inherit; font-size: 13px; }
    .controls input, .controls select { min-height: 34px; padding: 6px 8px; border: 1px solid color-mix(in srgb, CanvasText 24%, transparent); border-radius: 6px; background: Canvas; color: CanvasText; }
    .controls input { min-width: min(420px, 100%); flex: 1 1 260px; }
    .controls label { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; color: color-mix(in srgb, CanvasText 76%, transparent); }
    main { padding: 20px 28px 40px; display: grid; gap: 28px; }
    h1 { margin: 0 0 8px; font-size: 24px; letter-spacing: 0; }
    h2 { margin: 0 0 12px; font-size: 18px; letter-spacing: 0; }
    .meta { color: color-mix(in srgb, CanvasText 62%, transparent); font-size: 13px; }
    table { width: 100%; border-collapse: collapse; font-size: 13px; }
    th, td { text-align: left; vertical-align: top; padding: 8px 10px; border-bottom: 1px solid color-mix(in srgb, CanvasText 12%, transparent); }
    th { font-weight: 650; white-space: nowrap; }
    th button { all: unset; cursor: pointer; }
    th button::after { content: " v"; color: color-mix(in srgb, CanvasText 46%, transparent); }
    td { max-width: 460px; overflow-wrap: anywhere; }
    section { overflow-x: auto; }
    section[hidden], tr[hidden] { display: none; }
    code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 12px; }
  </style>
</head>
<body>
  <header>
    <h1>${escapeHtml(title)}</h1>
    <div class="meta">Generated ${escapeHtml(result.generated_at)} for <code>${escapeHtml(result.workspace_path ?? 'global')}</code></div>
    <div class="meta">${escapeHtml(completenessText(result))}</div>
    <div class="controls">
      <input id="global-filter" type="search" placeholder="Filter rows" autocomplete="off">
      <select id="section-filter" aria-label="Section">
        <option value="">All sections</option>
        ${sectionNames.map(name => `<option value="${escapeHtml(name)}">${escapeHtml(name)}</option>`).join('')}
      </select>
      <label><input id="missing-filter" type="checkbox"> Missing files</label>
    </div>
  </header>
  <main>
    ${sections}
  </main>
  <script>
    const filterInput = document.querySelector('#global-filter');
    const sectionFilter = document.querySelector('#section-filter');
    const missingFilter = document.querySelector('#missing-filter');
    function applyFilters() {
      const text = filterInput.value.trim().toLowerCase();
      const wantedSection = sectionFilter.value;
      const onlyMissing = missingFilter.checked;
      for (const section of document.querySelectorAll('section[data-section]')) {
        const sectionMatches = !wantedSection || section.dataset.section === wantedSection;
        let visible = 0;
        for (const row of section.querySelectorAll('tbody tr')) {
          const rowMatches = (!text || row.textContent.toLowerCase().includes(text)) && (!onlyMissing || row.dataset.missing === 'true');
          row.hidden = !(sectionMatches && rowMatches);
          if (!row.hidden) visible++;
        }
        section.hidden = !sectionMatches || visible === 0;
      }
    }
    for (const control of [filterInput, sectionFilter, missingFilter]) control.addEventListener('input', applyFilters);
    for (const button of document.querySelectorAll('th button[data-key]')) {
      button.addEventListener('click', () => {
        const table = button.closest('table');
        const tbody = table.querySelector('tbody');
        const index = Array.from(button.closest('tr').children).indexOf(button.closest('th'));
        const direction = button.dataset.direction === 'asc' ? 'desc' : 'asc';
        button.dataset.direction = direction;
        const rows = Array.from(tbody.querySelectorAll('tr'));
        rows.sort((a, b) => {
          const av = a.children[index]?.textContent.trim() ?? '';
          const bv = b.children[index]?.textContent.trim() ?? '';
          const an = Number(av);
          const bn = Number(bv);
          const cmp = Number.isFinite(an) && Number.isFinite(bn) ? an - bn : av.localeCompare(bv);
          return direction === 'asc' ? cmp : -cmp;
        });
        for (const row of rows) tbody.appendChild(row);
        applyFilters();
      });
    }
    applyFilters();
  </script>
</body>
</html>
`;
}

export function writeAwarenessView(
  db: DatabaseSync,
  params: AwarenessQueryParams & { out?: string | null; format?: string | null } = {},
): { ok: true; path: string; view: AwarenessQueryView; count: number; total: number | null; omitted_count: number | null; is_partial: boolean; continuation: string | null } {
  const result = queryAwareness(db, params);
  const workspacePath = scopeFromParams(params).workspacePath ?? process.cwd();
  const outPath = resolveWorkspaceOutputPath(params.out, workspacePath, join(workspacePath, '.octocode', 'awareness', 'index.html'));
  atomicWriteText(outPath, renderAwarenessHtml(result));
  return {
    ok: true,
    path: outPath,
    view: result.view,
    count: result.count,
    total: result.total,
    omitted_count: result.omitted_count,
    is_partial: result.is_partial,
    continuation: result.continuation,
  };
}
