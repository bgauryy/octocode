// Runs the native config view's browser app (core.js and app.js) against a minimal DOM and
// a recording fetch, so tests drive it through its controls and read the requests it sends.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const asset = (name: string) =>
  readFileSync(
    new URL(
      `../../../octocode-native/crates/cli/src/cli/config_view/assets/${name}`,
      import.meta.url
    ),
    'utf8'
  );
// The two ES modules as one function body: the page has no other way in.
const appJs = (asset('core.js') + asset('app.js'))
  .replace(/^export /gm, '')
  .replace(/^import .*$/gm, '');

type Listener = (event: { target: Element; type: string }) => unknown;

/** Minimal DOM stand-in: the page assigns arbitrary element properties. */
export class Element {
  [key: string]: unknown;
  tagName: string;
  parent: Element | undefined;
  children: Element[] = [];
  listeners = new Map<string, Listener[]>();
  value: unknown = '';
  textContent = '';
  dataset: Record<string, string> = {};
  attributes: Record<string, string> = {};
  classList = { toggle: () => {} };
  constructor(tag = 'div') {
    this.tagName = tag.toUpperCase();
  }
  append(...children: Element[]): void {
    for (const child of children) child.parent = this;
    this.children.push(...children);
  }
  replaceChildren(...children: Element[]): void {
    for (const child of children) child.parent = this;
    this.children = children;
  }
  setAttribute(key: string, value: string): void {
    this.attributes[key] = value;
  }
  getAttribute(key: string): string | undefined {
    return this.attributes[key];
  }
  addEventListener(event: string, listener: Listener): void {
    this.listeners.set(event, [...(this.listeners.get(event) ?? []), listener]);
  }
  /** This element, then each parent up to the root. */
  path(): Element[] {
    return [this, ...(this.parent?.path() ?? [])];
  }
  closest(selector: string): Element | undefined {
    const wanted = selector.replace('.', '');
    return this.path().find(element => element.className === wanted);
  }
  querySelectorAll(selector: string): Element[] {
    const attribute = /^\[([a-z-]+)\]$/.exec(selector)?.[1];
    const all = descendants(this);
    if (attribute === 'data-dirty') return all.filter(item => item.dataset.dirty);
    if (attribute) return all.filter(item => item.attributes[attribute] !== undefined);
    return [];
  }
  dispatchEvent(event: { type: string }): void {
    void this.fire(event.type);
  }
  /** Runs the listeners here and on each parent, as a real event bubbles. */
  async fire(event = 'click'): Promise<void> {
    for (const element of this.path()) {
      for (const listener of element.listeners.get(event) ?? []) {
        await listener({ target: this, type: event });
      }
    }
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
    summary: new Element(),
    close: new Element(),
    storage: new Element(),
    warnings: new Element(),
  };
  ids.scope.value = 'home';
  ids.close.tagName = 'BUTTON';
  return ids;
}

function fakeDocument(ids: Ids) {
  const all = (): Element[] => Object.values(ids).flatMap(descendants);
  return {
    getElementById: (id: string) => ids[id],
    createElement: (tag: string) => new Element(tag),
    createTextNode: (text: string) => {
      const element = new Element('span');
      element.textContent = text;
      return element;
    },
    addEventListener: () => {},
    querySelector: () => ids.scope,
    querySelectorAll: (selector: string) => {
      if (selector === 'input[name=scope]') return [ids.scope];
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
    fakeDocument(ids),
    { location: { hash: '#bootstrap' }, scrollY: 0, scrollTo: () => {} },
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
    controls: () => descendants(ids.content),
    labelled: (label: string) =>
      descendants(ids.content).find(element => element.attributes['aria-label'] === label),
    type: async (label: string, value: unknown, event = 'input') => {
      const input = descendants(ids.content).find(element => element.attributes['aria-label'] === label);
      assert.ok(input, `Missing input ${label}`);
      input.value = value;
      await input.fire(event);
    },
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
