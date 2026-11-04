import { describe, it } from 'vitest';
import assert from 'node:assert/strict';
import { browser, files } from './config-view-fixture.js';

describe('config view keys', () => {
  it('credential setting replacement uses native setting endpoint and never shows stored value', async () => {
    const secret = 'synthetic_saved_value';
    const view = await browser({
      files,
      settings: [],
      keys: [
        {
          key: 'OCTOCODE_CLASSIFICATION_API',
          setting: 'classification.api',
          scope: 'home',
          source: 'home',
          set: true,
          secret: true,
          value: secret,
        },
      ],
    });
    assert.ok(
      !view
        .controls()
        .some(
          element =>
            element.value === secret || element.textContent.includes(secret)
        )
    );
    const passwords = view
      .controls()
      .filter(element => element.type === 'password');
    assert.ok(passwords.every(element => element.value === ''));
    await view.type('OCTOCODE_CLASSIFICATION_API value', 'new_value');
    await view.click('Replace');
    assert.deepEqual(
      view.requests.find(request => request.operation === 'setSetting'),
      {
        operation: 'setSetting',
        key: 'classification.api',
        value: 'new_value',
        scope: 'home',
        revision: 'hs',
      }
    );
    assert.ok(
      view
        .controls()
        .filter(element => element.type === 'password')
        .every(element => element.value === '')
    );
  });
});

describe('config view agents', () => {
  it('unsupported agent enable toggle is omitted and env removal preserves launch method', async () => {
    const view = await browser({ files, settings: [] }, [
      {
        client: 'cursor',
        scope: 'home',
        revision: 'a1',
        configured: true,
        writable: true,
        supportsEnabled: false,
        entry: { enabled: null, customCommand: true, envKeys: ['MY_KEY'] },
      },
    ]);
    assert.ok(!view.controls().some(element => element.type === 'checkbox'));
    await view.click('Save');
    assert.deepEqual(
      view.requests.find(request => request.operation === 'setAgent'),
      {
        operation: 'setAgent',
        client: 'cursor',
        scope: 'home',
        revision: 'a1',
        patch: {},
      }
    );
    await view.click('Remove key');
    assert.deepEqual(
      view.requests.filter(request => request.operation === 'setAgent').at(-1),
      {
        operation: 'setAgent',
        client: 'cursor',
        scope: 'home',
        revision: 'a1',
        patch: { env: { MY_KEY: null } },
      }
    );
  });

  it('supported agent toggle uses entry.enabled and supplied agent scope', async () => {
    const view = await browser({ files, settings: [] }, [
      {
        client: 'codex',
        scope: 'project',
        revision: 'a2',
        configured: true,
        writable: true,
        supportsEnabled: true,
        entry: { enabled: false, method: 'npx', envKeys: [] },
      },
    ]);
    const checkbox = view
      .controls()
      .find(element => element.type === 'checkbox')!;
    assert.equal(checkbox.checked, false);
    checkbox.checked = true;
    await view.click('Save');
    assert.deepEqual(
      view.requests.find(request => request.operation === 'setAgent'),
      {
        operation: 'setAgent',
        client: 'codex',
        scope: 'project',
        revision: 'a2',
        patch: { enabled: true },
      }
    );
  });
});

describe('config view agent install', () => {
  it('offers one row to install into any client not yet set up, and lists no forms for them', async () => {
    const view = await browser({ files, settings: [] }, [
      { client: 'cursor', scope: 'home', revision: 'a3', status: 'absent' },
      { client: 'windsurf', scope: 'home', revision: 'a4', status: 'absent' },
    ]);
    const options = view.labelled('Client to add Octocode to')!.children;
    assert.equal(options.length, 2);
    assert.equal(view.controls().filter(element => element.textContent === 'Install').length, 1);
    view.labelled('Client to add Octocode to')!.value = '1';
    await view.click('Install');
    assert.deepEqual(view.requests.find(request => request.operation === 'setAgent'), {
      operation: 'setAgent', client: 'windsurf', scope: 'home', revision: 'a4', patch: { method: 'npx' },
    });
  });
});
