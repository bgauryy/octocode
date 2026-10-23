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
    assert.ok(
      view.controls().some(element => element.textContent.includes('read only'))
    );
    assert.ok(
      view
        .controls()
        .filter(element => element.tagName === 'BUTTON')
        .every(element => element.disabled === true)
    );
    await view.tab('keys');
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
    assert.equal(input.value, 'null');
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
      'Keys use local .env files. Agent changes may require restarting the agent.'
    );
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
        .find(element => element.textContent === 'Use inherited value')!
        .disabled,
      true
    );
    await view.click('Use inherited value');
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
