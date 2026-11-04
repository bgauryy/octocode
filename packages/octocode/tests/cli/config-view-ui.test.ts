import { describe, it } from 'vitest';
import assert from 'node:assert/strict';
import { browser, files } from './config-view-fixture.js';

describe('config view browser app', () => {
  it('shows sanitized configuration warnings with their key and source as text', async () => {
    const view = await browser({
      files,
      settings: [],
      warnings: [
        {
          key: 'obsolete',
          source: '/fixture/.octocoderc',
          message: 'Unsupported configuration; values omitted.',
        },
      ],
    });
    assert.equal(view.ids.warnings!.children[0]!.tagName, 'P');
    assert.equal(
      view.ids.warnings!.children[0]!.textContent,
      'obsolete: Unsupported configuration; values omitted. (/fixture/.octocoderc)'
    );
  });

  it('End session remains available while a save is pending', async () => {
    let finish!: () => void;
    const pending = new Promise<void>(resolve => {
      finish = resolve;
    });
    const view = await browser(
      {
        files,
        settings: [{ key: 'local.enabled', type: 'boolean', value: true }],
      },
      [],
      async payload => {
        if (payload.operation === 'setSetting') await pending;
      }
    );
    await view.type('local.enabled value', true, 'change');
    await view.click('Save');
    assert.notEqual(view.ids.close!.disabled, true);
    assert.equal(
      view.controls().find(element => element.textContent === 'Save')!.disabled,
      true
    );
    await view.ids.close!.fire();
    assert.ok(
      view
        .controls()
        .some(element => element.textContent.includes('Session ended'))
    );
    finish();
    await new Promise(resolve => setImmediate(resolve));
    assert.ok(
      view
        .controls()
        .some(element => element.textContent.includes('Session ended'))
    );
    assert.equal(view.ids.close!.disabled, true);
  });

  it('read-only settings and env files disable their write controls and explain why', async () => {
    const view = await browser({
      files: {
        ...files,
        homeSettings: { writable: false },
        homeEnv: { writable: false },
      },
      settings: [{ key: 'local.enabled', type: 'boolean', value: true }],
      keys: [
        { key: 'MY_KEY', scope: 'home', set: true },
        { key: 'API', setting: 'classification.api', scope: 'home', set: true },
      ],
    });
    // Every button on the page, Remove included: a read-only file cannot change.
    assert.ok(
      view.controls().some(element => element.textContent.includes('read only'))
    );
    assert.ok(
      view
        .controls()
        .filter(element => element.tagName === 'BUTTON')
        .every(element => element.disabled === true)
    );
    assert.equal(
      view.requests.some(request =>
        String(request.operation).startsWith('set')
      ),
      false
    );
  });

  it('nullable array editor preserves null and passes correct scope/revision', async () => {
    const view = await browser({
      files,
      settings: [
        {
          key: 'tools.enabled',
          type: 'stringArray',
          value: null,
          homeValue: null,
          workspaceValue: null,
          source: 'default',
        },
      ],
    });
    const input = view
      .controls()
      .find(element => element.tagName === 'TEXTAREA')!;
    // Unset is an empty box; saving it empty stays null.
    assert.equal(input.value, '');
    await view.type('tools.enabled value', '');
    await view.click('Save');
    assert.deepEqual(
      view.requests.find(request => request.operation === 'setSetting'),
      {
        operation: 'setSetting',
        key: 'tools.enabled',
        value: null,
        scope: 'home',
        revision: 'hs',
      }
    );
  });
  it('a boolean setting saves as a boolean and exposes local key storage', async () => {
    const view = await browser({
      files,
      storage: { message: 'Keys use local .env files.' },
      settings: [
        {
          key: 'local.enabled',
          type: 'boolean',
          value: true,
          workspaceAllowed: true,
        },
      ],
    });
    assert.equal(
      view.ids.storage.textContent,
      'Keys use local .env files. Restart agents after a change.'
    );
    await view.type('local.enabled value', true, 'change');
    await view.click('Save');
    assert.equal(
      view.requests.find(request => request.operation === 'setSetting')!.value,
      true
    );
  });

  it('workspace narrowing remains editable and only offers allowed enum values', async () => {
    const view = await browser({
      files,
      settings: [
        {
          key: 'storage.mode',
          type: 'enum',
          value: 'persistent',
          values: ['persistent', 'memory'],
          workspaceAllowed: false,
          workspaceNarrowValues: ['memory'],
        },
      ],
    });
    view.ids.scope.value = 'workspace';
    await view.ids.scope.fire('change');
    const select = view
      .controls()
      .find(element => element.tagName === 'SELECT')!;
    assert.deepEqual(
      select.children.map(element => element.value),
      ['memory']
    );
    assert.equal(select.value, 'memory');
    assert.equal(select.disabled, false);
    await view.click('Save');
    assert.equal(
      view.requests.find(request => request.operation === 'setSetting')!.value,
      'memory'
    );
  });

  it('workspace restrictions block setting a value but allow removing a stale override', async () => {
    const view = await browser({
      files,
      settings: [
        {
          key: 'local.allowedPaths',
          type: 'stringArray',
          value: [],
          workspaceValue: ['/stale'],
          workspaceSet: true,
          workspaceAllowed: false,
        },
      ],
    });
    view.ids.scope.value = 'workspace';
    await view.ids.scope.fire('change');
    assert.equal(
      view.controls().find(element => element.textContent === 'Save')!.disabled,
      true
    );
    assert.notEqual(
      view
        .controls()
        .find(element => element.textContent === 'Reset')!
        .disabled,
      true
    );
    await view.click('Reset');
    assert.deepEqual(
      view.requests.find(request => request.operation === 'removeSetting'),
      {
        operation: 'removeSetting',
        key: 'local.allowedPaths',
        scope: 'workspace',
        revision: 'ws',
      }
    );
  });
});

const text = (view: Awaited<ReturnType<typeof browser>>) =>
  view.controls().map(element => element.textContent).join(' ');

describe('one page', () => {
  const contract = {
    files,
    settings: [
      { key: 'lsp.prewarm', type: 'enum', value: 'targeted', values: ['targeted', 'off'], sectionTitle: 'LSP', description: 'Prewarm', displayDefault: 'targeted', env: 'OCTOCODE_LSP_PREWARM', source: 'default', homeValue: null },
      { key: 'classification.apiHost', type: 'url', value: null, sectionTitle: 'Classification provider', group: 'Classification', description: 'Host', displayDefault: 'https://api.typesafe.ai', source: 'default' },
    ],
    knownKeys: [
      { key: 'TAVILY_API_KEY', group: 'Web search', credential: true, description: 'Tavily', writableFrom: 'any', docsUrl: 'https://tavily.example/' },
      { key: 'TAVILY_BASE_URL', group: 'Web search', credential: false, description: 'Endpoint', default: 'https://api.tavily.example', writableFrom: 'home' },
      { key: 'OCTOCODE_CLASSIFICATION_API', group: 'Classification', credential: true, description: 'Classification', writableFrom: 'any' },
      { key: 'HTTPS_PROXY', credential: true, description: 'Proxy', writableFrom: 'process', default: 'unset', source: 'env', value: '[REDACTED-HTTPS_PROXY]' },
      { key: 'OCTOCODE_CARGO', credential: false, description: 'Cargo', writableFrom: 'home', default: 'cargo on PATH' },
    ],
  };
  it('shows every key, setting and environment variable at once, with no navigation', async () => {
    const view = await browser(contract);
    for (const name of ['TAVILY_API_KEY', 'TAVILY_BASE_URL', 'OCTOCODE_CLASSIFICATION_API', 'HTTPS_PROXY', 'OCTOCODE_CARGO', 'lsp.prewarm', 'classification.apiHost']) {
      assert.ok(text(view).includes(name), name);
    }
    assert.deepEqual(
      view.controls().filter(element => element.className === 'page-section').map(element => element.id),
      ['keys', 'settings', 'environment']
    );
  });

  it('lists keys first and keeps a setting with its provider only', async () => {
    const view = await browser(contract);
    const names = view.controls().filter(element => element.tagName === 'CODE').map(element => element.textContent);
    assert.equal(names[0], 'TAVILY_API_KEY');
    assert.equal(names.filter(name => name === 'classification.apiHost').length, 1);
  });

  it('shows a process-only variable masked, with nowhere to type it', async () => {
    const view = await browser(contract);
    assert.equal(view.labelled('HTTPS_PROXY value'), undefined);
    assert.ok(text(view).includes('[REDACTED-HTTPS_PROXY]'));
    assert.ok(text(view).includes('Set in shell'));
    assert.ok(view.labelled('OCTOCODE_CARGO value'));
  });

  it('offers Save only after a change', async () => {
    const view = await browser(contract);
    const save = () => view.controls().filter(element => element.tagName === 'BUTTON' && element.textContent === 'Save');
    assert.ok(save().every(element => element.disabled === true));
    await view.type('TAVILY_API_KEY value', 'typed');
    assert.equal(save().filter(element => element.disabled !== true).length, 1);
  });

  it('saving one row keeps what is typed in another and clears its own', async () => {
    const view = await browser(contract);
    await view.type('OCTOCODE_CARGO value', '/typed/cargo');
    await view.type('TAVILY_API_KEY value', 'typed-secret');
    await view.labelled('TAVILY_API_KEY value')!.parent!.parent!.children.at(-1)!.children[0]!.fire();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(view.labelled('OCTOCODE_CARGO value')!.value, '/typed/cargo');
    assert.equal(view.labelled('TAVILY_API_KEY value')!.value, '');
    assert.deepEqual(view.requests.find(request => request.operation === 'setEnv'), {
      operation: 'setEnv', key: 'TAVILY_API_KEY', scope: 'home', revision: 'he', value: 'typed-secret',
    });
  });

  it('searching keeps a value typed in a row it hides', async () => {
    const view = await browser(contract);
    await view.type('OCTOCODE_CARGO value', '/typed/cargo');
    view.ids.search!.value = 'tavily';
    await view.ids.search!.fire('input');
    assert.equal(view.labelled('OCTOCODE_CARGO value'), undefined);
    view.ids.search!.value = '';
    await view.ids.search!.fire('input');
    assert.equal(view.labelled('OCTOCODE_CARGO value')!.value, '/typed/cargo');
  });

  it('switching scope drops typed values and locks a home-only endpoint', async () => {
    const view = await browser(contract);
    await view.type('TAVILY_API_KEY value', 'typed-for-home');
    view.ids.scope.value = 'workspace';
    await view.ids.scope.fire('change');
    assert.equal(view.labelled('TAVILY_API_KEY value')!.value, '');
    assert.equal(view.labelled('TAVILY_BASE_URL value')!.disabled, true);
    assert.ok(text(view).includes('must not choose where your key goes'));
  });
});
