import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';

const scenarios = ['double-context', 'single', 'same-promise', 'state-between', 'adjacent', 'debug-value', 'genuine-mismatch'];
const scenario = process.argv[2];
if (!scenario) {
  const results = scenarios.map(name => {
    const child = spawnSync(process.execPath, [new URL(import.meta.url).pathname, name], { encoding: 'utf8', env: { ...process.env, NODE_ENV: 'development' }, timeout: 15000 });
    if (child.status !== 0) return { scenario: name, error: child.stderr, exitCode: child.status };
    return JSON.parse(child.stdout);
  });
  const mode = process.env.JEV_REACT_CANDIDATE || 'original';
  const patched = mode !== 'original';
  const output = { kind: 'actual-react-dom-runtime', mode, patched, results };
  const resultFile = mode === '1' ? 'candidate-results.json' : mode === 'original' ? 'baseline-results.json' : `${mode}-results.json`;
  writeFileSync(new URL(`./${resultFile}`, import.meta.url), JSON.stringify(output, null, 2) + '\n');
  for (const result of results) {
    assert.equal(result.error, undefined, `${result.scenario}: runtime failed`);
    assert.equal(result.text, 'rendered 1', `${result.scenario}: rendered output`);
    const expectedWarning = result.scenario === 'genuine-mismatch' || !patched && ['double-context', 'debug-value'].includes(result.scenario);
    assert.equal(result.hookOrderWarning, expectedWarning, `${result.scenario}: warning contract`);
  }
  const double = results.find(result => result.scenario === 'double-context');
  assert.equal(double.debugHookTypes.filter(type => type === 'useContext').length, patched ? 5 : 6);
  console.log(JSON.stringify({ kind: output.kind, patched, passed: results.length, results: results.map(({ scenario, hookOrderWarning }) => ({ scenario, hookOrderWarning })) }, null, 2));
} else {
  const { JSDOM } = await import('jsdom');
  const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://localhost/' });
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  const React = await import('react');
  const variants = { '1': './patched-client.cjs', 'baseline-proposal': './baseline-proposal.cjs', 'treatment-interpretation': './treatment-interpretation.cjs' };
  const { createRoot } = await import(variants[process.env.JEV_REACT_CANDIDATE] || 'react-dom/client');
  const { createContext, useContext, useState, useEffect, useDebugValue, use, Suspense, createElement: h, act } = React;
  const messages = [];
  const originalError = console.error;
  console.error = (...args) => messages.push(args.map(String).join(' '));
  const [A, B, C, D, E] = ['a', 'b', 'c', 'd', 'e'].map(createContext);
  const first = Promise.resolve('user');
  const second = scenario === 'same-promise' ? first : Promise.resolve('company');
  let changed = false;
  function Page() {
    useContext(A);
    useContext(B);
    useContext(C);
    if (scenario === 'genuine-mismatch') {
      if (changed) useDebugValue('changed'); else useContext(D);
    } else {
      use(first);
      if (scenario === 'state-between') useState(0);
      else if (scenario === 'debug-value') useDebugValue('middle');
      else if (scenario !== 'adjacent') useContext(D);
      if (scenario !== 'single') use(second);
    }
    useContext(E);
    const [n, setN] = useState(0);
    useEffect(() => { setN(1); }, []);
    return h('div', null, `rendered ${n}`);
  }
  const root = createRoot(document.getElementById('root'));
  await act(async () => { root.render(h(Suspense, { fallback: h('div', null, 'loading') }, h(Page))); });
  if (scenario === 'genuine-mismatch') {
    changed = true;
    await act(async () => { root.render(h(Suspense, { fallback: h('div', null, 'loading') }, h(Page))); });
  }
  const findPage = fiber => !fiber ? null : fiber.type === Page ? fiber : findPage(fiber.child) || findPage(fiber.sibling);
  const types = findPage(root._internalRoot.current)?._debugHookTypes;
  const result = { scenario, reactVersion: React.version, text: document.getElementById('root').textContent, debugHookTypes: types, hookOrderWarning: messages.some(message => message.includes('change in the order of Hooks')), messages };
  await act(async () => { root.unmount(); });
  dom.window.close();
  console.error = originalError;
  console.log(JSON.stringify(result));
}
