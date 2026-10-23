// Runs the native config view's browser app (app.js) against a minimal DOM and a
// recording fetch, so tests drive it through its controls and read the requests it sends.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const appJs = readFileSync(
  new URL(
    '../../../octocode-native/crates/cli/src/cli/config_view/assets/app.js',
    import.meta.url
  ),
  'utf8'
);

/** Minimal DOM stand-in: app.js assigns arbitrary element properties. */
export class Element {
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

export function descendants(element: Element): Element[] {
  return [element, ...element.children.flatMap(descendants)];
}

type Request = Record<string, unknown>;
type Ids = Record<string, Element> & {
  scope: Element;
  close: Element;
  content: Element;
  storage: Element;
};
type AppFunction = new (
  ...args: string[]
) => (...args: unknown[]) => Promise<void>;

export const files = {
  homeSettings: { revision: 'hs' },
  workspaceSettings: { revision: 'ws' },
  homeEnv: { revision: 'he' },
  workspaceEnv: { revision: 'we' },
};

function pageElements(): Ids {
  const ids: Ids = {
    scope: new Element(),
    search: new Element(),
    message: new Element(),
    content: new Element(),
    refresh: new Element(),
    close: new Element(),
    storage: new Element(),
    warnings: new Element(),
  };
  ids.scope.value = 'home';
  ids.close.tagName = 'BUTTON';
  return ids;
}

function fakeDocument(ids: Ids, tabs: Element[]) {
  const all = (): Element[] => [
    ...Object.values(ids).flatMap(descendants),
    ...tabs,
  ];
  return {
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
}

function recordingFetch(
  responses: { config: Record<string, unknown>; agents: unknown[] },
  requests: Request[],
  onRequest?: (payload: Request) => Promise<void>
) {
  return async (url: string, options: { body: string }) => {
    const payload: Request = JSON.parse(options.body);
    if (url === '/api/session')
      return { ok: true, json: async () => ({ token: 'session' }) };
    requests.push(payload);
    await onRequest?.(payload);
    const result =
      payload.operation === 'inspect'
        ? responses.config
        : payload.operation === 'agents'
          ? { agents: responses.agents }
          : { ok: true };
    return { ok: true, json: async () => result };
  };
}

export async function browser(
  config: Record<string, unknown>,
  agents: unknown[] = [],
  onRequest?: (payload: Request) => Promise<void>
) {
  const ids = pageElements();
  const tabs = ['settings', 'keys', 'agents'].map(tab => {
    const button = new Element('button');
    button.dataset.tab = tab;
    return button;
  });
  const requests: Request[] = [];
  let fragmentRemoved = false;
  const AsyncFunction: AppFunction = Object.getPrototypeOf(
    async function () {}
  ).constructor;
  const run = new AsyncFunction(
    'document',
    'window',
    'history',
    'fetch',
    appJs
  );
  await run(
    fakeDocument(ids, tabs),
    { location: { hash: '#bootstrap' } },
    {
      replaceState() {
        fragmentRemoved = true;
      },
    },
    recordingFetch({ config, agents }, requests, onRequest)
  );
  assert.equal(fragmentRemoved, true);
  return {
    ids,
    requests,
    tab: async (name: string) =>
      tabs.find(tab => tab.dataset.tab === name)!.fire(),
    controls: () => descendants(ids.content),
    click: async (name: string) => {
      const button = descendants(ids.content).find(
        element => element.tagName === 'BUTTON' && element.textContent === name
      );
      assert.ok(button, `Missing button ${name}`);
      await button.fire();
      await new Promise(resolve => setImmediate(resolve));
    },
  };
}
