import test from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { client } from '../scripts/lib/report/client.mjs';
import { diagramView } from '../scripts/lib/report/client-diagrams.mjs';
import { mdKit } from '../scripts/lib/report/markdown.mjs';

const noop = () => {};
const names = ['__proto__', 'constructor', 'toString', 'hasOwnProperty'];

function element() {
  return {
    dataset: {}, classList: { add: noop, remove: noop, toggle: noop },
    addEventListener: noop, querySelectorAll: () => [], querySelector: () => element(),
  };
}

function showImages(images, sources) {
  const controls = new Map();
  const select = selector => {
    if (!controls.has(selector)) controls.set(selector, element());
    return controls.get(selector);
  };
  const imageElements = sources.map(src => ({ ...element(), src, getAttribute: () => src }));
  select('#page').querySelectorAll = selector => selector === 'img' ? imageElements : [];
  select('#report-data').textContent = JSON.stringify({ files: [{ path: 'report.md', body: '# Report', meta: {}, images }] });
  let context;
  const globals = {
    document: { ...element(), querySelector: select, body: element(), documentElement: element() },
    window: {}, location: { hash: '' }, matchMedia: () => ({ matches: false }),
    addEventListener: noop, scrollTo: noop,
  };
  runInNewContext(`(${client.toString()})`, globals)(mdKit(), '', '', {
    diagramView: () => ({ renderDiagrams: noop, modal: element() }),
    documentView: ctx => { context = ctx; return { decorate: noop }; },
    widgetView: () => ({ disposeWidgets: noop, renderWidgets: noop }),
    searchView: () => ({ q: element(), results: element() }),
  });
  return { images: imageElements, show: context.show };
}

test('missing image names retain their source instead of resolving inherited map entries', async () => {
  const data = 'data:image/png;base64,eA==';
  const result = showImages({ 'real.png': data }, [...names, 'real.png']);
  await result.show();
  assert.deepEqual(result.images.map(image => image.src), [...names, data]);
});

test('owned image entries are accepted even with prototype names', async () => {
  const data = names.map((_, index) => `data:image/png;base64,${index}`);
  const result = showImages(Object.fromEntries(names.map((name, index) => [name, data[index]])), names);
  await result.show();
  assert.deepEqual(result.images.map(image => image.src), data);
});

test('unrecognized Mermaid kinds get text labels without inheriting object properties', () => {
  const view = runInNewContext(`(${diagramView.toString()})`)({ kit: mdKit(), $: element });
  for (const name of names) assert.equal(view.diagramLabel(`${name}\nbody`), name.toLowerCase());
  assert.equal(view.diagramLabel('flowchart TD\nA --> B'), 'Flowchart');
  assert.equal(view.diagramLabel('new-diagram-beta\nbody'), 'new diagram');
});
