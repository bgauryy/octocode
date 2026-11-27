// One page holds every key, setting and environment variable. All of them come from the config contract.
import { state, hooks, $, scope, query, node, append, message, call, writable, refresh, save, button, matches, settingMatches, settingRow, keyRow, processRow, group } from './core.js';

const table = rows => (rows.length ? append(node('div', undefined, 'table'), ...rows) : null);

/** One headed part of the page; nothing when a search leaves it empty. */
function pageSection(id, title, lede, content) {
  if (!content) return null;
  const section = node('section', undefined, 'page-section');
  section.id = id;
  return append(section, node('h2', title), lede ? node('p', lede, 'lede') : null, content);
}

const known = () => state.snapshot.knownKeys ?? [];
const knownMatches = item => matches(item.key, item.description, item.group);

function keysTable() {
  const entries = known().filter(item => item.group);
  const grouped = (state.snapshot.settings ?? []).filter(item => item.group);
  const titles = [...new Set(entries.map(item => item.group).concat(grouped.map(item => item.group)))];
  const rows = titles.flatMap(title => {
    const inGroup = entries.filter(item => item.group === title && knownMatches(item));
    // Keys first, then what configures them, so an endpoint follows its key.
    return group(title, [
      ...inGroup.filter(item => item.credential).map(item => keyRow(item.key, item.description, item)),
      ...grouped.filter(item => item.group === title && settingMatches(item)).map(settingRow),
      ...inGroup.filter(item => !item.credential).map(item => keyRow(item.key, item.description, item)),
    ]);
  });
  const names = new Set(known().map(item => item.key));
  const custom = (state.snapshot.keys ?? []).filter(item => item.scope === scope() && !names.has(item.key) && matches(item.key)).map(item => keyRow(item.key));
  rows.push(...group('Your other keys', custom));
  if (!query()) rows.push(...group('Add a key', [addKeyRow()]));
  return table(rows);
}

function addKeyRow() {
  const name = node('input'); name.autocomplete = 'off'; name.spellcheck = false; name.placeholder = 'MY_API_KEY'; name.setAttribute('aria-label', 'New key name');
  const secret = node('input'); secret.type = 'password'; secret.autocomplete = 'new-password'; secret.placeholder = 'Value'; secret.setAttribute('aria-label', 'New key value');
  const add = button('Add key', () => {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name.value)) { message('Enter a valid environment variable name.', true); return; }
    void save({ operation: 'setEnv', key: name.value, value: secret.value, scope: scope(), revision: state.snapshot.files?.[`${scope()}Env`]?.revision ?? null });
  });
  add.disabled = !writable('Env');
  return append(node('div', undefined, 'row'),
    append(node('div', undefined, 'row-info'), node('p', 'For a provider a skill or script reads from the environment.', 'row-text')),
    append(node('div', undefined, 'row-value'), append(node('div', undefined, 'fields'), name, secret)),
    append(node('div', undefined, 'row-actions'), add));
}

function settingsTable() {
  const settings = (state.snapshot.settings ?? []).filter(item => !item.credential && !item.group);
  const titles = [...new Set(settings.map(item => item.sectionTitle))];
  return table(titles.flatMap(title => group(title, settings.filter(item => item.sectionTitle === title && settingMatches(item)).map(settingRow))));
}

function environmentTable() {
  const entries = known().filter(item => !item.group && knownMatches(item));
  return table(entries.map(item => (item.writableFrom === 'process' ? processRow(item) : keyRow(item.key, item.description, item))));
}

const isInstalled = agent => agent.configured ?? agent.installed ?? ['configured', 'installed', 'enabled', 'disabled'].includes(agent.status);
const agentName = agent => `${agent.title ?? agent.client} (${agent.scope ?? 'home'})`;
const agentBase = agent => ({ client: agent.client, scope: agent.scope ?? 'home', revision: agent.revision ?? null });
const canEdit = agent => !(agent.writable === false || agent.readOnly || agent.managed);

function methodSelect(label, keepCurrent) {
  const method = node('select');
  if (keepCurrent) { const option = node('option', 'Keep current command'); option.value = ''; method.append(option); }
  ['npx', 'bunx', 'pnpm'].forEach(item => { const option = node('option', item); option.value = item; method.append(option); });
  method.value = keepCurrent ? '' : 'npx';
  method.setAttribute('aria-label', label);
  return method;
}

function agentKeys(agent) {
  const label = agentName(agent);
  const key = node('input'); key.placeholder = 'MY_API_KEY'; key.spellcheck = false; key.autocomplete = 'off'; key.setAttribute('aria-label', `${label} environment key`);
  const secret = node('input'); secret.type = 'password'; secret.autocomplete = 'new-password'; secret.placeholder = 'Value'; secret.setAttribute('aria-label', `${label} environment value`);
  const add = button('Save agent key', () => {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key.value)) { message('Enter a valid environment variable name.', true); return; }
    void save({ operation: 'setAgent', ...agentBase(agent), patch: { env: { [key.value]: secret.value } } });
  });
  const keys = append(node('details'), node('summary', 'Environment keys for this agent'), append(node('div', undefined, 'fields'), key, secret), append(node('div', undefined, 'row-actions'), add));
  (agent.entry?.envKeys ?? (Array.isArray(agent.envKeys) ? agent.envKeys : [])).forEach(item => {
    keys.append(append(node('div', undefined, 'row-actions'), node('code', item), button('Remove key', () => void save({ operation: 'setAgent', ...agentBase(agent), patch: { env: { [item]: null } } }), 'danger')));
  });
  return keys;
}

function installedRow(agent) {
  const editable = canEdit(agent);
  const label = agentName(agent);
  const method = methodSelect(`${label} launch with`, true);
  method.disabled = !editable;
  const toggled = agent.supportsEnabled === true;
  const enabled = node('input'); enabled.type = 'checkbox'; enabled.checked = (agent.entry?.enabled ?? agent.enabled) !== false; enabled.disabled = !editable; enabled.setAttribute('aria-label', `${label} enabled`);
  const update = button('Save', () => void save({ operation: 'setAgent', ...agentBase(agent), patch: { ...(toggled ? { enabled: enabled.checked } : {}), ...(method.value ? { method: method.value } : {}) } }));
  const remove = button('Remove', () => void save({ operation: 'removeAgent', ...agentBase(agent) }), 'danger');
  update.disabled = remove.disabled = !editable;
  const value = append(node('div', undefined, 'row-value'), toggled ? append(node('label', undefined, 'toggle'), enabled, node('span', undefined, 'track'), node('span', 'Enabled')) : null, method);
  if (editable) value.append(agentKeys(agent));
  else value.append(node('p', 'This configuration source is read only.', 'note'));
  return append(node('div', undefined, 'row'),
    append(node('div', undefined, 'row-info'), node('div', label, 'row-name'), agent.path ? node('code', agent.path, 'row-meta') : null, agent.error ? node('p', agent.error, 'warning') : null),
    value, append(node('div', undefined, 'row-actions'), update, remove));
}

/** One row to add Octocode to any client it is not in yet. */
function installRow(agents) {
  const client = node('select');
  client.setAttribute('aria-label', 'Client to add Octocode to');
  agents.forEach((agent, index) => { const option = node('option', agentName(agent)); option.value = String(index); client.append(option); });
  const method = methodSelect('Launch new client with', false);
  const install = button('Install', () => {
    const agent = agents[Number(client.value)];
    void save({ operation: 'setAgent', ...agentBase(agent), patch: { method: method.value } });
  });
  return append(node('div', undefined, 'row'),
    append(node('div', undefined, 'row-info'), node('p', 'Add Octocode to another client as an MCP server.', 'row-text')),
    append(node('div', undefined, 'row-value'), append(node('div', undefined, 'fields'), client, method)),
    append(node('div', undefined, 'row-actions'), install));
}

function agentsTable() {
  const visible = state.agents.filter(agent => matches(agent.title, agent.client, agent.path));
  const available = visible.filter(agent => !isInstalled(agent) && canEdit(agent));
  const rows = visible.filter(isInstalled).map(installedRow);
  if (available.length) rows.push(installRow(available));
  return table(rows);
}

function filesFooter() {
  const places = [['Home .env', 'homeEnv'], ['Home .octocoderc', 'homeSettings'], ['Workspace .env', 'workspaceEnv'], ['Workspace .octocoderc', 'workspaceSettings']];
  const rows = places.map(([label, key]) => {
    const file = state.snapshot.files?.[key] ?? {};
    return append(node('div', undefined, 'row-meta'), node('b', `${label}: `), document.createTextNode(file.path ?? ''), document.createTextNode(file.exists ? '' : ' (not created yet)'));
  });
  return append(node('details'), node('summary', 'Where values are saved, and what wins'),
    node('p', 'For each value, the first of these wins: shell or MCP env block, workspace .env, home .env, workspace .octocoderc, home .octocoderc, default.', 'order'), ...rows);
}

function summary() {
  const set = known().filter(item => item.credential && item.group && (item.set || (state.snapshot.keys ?? []).some(key => key.key === item.key && key.set !== false))).length;
  const changed = (state.snapshot.settings ?? []).filter(item => item.source !== 'default').length;
  return `${set} of ${known().filter(item => item.credential && item.group).length} keys set · ${changed} settings changed · ${state.agents.filter(isInstalled).length} agents installed`;
}

/** What someone has typed and not saved, by the label that survives a redraw. */
function typedValues() {
  return [...$('content').querySelectorAll('[data-dirty]')]
    .filter(input => input.getAttribute('aria-label'))
    .map(input => ({ label: input.getAttribute('aria-label'), value: input.value, checked: input.checked }));
}

// Typed values in rows a search has hidden, restored when the row comes back.
let hiddenTyped = [];

function restoreTyped(typed) {
  const inputs = [...$('content').querySelectorAll('[aria-label]')];
  hiddenTyped = typed.filter(({ label }) => !inputs.some(item => item.getAttribute('aria-label') === label));
  typed.forEach(({ label, value, checked }) => {
    const input = inputs.find(item => item.getAttribute('aria-label') === label);
    if (!input || input.disabled) return;
    if (input.type === 'checkbox') input.checked = checked;
    else input.value = value;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  });
}

function render(keepTyped = true) {
  const typed = keepTyped ? [...typedValues(), ...hiddenTyped] : [];
  const top = window.scrollY;
  const content = $('content');
  content.replaceChildren();
  $('summary').textContent = query() ? `Results for “${$('search').value.trim()}”` : summary();
  if (!writable('Settings')) content.append(node('p', `The ${scope()} settings file is read only. Repair it before editing.`, 'warning'));
  append(content,
    pageSection('keys', 'API keys and providers', 'Saved values stay hidden. Restart the MCP server or start a new agent session after a change.', keysTable()),
    pageSection('settings', 'Settings', 'Saved to .octocoderc. Each shows its default and what applies now.', settingsTable()),
    pageSection('environment', 'Environment variables', 'Set these in the shell, CI or an MCP env block unless a field is offered: a repository must not be able to change them.', environmentTable()),
    pageSection('agents', 'Agents', 'Where Octocode runs as an MCP server. Restart the agent after a change.', agentsTable()));
  if (!content.children.length) content.append(node('p', 'Nothing matches.', 'empty'));
  else if (!query()) content.append(filesFooter());
  restoreTyped(typed);
  window.scrollTo(0, top);
}

hooks.render = () => render();

// A value typed for one scope must not be saved to the other.
document.querySelectorAll('input[name=scope]').forEach(input => input.addEventListener('change', () => render(false)));
$('search').addEventListener('input', () => render());
for (const event of ['input', 'change']) {
  $('content').addEventListener(event, ({ target }) => { target.dataset.dirty = '1'; });
}
$('content').addEventListener('click', ({ target }) => { state.activeRow = target.closest('.row'); });
$('close').addEventListener('click', async () => {
  try {
    await call('/api/close', {});
    state.ended = true; state.credential = undefined;
    $('warnings').replaceChildren();
    $('summary').textContent = '';
    $('content').replaceChildren(node('p', 'Session ended. You can close this window.', 'empty'));
    document.querySelectorAll('button,input,select,textarea').forEach(input => { input.disabled = true; });
    message('');
  } catch (error) { message(error.message, true); }
});

// The session closes when idle. Typing counts, so it never ends under a person
// who is filling in a form; say so at most once a minute.
let lastActivity = Date.now();
for (const event of ['input', 'click']) {
  document.addEventListener(event, () => {
    if (state.ended || !state.credential || Date.now() - lastActivity < 60_000) return;
    lastActivity = Date.now();
    call('/api/activity', {}).catch(() => {});
  }, { passive: true });
}

try {
  if (!state.credential) throw new Error('Run octocode config to start a session.');
  const result = await call('/api/session', {});
  state.credential = result.token;
  await refresh();
} catch (error) {
  state.credential = undefined;
  message(error.message, true);
  $('content').replaceChildren();
}
