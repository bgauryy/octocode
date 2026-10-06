import { describe, it } from 'vitest';
import assert from 'node:assert/strict';
import appJs from '../../src/cli/config-view/assets/app.js?raw';

/** Minimal DOM stand-in: app.js assigns arbitrary element properties. */
class Element {
  [key: string]: unknown;
  tagName: string;
  children: Element[] = [];
  listeners = new Map<string, () => unknown>();
  value: unknown = '';
  textContent = '';
  dataset: Record<string, string> = {};
  attributes: Record<string, string> = {};
  constructor(tag = 'div') {
    this.tagName = tag.toUpperCase();
  }
  append(...children: Element[]): void {
    this.children.push(...children);
  }
  replaceChildren(...children: Element[]): void {
    this.children = children;
  }
  setAttribute(key: string, value: string): void {
    this.attributes[key] = value;
  }
  addEventListener(event: string, listener: () => unknown): void {
    this.listeners.set(event, listener);
  }
  checkValidity(): boolean {
    return true;
  }
  async fire(event = 'click'): Promise<void> {
    await this.listeners.get(event)?.();
  }
}
function descendants(element: Element): Element[] {
  return [element, ...element.children.flatMap(descendants)];
}
type Request = Record<string, unknown>;
async function browser(
  config: Record<string, unknown>,
  agents: unknown[] = [],
  onRequest?: (payload: Request) => Promise<void>
) {
  const ids: Record<string, Element> = Object.fromEntries(
    [
      'scope',
      'search',
      'message',
      'content',
      'refresh',
      'close',
      'storage',
      'warnings',
    ].map(id => [id, new Element()])
  );
  ids.scope!.value = 'home';
  ids.close!.tagName = 'BUTTON';
  const tabs = ['settings', 'keys', 'agents'].map(tab => {
    const button = new Element('button');
    button.dataset.tab = tab;
    return button;
  });
  const all = (): Element[] => [
    ...Object.values(ids).flatMap(descendants),
    ...tabs,
  ];
  const document = {
    getElementById: (id: string) => ids[id],
    createElement: (tag: string) => new Element(tag),
    querySelectorAll: (selector: string) => {
      if (selector === '[data-tab]') return tabs;
      if (selector === 'input[type=password]')
        return all().filter(element => element.type === 'password');
      return all().filter(element =>
        selector.split(',').includes(element.tagName.toLowerCase())
      );
    },
  };
  const requests: Request[] = [];
  let fragmentRemoved = false;
  const fetch = async (url: string, options: { body: string }) => {
    const payload = JSON.parse(options.body) as Request;
    if (url === '/api/session')
      return { ok: true, json: async () => ({ token: 'session' }) };
    requests.push(payload);
    await onRequest?.(payload);
    const result =
      payload.operation === 'inspect'
        ? config
        : payload.operation === 'agents'
          ? { agents }
          : { ok: true };
    return { ok: true, json: async () => result };
  };
  const AsyncFunction = Object.getPrototypeOf(async function () {})
    .constructor as new (
    ...args: string[]
  ) => (...args: unknown[]) => Promise<void>;
  const run = new AsyncFunction(
    'document',
    'window',
    'history',
    'fetch',
    appJs
  );
  await run(
    document,
    { location: { hash: '#bootstrap' } },
    {
      replaceState() {
        fragmentRemoved = true;
      },
    },
    fetch
  );
  assert.equal(fragmentRemoved, true);
  return {
    ids: ids as Record<string, Element> & { scope: Element; storage: Element },
    requests,
    tab: async (name: string) =>
      tabs.find(tab => tab.dataset.tab === name)!.fire(),
    controls: () => descendants(ids.content!),
    click: async (name: string) => {
      const button = descendants(ids.content!).find(
        element => element.tagName === 'BUTTON' && element.textContent === name
      );
      assert.ok(button, `Missing button ${name}`);
      await button.fire();
      await new Promise(resolve => setImmediate(resolve));
    },
  };
}

const files = {
  homeSettings: { revision: 'hs' },
  workspaceSettings: { revision: 'ws' },
  homeEnv: { revision: 'he' },
  workspaceEnv: { revision: 'we' },
};

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
        settings: [{ key: 'version', type: 'schemaVersion', value: 1 }],
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
      settings: [{ key: 'version', type: 'schemaVersion', value: 1 }],
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

  it('schema version saves as a number and exposes local key storage', async () => {
    const view = await browser({
      files,
      storage: { message: 'Keys use local .env files.' },
      settings: [
        {
          key: 'version',
          type: 'schemaVersion',
          value: 1,
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
      1
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
