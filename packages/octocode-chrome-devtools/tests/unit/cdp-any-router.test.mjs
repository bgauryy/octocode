// Scope routing and dynamic enable, judged without a browser: the routing rules run over fake
// connections that replay the answers recorded from a real Chrome (tests/fixtures/cdp-scope-sample.json).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  BrowserHub,
  CdpRouter,
  enableNeeded,
  enableOrder,
  scopeVerdict,
} from '../../dist/engine/cdp-scope.mjs';

const sample = JSON.parse(
  readFileSync(
    join(import.meta.dirname, '../fixtures/cdp-scope-sample.json'),
    'utf8'
  )
);

const protocolError = ({ code, msg }) =>
  Object.assign(new Error(`CDP error [${code}]: ${msg}`), {
    protocolError: { code, message: msg },
  });
/** A connection that replays one recorded answer (`{ ok }` or `{ code, msg }`). */
const replay = (answer, calls) => async method => {
  calls.push(method);
  if (answer.ok) return {};
  throw protocolError(answer);
};

function routerFor(row) {
  const pageCalls = [],
    browserCalls = [];
  let opened = 0;
  const hub = new BrowserHub(async () => {
    opened++;
    return { send: replay(row.browser, browserCalls), close() {} };
  });
  const router = new CdpRouter({
    page: replay(row.page, pageCalls),
    hub,
    version: async () => sample.chrome,
    // BluetoothEmulation.enable needs parameters, so it cannot be enabled without them.
    schema: {
      domains: [
        {
          domain: 'BluetoothEmulation',
          commands: [
            { name: 'enable', parameters: [{ name: 'state', type: 'string' }] },
          ],
        },
      ],
    },
  });
  return { router, pageCalls, browserCalls, opened: () => opened };
}

const settle = promise =>
  promise.then(
    value => ({ value }),
    error => ({ error })
  );

test('the recorded sample covers every class the router must route', () => {
  const classes = new Set(sample.rows.map(row => row.cls));
  for (const cls of [
    'PAGE_OK',
    'BOTH',
    'BROWSER_ONLY',
    'UNSUPPORTED',
    'NEITHER_VALID',
  ])
    assert.ok(classes.has(cls), cls);
  assert.ok(sample.rows.filter(row => row.cls === 'BROWSER_ONLY').length >= 10);
});

test('auto scope: every recorded row ends where the sweep says it works, with Chrome errors forwarded', async () => {
  for (const row of sample.rows) {
    const { router, pageCalls, browserCalls, opened } = routerFor(row);
    const { value, error } = await settle(router.route(row.name, row.params));
    const label = `${row.name} (${row.cls})`;
    const pageVerdict = row.page.ok
      ? 'ok'
      : scopeVerdict(row.name, { code: row.page.code, message: row.page.msg });
    if (pageVerdict === 'ok' || pageVerdict === 'other') {
      // Page answered: the browser connection is never opened.
      assert.equal(opened(), 0, label);
      assert.deepEqual(browserCalls, [], label);
      assert.deepEqual(pageCalls, [row.name], label);
      if (pageVerdict === 'ok') assert.equal(value.scope, 'page', label);
      else assert.equal(error.protocolError.message, row.page.msg, label);
      continue;
    }
    // Wrong scope on the page: exactly one retry on the browser connection.
    assert.equal(opened(), 1, label);
    assert.deepEqual(browserCalls, [row.name], label);
    if (row.browser.ok) {
      assert.equal(value.scope, 'browser', label);
      assert.equal(row.cls, 'BROWSER_ONLY', label);
    } else if (
      scopeVerdict(row.name, {
        code: row.browser.code,
        message: row.browser.msg,
      }) === 'notFound'
    ) {
      assert.equal(row.cls, 'UNSUPPORTED', label);
      assert.match(error.message, /\[UNSUPPORTED\]/, label);
      assert.match(
        error.message,
        /devtools-protocol\/tot\/\w+\/#method-/,
        label
      );
      assert.match(
        error.message,
        new RegExp(sample.chrome.split(' ')[0].replace('.', '\\.')),
        label
      );
    } else {
      // A Chrome failure on the browser connection (e.g. a domain that needs parameters to enable).
      assert.equal(error.protocolError.message, row.browser.msg, label);
    }
  }
});

test('every browser-only row routes to the browser connection and nothing else does', async () => {
  const browserOnly = sample.rows.filter(row => row.cls === 'BROWSER_ONLY');
  let routed = 0;
  for (const row of browserOnly) {
    const { router, browserCalls } = routerFor(row);
    const { value, error } = await settle(router.route(row.name, row.params));
    assert.equal(browserCalls[0], row.name, row.name);
    routed++;
    if (value) assert.equal(value.scope, 'browser', row.name);
    else assert.ok(error.message.includes(row.browser.msg), row.name);
  }
  assert.equal(routed, browserOnly.length);
});

test('a -32601 naming another method is a forwarded failure, not a routing signal', async () => {
  const row = sample.rows.find(r => r.name === 'Page.deleteCookie');
  assert.ok(row, 'the sample keeps the Page.deleteCookie forwarding case');
  assert.equal(
    scopeVerdict(row.name, { code: row.page.code, message: row.page.msg }),
    'other'
  );
  const { router, browserCalls } = routerFor(row);
  const { error } = await settle(router.route(row.name, row.params));
  assert.equal(error.protocolError.message, row.page.msg);
  assert.deepEqual(browserCalls, []);
});

test('scope verdicts follow the verified wrong-scope patterns only', () => {
  const v = (message, code = -32000, method = 'Target.getBrowserContexts') =>
    scopeVerdict(method, { code, message });
  assert.equal(
    v("'Target.getBrowserContexts' wasn't found", -32601),
    'notFound'
  );
  assert.equal(v("'Target.other' wasn't found", -32601), 'other');
  assert.equal(v("'Target.getBrowserContexts' wasn't found", -32000), 'other');
  assert.equal(v('Not allowed'), 'wrongTarget');
  assert.equal(v('Not allowed', -32602), 'other');
  assert.equal(v('Not allowed to do that'), 'other');
  assert.equal(
    v('Method is only supported on the Browser target'),
    'wrongTarget'
  );
  assert.equal(
    v('Method is only supported on the browser target'),
    'wrongTarget'
  );
  assert.equal(v('Invalid parameters', -32602), 'other');
  assert.equal(scopeVerdict('A.b', undefined), 'other');
});

test('scope page never retries and scope browser never tries the page', async () => {
  const row = sample.rows.find(r => r.name === 'SystemInfo.getInfo');
  const forced = routerFor(row);
  const page = await settle(forced.router.route(row.name, {}, 'page'));
  assert.equal(page.error.protocolError.message, row.page.msg);
  assert.equal(forced.opened(), 0);
  const only = routerFor(row);
  const browser = await settle(only.router.route(row.name, {}, 'browser'));
  assert.equal(browser.value.scope, 'browser');
  assert.deepEqual(only.pageCalls, []);
});

test('the browser connection opens once per run and closes with the hub', async () => {
  let opened = 0,
    closed = 0;
  const hub = new BrowserHub(async () => {
    opened++;
    return { send: async () => ({}), close: () => closed++ };
  });
  const answer = { code: -32601, msg: "'Tethering.bind' wasn't found" };
  const page = async () => {
    throw protocolError(answer);
  };
  const a = new CdpRouter({ page, hub }),
    b = new CdpRouter({ page, hub });
  await a.route('Tethering.bind', {});
  await b.route('Tethering.bind', {});
  assert.equal(opened, 1);
  hub.close();
  assert.equal(closed, 1);
});

test('Chrome wordings that ask for an enable name the domain to enable', () => {
  const schema = {
    domains: [
      'DOM',
      'CSS',
      'Overlay',
      'WebAuthn',
      'Fetch',
      'ServiceWorker',
      'BluetoothEmulation',
      'Debugger',
      'Runtime',
      'DeviceAccess',
      'WebMCP',
    ].map(domain => ({ domain })),
  };
  const needs = (method, message) =>
    enableNeeded(method, { code: -32000, message }, schema);
  assert.equal(needs('CSS.addRule', 'CSS agent was not enabled'), 'CSS');
  assert.equal(
    needs('CSS.enable', 'DOM agent needs to be enabled first.'),
    'DOM'
  );
  assert.equal(needs('Overlay.enable', 'DOM should be enabled first'), 'DOM');
  assert.equal(
    needs(
      'Overlay.highlightRect',
      'Overlay must be enabled before a tool can be shown'
    ),
    'Overlay'
  );
  assert.equal(
    needs('Fetch.failRequest', 'Fetch domain is not enabled'),
    'Fetch'
  );
  assert.equal(
    needs(
      'ServiceWorker.deliverPushMessage',
      'ServiceWorker domain not enabled'
    ),
    'ServiceWorker'
  );
  assert.equal(
    needs(
      'BluetoothEmulation.setSimulatedCentralState',
      'BluetoothEmulation not enabled'
    ),
    'BluetoothEmulation'
  );
  assert.equal(
    needs('Debugger.continueToLocation', 'Debugger agent is not enabled'),
    'Debugger'
  );
  assert.equal(
    needs('Runtime.compileScript', 'Runtime agent is not enabled'),
    'Runtime'
  );
  assert.equal(
    needs('DeviceAccess.selectPrompt', 'DeviceAccess domain is not enabled'),
    'DeviceAccess'
  );
  assert.equal(
    needs('WebMCP.invokeTool', 'WebMCP domain is not enabled'),
    'WebMCP'
  );
  assert.equal(
    needs(
      'WebAuthn.addVirtualAuthenticator',
      'The Virtual Authenticator Environment has not been enabled for this session'
    ),
    'WebAuthn'
  );
  assert.equal(
    needs('Page.getResourceContent', 'Agent is not enabled.'),
    'Page'
  );
  assert.equal(
    needs('Page.produceCompilationCache', 'Agent needs to be enabled first'),
    'Page'
  );
  // Wordings that name no domain: the method's own domain is the one to enable.
  for (const [method, message] of [
    ['Accessibility.getPartialAXTree', 'Accessibility has not been enabled'],
    ['Log.clear', 'Log is not enabled'],
    ['Profiler.start', 'Profiler is not enabled'],
    ['Overlay.hideHighlight', 'it must be enabled before use'],
    ['Page.reload', "agent hasn't been enabled"],
    ['DOM.getDocument', 'This domain needs to be enabled first'],
  ])
    assert.equal(
      enableNeeded(
        method,
        { code: -32000, message },
        {
          domains: [
            'Accessibility',
            'Log',
            'Profiler',
            'Overlay',
            'Page',
            'DOM',
          ].map(domain => ({ domain })),
        }
      ),
      method.split('.')[0],
      message
    );
  // Not a request to enable: a disable, an unknown name, a missing method, another failure.
  assert.equal(
    needs('Accessibility.disable', 'Accessibility has not been enabled'),
    undefined
  );
  assert.equal(needs('Log.clear', 'Log is broken'), undefined);
  assert.equal(
    needs('DOM.disable', "DOM agent hasn't been enabled"),
    undefined
  );
  assert.equal(needs('Foo.bar', 'Nope agent was not enabled'), undefined);
  assert.equal(
    enableNeeded(
      'CSS.addRule',
      { code: -32601, message: 'CSS agent was not enabled' },
      schema
    ),
    undefined
  );
});

/** A fake page whose domains behave like Chrome's: CSS needs DOM, methods need their own domain enabled. */
function fakeChrome() {
  const on = new Set(),
    sent = [];
  const fail = message =>
    Object.assign(new Error(message), {
      protocolError: { code: -32000, message },
    });
  const send = async method => {
    sent.push(method);
    const [domain, name] = method.split('.');
    if (name === 'enable') {
      if (domain === 'CSS' && !on.has('DOM'))
        throw fail('DOM agent needs to be enabled first.');
      on.add(domain);
      return {};
    }
    if (domain === 'CSS' && !on.has('CSS'))
      throw fail('CSS agent was not enabled');
    if (domain === 'CSS' && !on.has('DOM'))
      throw fail('DOM agent needs to be enabled first');
    return { ok: method };
  };
  const schema = {
    domains: [
      { domain: 'Runtime', commands: [{ name: 'enable' }] },
      {
        domain: 'DOM',
        dependencies: ['Runtime'],
        commands: [{ name: 'enable' }],
      },
      {
        domain: 'CSS',
        dependencies: ['DOM'],
        commands: [{ name: 'enable' }, { name: 'getStyle' }],
      },
      {
        domain: 'Page',
        dependencies: ['DOM', 'CSS'],
        commands: [{ name: 'enable' }],
      },
      { domain: 'Fetch', commands: [{ name: 'enable' }] },
      { domain: 'SystemInfo', commands: [{ name: 'getInfo' }] },
      {
        domain: 'Needy',
        commands: [
          { name: 'enable', parameters: [{ name: 'state', type: 'string' }] },
        ],
      },
    ],
  };
  const hub = new BrowserHub(async () => ({
    send: async () => ({}),
    close() {},
  }));
  return {
    router: new CdpRouter({ page: send, hub, schema }),
    on,
    sent,
    schema,
  };
}

test("a call that needs an enable gets it from Chrome's own message, once, then runs", async () => {
  const { router, sent } = fakeChrome();
  const outcome = await router.route('CSS.getStyle', {});
  assert.deepEqual(outcome.enabled, ['DOM', 'CSS']);
  assert.deepEqual(sent, [
    'CSS.getStyle',
    'CSS.enable',
    'DOM.enable',
    'CSS.enable',
    'CSS.getStyle',
  ]);
  // Enabled once per connection: the next call needs no enable at all.
  sent.length = 0;
  const again = await router.route('CSS.getStyle', {});
  assert.deepEqual(again.enabled, []);
  assert.deepEqual(sent, ['CSS.getStyle']);
});

test("an enable that does not help is retried once and then surfaces Chrome's error", async () => {
  const sent = [];
  const fail = message =>
    Object.assign(new Error(message), {
      protocolError: { code: -32000, message },
    });
  const router = new CdpRouter({
    page: async method => {
      sent.push(method);
      if (method === 'DOM.getDocument') throw fail('DOM agent was not enabled');
      return {};
    },
    hub: new BrowserHub(async () => ({ send: async () => ({}), close() {} })),
    schema: {
      domains: [
        {
          domain: 'DOM',
          commands: [{ name: 'enable' }, { name: 'getDocument' }],
        },
      ],
    },
  });
  await assert.rejects(
    router.route('DOM.getDocument', {}),
    /DOM agent was not enabled/
  );
  assert.deepEqual(sent, ['DOM.getDocument', 'DOM.enable', 'DOM.getDocument']);
});

test('explicit enable puts dependencies first, skips what cannot be enabled and refuses unknown names', async () => {
  const { router, sent, schema } = fakeChrome();
  const done = await router.enable(['CSS']);
  assert.deepEqual(done.enabled, ['DOM', 'CSS']);
  assert.deepEqual(enableOrder(schema, ['Page', 'CSS', 'DOM']), [
    'DOM',
    'CSS',
    'Page',
  ]);
  // Nothing is sent twice.
  sent.length = 0;
  assert.deepEqual((await router.enable(['DOM', 'CSS'])).enabled, []);
  assert.deepEqual(sent, []);
  const skipped = await router.enable(['SystemInfo', 'Needy', 'Page']);
  assert.deepEqual(skipped.enabled, ['Page']);
  assert.match(skipped.notes.join('\n'), /SystemInfo has no enable command/);
  assert.match(
    skipped.notes.join('\n'),
    /Needy\.enable needs parameters \(state\)/
  );
  await assert.rejects(
    router.enable(['Dom']),
    /not a domain.*did you mean DOM/
  );
  await assert.rejects(
    router.enable(['Nonsense']),
    /\[ENABLE\] Nonsense is not a domain/
  );
});

test('Fetch is never enabled on its own: it pauses traffic and needs patterns', async () => {
  const { router, sent } = fakeChrome();
  const events = await router.ensureEvents('Fetch.requestPaused');
  assert.deepEqual(events.enabled, []);
  assert.match(events.note, /Fetch\.enable pauses requests/);
  assert.deepEqual(sent, []);
  const network = await router.ensureEvents('CSS.styleSheetAdded');
  assert.deepEqual(network.enabled, ['DOM', 'CSS']);
});

test('a domain that stays disabled after the one enable ends with the in-call hint; Fetch is never enabled', async () => {
  const sent = [];
  const fail = message =>
    Object.assign(new Error(message), {
      protocolError: { code: -32000, message },
    });
  const page = async method => {
    sent.push(method);
    if (method === 'Fetch.failRequest')
      throw fail('Fetch domain is not enabled');
    if (method === 'Log.clear') throw fail('Log is not enabled');
    return {};
  };
  const schema = {
    domains: [
      { domain: 'Fetch', commands: [{ name: 'enable' }] },
      { domain: 'Log', commands: [{ name: 'enable' }] },
    ],
  };
  const router = new CdpRouter({
    schema,
    page,
    hub: new BrowserHub(async () => ({ send: page, close() {} })),
  });
  await assert.rejects(router.route('Fetch.failRequest', {}, 'page'), error => {
    assert.match(error.message, /enable:\["Fetch"\]/);
    return true;
  });
  assert.ok(!sent.includes('Fetch.enable'), 'Fetch is never auto-enabled');
  await assert.rejects(router.route('Log.clear', {}, 'page'), error => {
    assert.match(
      error.message,
      /hint: enable the domain in the same call: enable:\["Log"\] \(or run it inside a plan\)/
    );
    return true;
  });
  assert.deepEqual(
    sent.filter(m => m === 'Log.enable'),
    ['Log.enable']
  );
});
