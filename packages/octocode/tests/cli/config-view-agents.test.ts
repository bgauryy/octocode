import { describe, it } from 'vitest';
import assert from 'node:assert/strict';
import { browser, files } from './config-view-fixture.js';

describe('config view keys tab', () => {
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
    await view.tab('keys');
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
    passwords.at(-1)!.value = 'new_value';
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

describe('config view agents tab', () => {
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
    await view.tab('agents');
    assert.ok(!view.controls().some(element => element.type === 'checkbox'));
    await view.click('Save / update');
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
    await view.tab('agents');
    const checkbox = view
      .controls()
      .find(element => element.type === 'checkbox')!;
    assert.equal(checkbox.checked, false);
    checkbox.checked = true;
    await view.click('Save / update');
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
