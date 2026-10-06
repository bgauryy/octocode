let credential = window.location.hash.slice(1);
history.replaceState(null, '', '/');
let snapshot = {};
let agents = [];
let tab = 'settings';
let busy = false;
let ended = false;
const numeric = setting => ['number', 'schemaVersion'].includes(setting.type);
const $ = id => document.getElementById(id);
const scope = () => $('scope').value;
function node(tag, text, className) {
  const element = document.createElement(tag);
  if (text !== undefined) element.textContent = text;
  if (className) element.className = className;
  return element;
}
function message(text, error = false) { $('message').textContent = text; $('message').className = error ? 'error' : ''; }
async function call(path, payload) {
  const response = await fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Octocode-Session': credential }, body: JSON.stringify(payload), cache: 'no-store', credentials: 'omit', redirect: 'error' });
  const result = await response.json();
  if (!response.ok || result.error) {
    const error = new Error(result.error?.message ?? 'The request could not be completed.');
    error.code = result.error?.code;
    throw error;
  }
  return result;
}
const request = payload => call('/api/request', payload);
function revision(kind) {
  const file = snapshot.files?.[`${scope()}${kind}`];
  return file?.revision ?? null;
}
function writable(kind) { return snapshot.files?.[`${scope()}${kind}`]?.writable !== false; }
function fileNotice(kind) {
  if (!writable(kind)) $('content').append(node('p', `The ${scope()} ${kind.toLowerCase()} file is read only. Repair the file or its path before editing.`, 'error'));
}
async function refresh() {
  const [config, installed] = await Promise.all([request({ operation: 'inspect' }), request({ operation: 'agents' })]);
  if (ended) return;
  snapshot = config;
  $('warnings').replaceChildren(...(config.warnings ?? []).map(warning => node('p', `${warning.key ?? warning.file ?? 'Configuration'}: ${warning.message ?? 'Review this configuration.'}${warning.source ? ` (${warning.source})` : ''}`, 'error')));
  $('storage').textContent = `${config.storage?.message ?? 'Changes are saved on this computer.'} Agent changes may require restarting the agent.`;
  agents = installed.agents ?? [];
  render();
}
async function save(payload, success = 'Configuration saved.') {
  if (busy || ended) return;
  busy = true;
  const disabledStates = new Map();
  document.querySelectorAll('button').forEach(button => { if (button === $('close')) return; disabledStates.set(button, button.disabled); button.disabled = true; });
  try {
    await request(payload);
    if (ended) return;
    // Clear replacement fields before refreshing, including when refresh fails.
    document.querySelectorAll('input[type=password]').forEach(input => { input.value = ''; });
    await refresh();
    if (!ended) message(success);
  } catch (error) {
    if (ended) return;
    message(error.message, true);
    if (error.code === 'CONFLICT') { try { await refresh(); } catch { /* Keep the conflict message. */ } }
  } finally {
    busy = false;
    if (!ended) document.querySelectorAll('button').forEach(button => { if (disabledStates.has(button)) button.disabled = disabledStates.get(button); });
  }
}
function button(text, action, className = '') {
  const element = node('button', text, className);
  element.type = 'button';
  element.addEventListener('click', action);
  return element;
}
function card(title, description) {
  const element = node('article', undefined, 'card');
  element.append(node('h2', title));
  if (description) element.append(node('p', description));
  return element;
}
function detail(element, text) { element.append(node('p', text, 'details')); }
function matches(item) { return JSON.stringify(item).toLowerCase().includes($('search').value.toLowerCase()); }
function editor(setting) {
  let input;
  const values = setting.values ?? setting.enum;
  if (setting.type === 'boolean') {
    input = node('input'); input.type = 'checkbox'; input.checked = setting.value === true;
  } else if (Array.isArray(values)) {
    input = node('select');
    values.forEach(value => { const option = node('option', String(value)); option.value = String(value); input.append(option); });
    input.value = String(setting.value ?? setting.defaultValue ?? '');
  } else {
    input = node(setting.type === 'stringArray' ? 'textarea' : 'input');
    if (input.tagName === 'INPUT') input.type = numeric(setting) ? 'number' : 'text';
    if (setting.minimum !== undefined) input.min = setting.minimum;
    if (setting.maximum !== undefined) input.max = setting.maximum;
    input.value = setting.type === 'stringArray' ? JSON.stringify(setting.value === undefined ? setting.defaultValue ?? null : setting.value) : String(setting.value ?? setting.defaultValue ?? '');
    if (setting.type === 'stringArray') input.placeholder = '["value"] or null to inherit';
  }
  input.setAttribute('aria-label', `${setting.key} value`);
  return input;
}
function settingsView() {
  fileNotice('Settings');
  const entries = snapshot.settings ?? [];
  entries.filter(matches).forEach(setting => {
    if (setting.credential) return;
    const key = setting.key ?? setting.name;
    const element = card(key, setting.description);
    detail(element, `Effective source: ${setting.source ?? 'default'}`);
    const effective = setting.value === undefined ? 'Unset' : JSON.stringify(setting.value);
    detail(element, `Effective value: ${effective}`);
    const row = node('div', undefined, 'row');
    const value = node('div', undefined, 'value');
    const saved = scope() === 'home' ? setting.homeValue : setting.workspaceValue;
    const narrow = scope() === 'workspace' ? setting.workspaceNarrowValues ?? [] : [];
    const input = editor({ ...setting, key, ...(narrow.length ? { values: narrow } : {}), value: narrow.length && !narrow.includes(saved ?? setting.value) ? narrow[0] : saved ?? setting.value });
    value.append(input);
    const disabled = !writable('Settings') || scope() === 'workspace' && !narrow.length && (setting.workspaceAllowed === false || setting.dotenv === 'home' || setting.dotenv === 'never');
    input.disabled = disabled;
    const apply = button('Save', () => {
      try {
        const value = setting.type === 'boolean' ? input.checked : numeric(setting) ? Number(input.value) : setting.type === 'stringArray' ? JSON.parse(input.value) : input.value;
        if (numeric(setting) && (input.value === '' || !Number.isFinite(value) || !input.checkValidity())) throw new Error('Enter a valid number within the allowed range.');
        if (setting.type === 'stringArray' && value !== null && (!Array.isArray(value) || value.some(item => typeof item !== 'string'))) throw new Error('Enter a JSON array of strings, or null to inherit.');
        void save({ operation: 'setSetting', key, value, scope: scope(), revision: revision('Settings') });
      } catch (error) { message(error.message, true); }
    }, 'primary');
    const reset = button('Use inherited value', () => void save({ operation: 'removeSetting', key, scope: scope(), revision: revision('Settings') }));
    apply.disabled = disabled;
    reset.disabled = !writable('Settings');
    row.append(value, apply, reset); element.append(row);
    if (narrow.length) detail(element, `This workspace can only use: ${narrow.join(', ')}.`);
    if (disabled && writable('Settings')) detail(element, 'This setting is controlled from home or the launching environment.');
    $('content').append(element);
  });
}
function secretInput(labelText) {
  const label = node('label', labelText);
  const input = node('input'); input.type = 'password'; input.autocomplete = 'off'; input.spellcheck = false;
  label.append(input);
  return { label, input };
}
function keysView() {
  fileNotice('Env');
  const add = card('Add a key', 'Enter a key name and its new value. Saved values stay hidden.');
  const fields = node('div', undefined, 'fields');
  const nameLabel = node('label', 'Key name');
  const name = node('input'); name.autocomplete = 'off'; name.placeholder = 'MY_API_KEY'; nameLabel.append(name);
  const secret = secretInput('New value'); fields.append(nameLabel, secret.label); add.append(fields);
  const addKey = button('Save key', () => {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name.value)) { message('Enter a valid environment variable name.', true); return; }
    void save({ operation: 'setEnv', key: name.value, value: secret.input.value, scope: scope(), revision: revision('Env') });
  }, 'primary');
  addKey.disabled = !writable('Env');
  add.append(addKey);
  $('content').append(add);
  (snapshot.keys ?? []).filter(entry => (!entry.scope || entry.scope === scope()) && matches(entry)).forEach(entry => {
    const key = entry.key ?? entry.name;
    const element = card(key);
    detail(element, `${entry.set === false ? 'Not set' : 'Set'} · ${entry.source ?? entry.scope ?? 'configuration'}`);
    const row = node('div', undefined, 'row');
    const secret = secretInput('Replacement');
    secret.label.className = 'value';
    const identity = { key: entry.setting ?? key, scope: scope(), revision: revision(entry.setting ? 'Settings' : 'Env') };
    if (entry.setting) detail(element, `Stored setting: ${entry.setting}`);
    const replace = button('Replace', () => void save({ operation: entry.setting ? 'setSetting' : 'setEnv', ...identity, value: secret.input.value }), 'primary');
    const remove = button('Remove', () => void save({ operation: entry.setting ? 'removeSetting' : 'removeEnv', ...identity }), 'danger');
    replace.disabled = remove.disabled = !writable(entry.setting ? 'Settings' : 'Env');
    if (replace.disabled) detail(element, 'The file containing this key is read only. Repair it before editing.');
    row.append(secret.label, replace, remove);
    element.append(row); $('content').append(element);
  });
}
function agentsView() {
  agents.filter(matches).forEach(agent => {
    const element = card(agent.title ?? agent.client);
    detail(element, `${agent.scope ?? 'home'} · ${agent.status ?? (agent.installed ? 'Installed' : 'Available')}`);
    detail(element, agent.path ?? '');
    if (agent.error) detail(element, agent.error);
    const base = { client: agent.client, scope: agent.scope ?? 'home', revision: agent.revision ?? null };
    const installed = agent.configured ?? agent.installed ?? ['configured', 'installed', 'enabled', 'disabled'].includes(agent.status);
    const readOnly = agent.writable === false || agent.readOnly || agent.managed;
    const row = node('div', undefined, 'row');
    const label = node('label', 'Enabled');
    const enabled = node('input'); enabled.type = 'checkbox'; enabled.checked = (agent.entry?.enabled ?? agent.enabled) !== false; enabled.disabled = Boolean(readOnly); label.append(enabled);
    if (agent.supportsEnabled === true) row.append(label);
    const methodLabel = node('label', 'Launch with');
    const method = node('select');
    if (installed) { const option = node('option', 'Keep current command'); option.value = ''; method.append(option); }
    for (const name of ['npx', 'bunx', 'pnpm']) { const option = node('option', name); option.value = name; method.append(option); }
    method.value = installed ? '' : 'npx'; method.disabled = Boolean(readOnly); methodLabel.append(method); row.append(methodLabel);
    if (agent.entry?.customCommand) detail(element, 'Custom launch command configured.');
    else if (agent.entry?.method) detail(element, `Current launch method: ${agent.entry.method}`);
    const update = button(installed ? 'Save / update' : 'Install', () => void save({ operation: 'setAgent', ...base, patch: { ...(agent.supportsEnabled === true ? { enabled: enabled.checked } : {}), ...(method.value ? { method: method.value } : {}) } }), 'primary');
    update.disabled = Boolean(readOnly); row.append(update);
    if (installed) {
      const remove = button('Remove Octocode', () => void save({ operation: 'removeAgent', ...base }), 'danger');
      remove.disabled = Boolean(readOnly); row.append(remove);
    }
    element.append(row);
    if (readOnly) detail(element, 'This configuration source is read only.');
    else {
      const controls = node('div', undefined, 'fields agent-secret');
      const label = node('label', 'Environment key'); const key = node('input'); key.placeholder = 'MY_API_KEY'; label.append(key);
      const secret = secretInput('New value'); controls.append(label, secret.label);
      controls.append(button('Save agent key', () => {
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key.value)) { message('Enter a valid environment variable name.', true); return; }
        void save({ operation: 'setAgent', ...base, patch: { env: { [key.value]: secret.input.value } } });
      }));
      const names = agent.entry?.envKeys ?? (Array.isArray(agent.envKeys) ? agent.envKeys : []);
      names.forEach(name => {
        const row = node('div', undefined, 'row');
        row.append(node('span', name, 'value'), button('Remove key', () => void save({ operation: 'setAgent', ...base, patch: { env: { [name]: null } } }), 'danger'));
        controls.append(row);
      });
      element.append(controls);
    }
    $('content').append(element);
  });
}
function render() {
  $('content').replaceChildren();
  if (tab === 'settings') settingsView();
  else if (tab === 'keys') keysView();
  else agentsView();
  if (!$('content').children.length) $('content').append(node('p', 'No matching entries.', 'empty'));
}
document.querySelectorAll('[data-tab]').forEach(button => button.addEventListener('click', () => {
  tab = button.dataset.tab;
  document.querySelectorAll('[data-tab]').forEach(item => item.setAttribute('aria-pressed', String(item === button)));
  render();
}));
$('scope').addEventListener('change', render);
$('search').addEventListener('input', render);
$('refresh').addEventListener('click', async () => { try { await refresh(); if (!ended) message('Configuration refreshed.'); } catch (error) { if (!ended) message(error.message, true); } });
$('close').addEventListener('click', async () => {
  try { await call('/api/close', {}); ended = true; credential = undefined; $('warnings').replaceChildren(); $('content').replaceChildren(node('p', 'Session ended. You can close this tab.')); document.querySelectorAll('button,input,select').forEach(input => { input.disabled = true; }); message(''); }
  catch (error) { message(error.message, true); }
});
try {
  if (!credential) throw new Error('Open the link from octocode config view to start a session.');
  const result = await call('/api/session', {}); credential = result.token;
  await refresh();
} catch (error) { credential = undefined; message(error.message, true); $('content').replaceChildren(); }
