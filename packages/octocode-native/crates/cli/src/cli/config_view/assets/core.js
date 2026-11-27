// Session, shared state, requests and the rows every view is built from.
export const state = { credential: window.location.hash.slice(1), snapshot: {}, agents: [], busy: false, ended: false };
history.replaceState(null, '', '/');
// Set by app.js: a saved change redraws the page.
export const hooks = { render() {} };

const SOURCES = {
  process: 'Shell or MCP environment',
  homeEnv: 'Home .env',
  workspaceEnv: 'Workspace .env',
  homeSettings: 'Home .octocoderc',
  workspaceSettings: 'Workspace .octocoderc',
  default: 'Default',
};

export const $ = id => document.getElementById(id);
export const scope = () => document.querySelector('input[name=scope]:checked').value;
export const query = () => $('search').value.trim().toLowerCase();

export function node(tag, text, className) {
  const element = document.createElement(tag);
  if (text !== undefined && text !== null) element.textContent = text;
  if (className) element.className = className;
  return element;
}
export function append(parent, ...children) { children.filter(Boolean).forEach(child => parent.append(child)); return parent; }
export function message(text, error = false) { $('message').textContent = text; $('message').className = error ? 'error' : ''; }

export async function call(path, payload) {
  const response = await fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Octocode-Session': state.credential }, body: JSON.stringify(payload), cache: 'no-store', credentials: 'omit', redirect: 'error' });
  const result = await response.json();
  if (!response.ok || result.error) {
    const error = new Error(result.error?.message ?? 'The request could not be completed.');
    error.code = result.error?.code;
    throw error;
  }
  return result;
}
const request = payload => call('/api/request', payload);

const fileFor = kind => state.snapshot.files?.[`${scope()}${kind}`];
const revision = kind => fileFor(kind)?.revision ?? null;
export const writable = kind => fileFor(kind)?.writable !== false;

export async function refresh() {
  const [config, installed] = await Promise.all([request({ operation: 'inspect' }), request({ operation: 'agents' })]);
  if (state.ended) return;
  state.snapshot = config;
  $('warnings').replaceChildren(...(config.warnings ?? []).map(warning => node('p', `${warning.key ?? warning.file ?? 'Configuration'}: ${warning.message ?? 'Review this configuration.'}${warning.source ? ` (${warning.source})` : ''}`, 'warning')));
  $('storage').textContent = `${config.storage?.message ?? 'Changes are saved on this computer.'} Restart agents after a change.`;
  state.agents = installed.agents ?? [];
  hooks.render();
}

export async function save(payload, success = 'Saved. Restart the MCP server or start a new agent session to apply it.') {
  if (state.busy || state.ended) return;
  state.busy = true;
  const disabled = new Map();
  document.querySelectorAll('button').forEach(item => { if (item === $('close')) return; disabled.set(item, item.disabled); item.disabled = true; });
  try {
    await request(payload);
    if (state.ended) return;
    // What this row saved is done, secrets first, even when the refresh fails.
    // Other rows keep what is typed in them: they share this one page.
    state.activeRow?.querySelectorAll('[data-dirty]').forEach(input => {
      if (input.type !== 'checkbox') input.value = '';
      delete input.dataset.dirty;
    });
    await refresh();
    if (!state.ended) message(success);
  } catch (error) {
    if (state.ended) return;
    message(error.message, true);
    if (error.code === 'CONFLICT') { try { await refresh(); } catch { /* Keep the conflict message. */ } }
  } finally {
    state.busy = false;
    if (!state.ended) document.querySelectorAll('button').forEach(item => { if (disabled.has(item)) item.disabled = disabled.get(item); });
  }
}

export function button(text, action, className = '') {
  const element = node('button', text, className);
  element.type = 'button';
  element.addEventListener('click', action);
  return element;
}
const defaultText = setting => (typeof setting.displayDefault === 'string' ? setting.displayDefault : JSON.stringify(setting.displayDefault ?? null));
export const matches = (...values) => !query() || values.some(value => String(value ?? '').toLowerCase().includes(query()));
export const settingMatches = setting => matches(setting.key, setting.env, setting.description, setting.sectionTitle);

function meta(pairs) {
  const element = node('div', undefined, 'row-meta');
  pairs.filter(([, value]) => value).forEach(([label, value]) => {
    const item = node('span');
    append(item, document.createTextNode(`${label} `), node('b', value));
    element.append(item);
  });
  return element;
}

function copyLink(url) {
  const line = node('div', undefined, 'link-line');
  // Plain text: following a link would leave this window and end the session.
  append(line, node('span', `Get a key: ${url}`), button('Copy link', async event => {
    try { await navigator.clipboard.writeText(url); event.target.textContent = 'Copied'; } catch { message(`Copy this link: ${url}`); }
  }, 'link'));
  return line;
}

function editor(setting, value, disabled, changed) {
  const label = `${setting.key} value`;
  const values = setting.values;
  let input;
  let element;
  if (setting.type === 'boolean') {
    input = node('input'); input.type = 'checkbox'; input.checked = value === true;
    element = node('label', undefined, 'toggle');
    const onOff = node('span', value === true ? 'On' : 'Off');
    append(element, input, node('span', undefined, 'track'), onOff);
    input.addEventListener('change', () => { onOff.textContent = input.checked ? 'On' : 'Off'; });
  } else if (Array.isArray(values)) {
    input = node('select');
    values.forEach(item => { const option = node('option', String(item)); option.value = String(item); input.append(option); });
    input.value = String(value ?? setting.defaultValue ?? '');
    element = input;
  } else if (setting.type === 'stringArray') {
    input = node('textarea');
    // Unset is an empty box, not the word null.
    const list = value ?? setting.defaultValue ?? null;
    input.value = list === null ? '' : JSON.stringify(list);
    input.placeholder = '["localSearch", "localFetch"]';
    input.spellcheck = false;
    element = input;
  } else {
    input = node('input'); input.type = 'text'; input.spellcheck = false; input.autocomplete = 'off';
    input.value = String(value ?? '');
    input.placeholder = typeof setting.displayDefault === 'string' ? setting.displayDefault : '';
    element = input;
  }
  input.setAttribute('aria-label', label);
  input.disabled = disabled;
  input.addEventListener(input.type === 'checkbox' || input.tagName === 'SELECT' ? 'change' : 'input', changed);
  return { input, element };
}

const nowText = value => {
  if (value === undefined || value === null) return 'unset';
  return typeof value === 'string' ? value : JSON.stringify(value);
};

/** A description that folds to two lines; a click shows all of it. */
function folded(text) {
  const element = node('p', text.replaceAll('`', ''), 'row-text');
  element.title = 'Click to show all';
  element.addEventListener('click', () => element.classList.toggle('open'));
  return element;
}

const stateMark = (text, kind) => node('span', text, `state ${kind ?? ''}`.trim());

function settingInfo(setting) {
  const name = append(node('div', undefined, 'row-name'), node('code', setting.key),
    setting.source === 'default' ? null : stateMark(SOURCES[setting.source] ?? setting.source, 'custom'),
    setting.workspaceAllowed === false ? stateMark('Home only') : null);
  return append(node('div', undefined, 'row-info'), name,
    folded([setting.description, setting.notes].filter(Boolean).join(' ')),
    meta([['Variable', setting.env]]));
}

/** What this scope holds for a setting and what it may do with it. */
function settingInScope(setting) {
  const home = scope() === 'home';
  const narrow = home ? [] : setting.workspaceNarrowValues ?? [];
  const saved = (home ? setting.homeValue : setting.workspaceValue) ?? setting.value;
  const locked = !home && !narrow.length && setting.workspaceAllowed === false;
  return {
    narrow,
    locked,
    isSet: home ? setting.homeSet : setting.workspaceSet,
    initial: narrow.length && !narrow.includes(saved) ? narrow[0] : saved,
    disabled: !writable('Settings') || locked,
  };
}

function editedValue(setting, input) {
  if (setting.type === 'boolean') return input.checked;
  if (setting.type !== 'stringArray') return input.value;
  if (!input.value.trim()) return null;
  const value = JSON.parse(input.value);
  if (value !== null && (!Array.isArray(value) || value.some(item => typeof item !== 'string'))) throw new Error('Enter a JSON array of strings, or leave it empty to inherit.');
  return value;
}

export function settingRow(setting) {
  const { narrow, locked, isSet, initial, disabled } = settingInScope(setting);
  const target = { key: setting.key, scope: scope(), revision: revision('Settings') };
  const apply = button('Save', () => {
    try {
      void save({ operation: 'setSetting', ...target, value: editedValue(setting, field.input) });
    } catch (error) { message(error.message, true); }
  }, 'primary');
  const field = editor({ ...setting, ...(narrow.length ? { values: narrow } : {}) }, initial, disabled, () => { apply.disabled = disabled; });
  // A narrowing value differs from what applies, and its one-option list cannot
  // change, so it is saveable from the start.
  apply.disabled = disabled || !narrow.length || Boolean(isSet);
  const reset = isSet ? button('Reset', () => void save({ operation: 'removeSetting', ...target })) : null;
  if (reset) reset.disabled = !writable('Settings');
  const value = append(node('div', undefined, 'row-value'), field.element, meta([['Default', defaultText(setting)], ['Now', nowText(setting.value)]]));
  if (locked) value.append(node('p', 'Set this in Home: a workspace cannot change it.', 'note'));
  if (narrow.length) value.append(node('p', `A workspace can only set ${narrow.join(', ')}.`, 'note'));
  return append(node('div', undefined, 'row'), settingInfo(setting), value, append(node('div', undefined, 'row-actions'), apply, reset));
}

/** Where a key or variable is set: this scope's file, the other scope's, or the shell. */
function keyState(known, entry, elsewhere) {
  if (entry && entry.set !== false) return stateMark(`Set in ${scope()}`, 'on');
  if (elsewhere) return stateMark(`Set in ${elsewhere.scope}`, 'on');
  if (known.source === 'env') return stateMark('Set in shell', 'on');
  return stateMark('Not set');
}

function keyInfo(key, description, known, entry) {
  const elsewhere = (state.snapshot.keys ?? []).find(item => item.key === key && item.scope !== scope() && item.set !== false);
  const name = append(node('div', undefined, 'row-name'), node('code', key), keyState(known, entry, elsewhere),
    known.writableFrom === 'home' ? stateMark('Home only') : null);
  return append(node('div', undefined, 'row-info'), name,
    description ? folded(description) : null,
    known.docsUrl ? copyLink(known.docsUrl) : null);
}

function keyInput(key, known, isSet) {
  const secret = known.credential !== false;
  const input = node('input');
  input.type = secret ? 'password' : 'text';
  input.autocomplete = secret ? 'new-password' : 'off';
  input.spellcheck = false;
  const empty = secret ? 'Paste the value' : known.default ?? '';
  input.placeholder = isSet ? 'Enter a new value to replace it' : empty;
  input.setAttribute('aria-label', `${key} value`);
  return input;
}

export function keyRow(key, description, known = {}) {
  const entry = (state.snapshot.keys ?? []).find(item => item.key === key && item.scope === scope());
  const isSet = entry && entry.set !== false;
  const stored = entry?.setting;
  const kind = stored ? 'Settings' : 'Env';
  const locked = scope() === 'workspace' && known.writableFrom === 'home';
  const blocked = !writable(kind) || locked;
  const input = keyInput(key, known, isSet);
  input.disabled = blocked;
  const target = { key: stored ?? key, scope: scope(), revision: revision(kind) };
  const saveKey = button(isSet ? 'Replace' : 'Save', () => void save({ operation: stored ? 'setSetting' : 'setEnv', ...target, value: input.value }, 'Saved. The value stays hidden. Restart the MCP server or start a new agent session to apply it.'), 'primary');
  saveKey.disabled = true;
  input.addEventListener('input', () => { saveKey.disabled = !input.value || blocked; });
  const remove = isSet ? button('Remove', () => void save({ operation: stored ? 'removeSetting' : 'removeEnv', ...target }), 'danger') : null;
  if (remove) remove.disabled = !writable(kind);
  const value = append(node('div', undefined, 'row-value'), input, known.credential === false ? meta([['Default', known.default]]) : null);
  if (!writable(kind)) value.append(node('p', 'This file is read only. Repair it before editing.', 'note'));
  if (locked) value.append(node('p', 'Set this in Home: a repository must not choose where your key goes.', 'note'));
  return append(node('div', undefined, 'row'), keyInfo(key, description, known, entry), value, append(node('div', undefined, 'row-actions'), saveKey, remove));
}

/** A variable only the process may set: shown, never edited here. */
export function processRow(known) {
  const where = known.source === 'env' ? stateMark('Set in shell', 'on') : stateMark('Not set');
  const info = append(node('div', undefined, 'row-info'), append(node('div', undefined, 'row-name'), node('code', known.key), where), folded(known.description));
  const value = append(node('div', undefined, 'row-value'), node('code', nowText(known.value), 'row-meta'), meta([['Default', typeof known.default === 'string' ? known.default : 'unset']]));
  return append(node('div', undefined, 'row'), info, value, node('div', undefined, 'row-actions'));
}

/** Rows under an optional small heading, for one shared table. */
export function group(title, rows) {
  const present = rows.filter(Boolean);
  if (!present.length) return [];
  return [title ? node('div', title, 'subhead') : null, ...present].filter(Boolean);
}
