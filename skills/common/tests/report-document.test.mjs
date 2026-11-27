import test from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { documentView } from '../scripts/lib/report/client-document.mjs';

function element(tag, textContent = '', attributes = {}) {
  const classes = new Set();
  return {
    tagName: tag.toUpperCase(), textContent, attributes: { ...attributes }, dataset: {}, children: [],
    classList: { add: name => classes.add(name), contains: name => classes.has(name) },
    getAttribute(name) { return this.attributes[name] ?? null; },
    setAttribute(name, value) { this.attributes[name] = value; },
    removeAttribute(name) { delete this.attributes[name]; },
    closest() { return this.enclosedBy ?? null; },
    replaceWith(replacement) { this.replacement = replacement; },
    append(child) { this.children.push(child); },
    replaceChildren() { this.children = []; },
  };
}

/** Exercise the same serialized function the standalone HTML installs. */
function decorate({ refs = {}, codes = [], anchors = [], headings = [] } = {}) {
  const controls = new Map(['#toc', '#mobile-toc', '#sources', '#tab-sources', '#library-summary'].map(id => [id, element('div')]));
  const editorCalls = [];
  const document = { createElement: tag => element(tag) };
  const view = runInNewContext(`(${documentView.toString()})`, { document, URL })({
    kit: { toneOf: () => null },
    $: selector => controls.get(selector) ?? null,
    $$: selector => selector === 'a[href]' ? anchors : selector === 'code' ? codes : selector === 'h1,h2,h3,h4' ? headings : [],
    esc: String, slugify: value => value.toLowerCase(), decode: decodeURIComponent,
    href: (file, fragment) => `#/${file}${fragment ? `#${fragment}` : ''}`,
    resolvePath: (_file, target) => target,
    editorUrl: (abs, line) => {
      assert.equal(typeof abs, 'string', 'only a resolved reference can reach editor URL construction');
      editorCalls.push({ abs, line });
      return `editor:${abs}${line ? `:${line}` : ''}`;
    },
    byPath: new Map(),
  });
  view.decorate(element('article'), { path: 'report.md', refs });
  return { editorCalls, sources: view.sources(), controls };
}

const inheritedNames = ['__proto__', 'constructor', 'toString', 'hasOwnProperty'];
const realRef = { abs: '/workspace/src/file.js', rel: 'src/file.js', line: 7 };

test('inline code ignores inherited map entries and still decorates real sources', () => {
  const refs = Object.assign(Object.create({ inherited: realRef }), { 'src/file.js:7': realRef });
  const plain = [...inheritedNames, 'inherited'].map(name => element('code', name));
  const actual = element('code', 'src/file.js:7');
  const result = decorate({ refs, codes: [...plain, actual] });
  for (const code of plain) assert.equal(code.replacement, undefined, code.textContent);
  assert.equal(actual.replacement.href, 'editor:/workspace/src/file.js:7');
  assert.deepEqual(result.editorCalls, [{ abs: realRef.abs, line: 7 }]);
  assert.equal(result.sources.length, 1);
  assert.equal(result.controls.get('#tab-sources').textContent, 'Sources (1)');
});

test('explicit links to inherited keys are missing rather than workspace references', () => {
  const missing = inheritedNames.map(name => element('a', name, { href: name }));
  const actual = element('a', 'Source', { href: 'src/file.js:7' });
  const result = decorate({ refs: { 'src/file.js:7': realRef }, anchors: [...missing, actual] });
  for (const anchor of missing) {
    assert.equal(anchor.getAttribute('href'), null, anchor.textContent);
    assert.equal(anchor.getAttribute('aria-disabled'), 'true', anchor.textContent);
    assert.ok(anchor.classList.contains('missing'), anchor.textContent);
  }
  assert.equal(actual.href, 'editor:/workspace/src/file.js:7');
  assert.equal(result.sources.length, 1);
});

test('owned keys are resolved even when their names occur on Object.prototype', () => {
  const refs = Object.fromEntries(inheritedNames.map(name => [name, { abs: `/workspace/${name}`, rel: name }]));
  const codes = inheritedNames.map(name => element('code', name));
  const anchors = inheritedNames.map(name => element('a', name, { href: name }));
  const result = decorate({ refs, codes, anchors });
  for (const [index, name] of inheritedNames.entries()) {
    assert.equal(codes[index].replacement.href, `editor:/workspace/${name}`);
    assert.equal(anchors[index].href, `editor:/workspace/${name}`);
  }
  assert.equal(result.editorCalls.length, 2 * inheritedNames.length);
  assert.equal(result.sources.length, inheritedNames.length);
  assert.equal(Object.getPrototypeOf(refs), Object.prototype);
});

test('code already inside preformatted blocks or links stays untouched', () => {
  const codes = ['pre', 'a'].map(tag => Object.assign(element('code', 'src/file.js:7'), { enclosedBy: element(tag) }));
  const result = decorate({ refs: { 'src/file.js:7': realRef }, codes });
  assert.equal(result.editorCalls.length, 0);
  assert.ok(codes.every(code => code.replacement === undefined));
});

test('prototype names receive ordinary unique heading IDs and matching navigation', () => {
  const headings = inheritedNames.flatMap(name => [element('h2', name), element('h3', name)]);
  const result = decorate({ headings });
  for (const [index, name] of inheritedNames.entries()) {
    for (let duplicate = 0; duplicate < 2; duplicate++) {
      const id = name.toLowerCase() + (duplicate ? '-1' : '');
      const heading = headings[2 * index + duplicate];
      assert.equal(heading.id, id);
      assert.equal(heading.children[0].href, `#/report.md#${id}`);
      assert.ok(result.controls.get('#toc').innerHTML.includes(`data-id="${id}"`));
    }
  }
});
