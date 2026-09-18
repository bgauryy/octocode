import { cellToString, escapeHtml } from './repo-formats.js';
import type { AwarenessEntityKind, AwarenessEntityLifecycle, AwarenessEntityOwner } from './schema/entities.js';

export interface AwarenessViewColumn {
  cid: number;
  name: string;
  type: string;
  not_null: number;
  default_value: unknown;
  primary_key: number;
}

export interface AwarenessViewEntity {
  name: string;
  family: string;
  kind: AwarenessEntityKind;
  owner: AwarenessEntityOwner;
  lifecycle: AwarenessEntityLifecycle;
  columns: AwarenessViewColumn[];
  rows: Array<Record<string, unknown>>;
}

export interface AwarenessViewSnapshot {
  generated_at: string;
  workspace: string;
  database: {
    path: string;
    sqlite_version: string;
    journal_mode: string;
    application_id: number;
    user_version: number;
  };
  local_git: Record<string, unknown>;
  agents: object[];
  entities: AwarenessViewEntity[];
}

function displayValue(value: unknown): string {
  if (value instanceof Uint8Array) return `base64:${Buffer.from(value).toString('base64')}`;
  if (typeof value === 'bigint') return value.toString();
  return cellToString(value);
}

function renderCell(value: unknown): string {
  const text = displayValue(value);
  return `<td><code>${escapeHtml(text)}</code></td>`;
}

function renderEntityTable(entity: AwarenessViewEntity): string {
  const names = entity.columns.map(column => column.name);
  if (entity.rows.length === 0) {
    return `<p class="empty">No rows. Columns: ${names.length ? names.map(escapeHtml).join(', ') : 'none'}.</p>`;
  }
  return `<div class="table-wrap"><table>
    <thead><tr>${names.map(name => `<th>${escapeHtml(name)}</th>`).join('')}</tr></thead>
    <tbody>${entity.rows.map(row => `<tr>${names.map(name => renderCell(row[name])).join('')}</tr>`).join('\n')}</tbody>
  </table></div>`;
}

function renderEntity(entity: AwarenessViewEntity): string {
  return `<details class="entity" data-entity="${escapeHtml(entity.name)}" data-family="${escapeHtml(entity.family)}" data-count="${entity.rows.length}">
    <summary><span>${escapeHtml(entity.name)}</span><small>${escapeHtml(entity.family)} · ${escapeHtml(entity.kind)} · ${entity.rows.length} rows</small></summary>
    <div class="entity-body">
      <p class="lifecycle"><span>owner: ${escapeHtml(entity.owner)}</span><span>access: ${escapeHtml(entity.lifecycle.access)}</span><span>retention: ${escapeHtml(entity.lifecycle.retention)}</span><span>deletion: ${escapeHtml(entity.lifecycle.deletion)}</span><span>cleanup: ${escapeHtml(entity.lifecycle.cleanup_operation)}</span></p>
      <p class="columns">${entity.columns.map(column => {
        const flags = [
          column.type || 'ANY',
          column.not_null ? 'NOT NULL' : '',
          column.primary_key ? 'PRIMARY KEY' : '',
          column.default_value == null ? '' : `DEFAULT ${displayValue(column.default_value)}`,
        ].filter(Boolean).join(' · ');
        return `<span><b>${escapeHtml(column.name)}</b> ${escapeHtml(flags)}</span>`;
      }).join('')}</p>
      ${renderEntityTable(entity)}
    </div>
  </details>`;
}

function renderAgents(agents: object[]): string {
  if (agents.length === 0) return '<p class="empty">No visible agents.</p>';
  const records = agents.map(agent => agent as Record<string, unknown>);
  const preferred = ['agent_id', 'agent_name', 'provenance', 'status', 'agent_vendor', 'agent_host', 'workspace_path', 'artifact', 'context', 'registered_at', 'last_seen_at'];
  const present = new Set(records.flatMap(agent => Object.keys(agent)));
  const columns = [...preferred.filter(column => present.has(column)), ...[...present].filter(column => !preferred.includes(column)).sort()];
  return `<div class="table-wrap"><table>
    <thead><tr>${columns.map(column => `<th>${escapeHtml(column)}</th>`).join('')}</tr></thead>
    <tbody>${records.map(agent => `<tr>${columns.map(column => renderCell(agent[column])).join('')}</tr>`).join('\n')}</tbody>
  </table></div>`;
}

export function renderOperatorAwarenessView(snapshot: AwarenessViewSnapshot): string {
  const totalRows = snapshot.entities.reduce((sum, entity) => sum + entity.rows.length, 0);
  const families = [...new Set(snapshot.entities.map(entity => entity.family))].sort();
  const initialized = snapshot.local_git['initialized'] === true;
  const operations = Number(snapshot.local_git['operations'] ?? 0);
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Octocode Awareness local view</title>
  <style>
    :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #091016; color: #e8f0f5; }
    * { box-sizing: border-box; }
    body { margin: 0; background: radial-gradient(circle at top left, #102a32 0, #091016 42%); min-height: 100vh; }
    header, main { width: min(1500px, calc(100% - 36px)); margin: 0 auto; }
    header { padding: 34px 0 22px; }
    h1 { margin: 0; font-size: clamp(26px, 4vw, 44px); letter-spacing: -.04em; }
    .subtitle, .note { color: #93a8b4; line-height: 1.5; }
    .subtitle code { color: #d5e8ef; }
    .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; margin: 22px 0; }
    .card, .panel, details { border: 1px solid #23404a; border-radius: 12px; background: rgba(12, 26, 33, .92); box-shadow: 0 10px 28px rgba(0,0,0,.2); }
    .card { padding: 15px 17px; }
    .card b { display: block; color: #7fe1c2; font-size: 24px; margin-top: 4px; overflow-wrap: anywhere; }
    .card span { color: #8fa4ae; font-size: 12px; text-transform: uppercase; letter-spacing: .08em; }
    .panel { padding: 18px; margin-bottom: 18px; }
    h2 { margin: 0 0 12px; font-size: 19px; }
    pre { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; font: 12px/1.55 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; color: #c8d8df; }
    .controls { position: sticky; top: 0; z-index: 2; display: flex; flex-wrap: wrap; gap: 9px; padding: 12px; margin-bottom: 14px; border: 1px solid #23404a; border-radius: 12px; background: rgba(9, 16, 22, .96); backdrop-filter: blur(10px); }
    input, select, button { min-height: 38px; border-radius: 8px; border: 1px solid #31505b; background: #0c1a21; color: #e8f0f5; padding: 7px 10px; font: inherit; }
    input { flex: 1 1 320px; }
    button { cursor: pointer; }
    details { margin-bottom: 10px; overflow: clip; }
    details[hidden] { display: none; }
    summary { cursor: pointer; display: flex; align-items: baseline; justify-content: space-between; gap: 12px; padding: 14px 16px; }
    summary span { font: 650 14px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; color: #8ee5c9; }
    summary small { color: #8097a2; }
    .entity-body { border-top: 1px solid #203942; padding: 13px 15px 16px; }
    .columns, .lifecycle { display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 12px; }
    .columns span, .lifecycle span { padding: 4px 7px; border-radius: 6px; background: #12252d; color: #91a8b2; font: 11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
    .columns b { color: #d7e6eb; }
    .table-wrap { max-height: 68vh; overflow: auto; border: 1px solid #1f3740; border-radius: 8px; }
    table { border-collapse: collapse; min-width: 100%; font-size: 12px; }
    th { position: sticky; top: 0; z-index: 1; background: #13252d; color: #cfe0e6; text-align: left; }
    th, td { border-bottom: 1px solid #1d343c; border-right: 1px solid #1d343c; padding: 7px 9px; vertical-align: top; max-width: 540px; overflow-wrap: anywhere; }
    td code { white-space: pre-wrap; }
    .empty { color: #718993; font-style: italic; }
    footer { color: #738a94; padding: 12px 0 36px; font-size: 12px; }
  </style>
</head>
<body>
  <header>
    <h1>Awareness local view</h1>
    <p class="subtitle">Generated ${escapeHtml(snapshot.generated_at)} for <code>${escapeHtml(snapshot.workspace)}</code>.</p>
    <p class="note">Agents and LocalGit status are workspace-scoped. Entity tables and their SQLite row count cover the complete selected store. LocalGit remains evidence, not coordination truth; captured file bytes are not embedded.</p>
    <div class="cards">
      <div class="card"><span>Store-wide entities</span><b>${snapshot.entities.length}</b></div>
      <div class="card"><span>Store-wide SQLite rows</span><b>${totalRows}</b></div>
      <div class="card"><span>Workspace-scoped agents</span><b>${snapshot.agents.length}</b></div>
      <div class="card"><span>LocalGit</span><b>${initialized ? 'ready' : 'not initialized'}</b></div>
      <div class="card"><span>History operations</span><b>${Number.isFinite(operations) ? operations : 0}</b></div>
    </div>
  </header>
  <main>
    <section class="panel">
      <h2>Agents · workspace scope</h2>
      <p class="note">Visible registered and Message-observed identities use the same linked-workspace scope as <code>work.list</code> with <code>kind: agents</code>; store-global registrations remain visible.</p>
      ${renderAgents(snapshot.agents)}
    </section>
    <section class="panel">
      <h2>Database</h2>
      <pre>${escapeHtml(JSON.stringify(snapshot.database, null, 2))}</pre>
    </section>
    <section class="panel">
      <h2>LocalGit evidence</h2>
      <pre>${escapeHtml(JSON.stringify(snapshot.local_git, null, 2))}</pre>
    </section>
    <div class="controls">
      <input id="filter" type="search" placeholder="Filter entity names, columns, or row values" autocomplete="off">
      <select id="family" aria-label="Entity family"><option value="">All families</option>${families.map(family => `<option value="${escapeHtml(family)}">${escapeHtml(family)}</option>`).join('')}</select>
      <button id="expand" type="button">Expand visible</button>
      <button id="collapse" type="button">Collapse all</button>
    </div>
    <div id="entities">${snapshot.entities.map(renderEntity).join('\n')}</div>
    <footer>Private local artifact · reopen the CLI view command to refresh this snapshot.</footer>
  </main>
  <script>
    const filter = document.querySelector('#filter');
    const family = document.querySelector('#family');
    const entities = Array.from(document.querySelectorAll('details.entity'));
    function apply() {
      const wanted = filter.value.trim().toLowerCase();
      const wantedFamily = family.value;
      for (const entity of entities) {
        const familyMatches = !wantedFamily || entity.dataset.family === wantedFamily;
        const textMatches = !wanted || entity.textContent.toLowerCase().includes(wanted);
        entity.hidden = !(familyMatches && textMatches);
        if (wanted && !entity.hidden) entity.open = true;
      }
    }
    filter.addEventListener('input', apply);
    family.addEventListener('input', apply);
    document.querySelector('#expand').addEventListener('click', () => entities.filter(entity => !entity.hidden).forEach(entity => { entity.open = true; }));
    document.querySelector('#collapse').addEventListener('click', () => entities.forEach(entity => { entity.open = false; }));
  </script>
</body>
</html>
`;
}
