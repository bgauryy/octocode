// Event trust, waits, ambiguous finds and snapshot text scoping.

import {
  live,
  check,
  success,
  open,
  snapshot,
  act,
  artifact,
  savedPageText,
  expect,
  refute,
} from './harness.mjs';

export const cases = [];
const test = (name, fn) => cases.push([name, fn]);

test('events: trusted typing and synthetic fallback are distinguished', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  for (const mode of ['trusted', 'js']) {
    const result = act({
      DOM_ROLE: 'textbox',
      DOM_NAME: 'Shadow search',
      DOM_ACTION: 'type',
      DOM_VALUE: 'event',
      DOM_INPUT: mode,
      DOM_TRACE_EVENTS: '1',
    });
    const data = artifact(result, 'DOM_CHECK');
    const input = data.after.events.filter(e => e.type === 'input');
    if (!input.length || input.some(e => e.trusted !== (mode === 'trusted')))
      throw new Error(`wrong event trust for ${mode}`);
    if (
      mode === 'trusted' &&
      !data.after.events.some(e => e.type === 'keydown')
    )
      throw new Error('trusted key events missing');
  }
});

test('wait: late selector and exact role/name after network content', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  act({ DOM_ROLE: 'button', DOM_NAME: 'Load content', DOM_ACTION: 'click' });
  const waited = act({
    DOM_ACTION: 'wait',
    DOM_SELECTOR: '#dynamic-search',
    DOM_WAIT_MS: '4000',
  });
  expect(waited, /\[WAIT\].*found/);
  const typed = act({
    DOM_ROLE: 'textbox',
    DOM_NAME: 'Search collection',
    DOM_ACTION: 'type',
    DOM_VALUE: 'smart search',
  });
  expect(typed, /\[VERIFY\].*ok/);
  const snap = snapshot();
  expect(snap, /textbox "Search collection"/);
});
test('wait: disabled control becomes enabled with progress', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  const result = act({
    DOM_SELECTOR: '#ready',
    DOM_ACTION: 'click',
    DOM_WAIT_MS: '4000',
    DOM_WAIT_TEXT: 'Progress complete',
  });
  expect(result, /\[PROGRESS\].*disabled=true/);
  expect(result, /\[WAIT\].*Progress complete/);
});
test('wait: unmet post-action text fails', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  const result = check('dom-operations-check.mjs', {
    DOM_SELECTOR: '#load',
    DOM_ACTION: 'click',
    DOM_WAIT_TEXT: 'will never appear',
    DOM_WAIT_MS: '600',
  });
  if (result.code === 0)
    throw new Error('post-action timeout reported success');
  expect(result.out, /WAIT_TIMEOUT/);
});
test('find: ambiguous role/name does not choose a control', () => {
  open('page-snapshot.mjs', 'actions.html');
  const result = check('dom-operations-check.mjs', {
    DOM_ROLE: 'link',
    DOM_NAME: 'Repeated nav label',
    DOM_ACTION: 'click',
  });
  if (result.code === 0) throw new Error('ambiguous target was clicked');
  expect(result.out, /2 elements match role\/name/);
});
test('snapshot: waits for delayed requested content', () => {
  open('page-snapshot.mjs', 'async-flow.html');
  act({ DOM_SELECTOR: '#load', DOM_ACTION: 'click', DOM_SETTLE_MS: '0' });
  const result = success(
    check('page-snapshot.mjs', {
      SNAPSHOT_WAIT_SELECTOR: '#dynamic-search',
      SNAPSHOT_WAIT_TEXT: 'Content loaded',
      SNAPSHOT_WAIT_MS: '4000',
    })
  ).out;
  expect(result, /textbox "Search collection"/);
  refute(result, /PAGE_NOT_FULLY_LOADED/);
});
test('snapshot text: retains every article and surrounding content', () => {
  const out = open('page-snapshot.mjs', 'text-scope.html', {
    SNAPSHOT_TEXT: '12',
  }).out;
  const text = savedPageText(out);
  for (const expected of [
    'Evidence before articles',
    'First article evidence',
    'Second article evidence',
    'Evidence after articles',
  ]) {
    if (!text.includes(expected)) throw new Error(`Missing ${expected}`);
  }
  refute(text, /Hidden evidence/);
});
test('snapshot text: main does not hide surrounding document evidence', () => {
  const text = savedPageText(
    open('page-snapshot.mjs', 'text-scope-main.html', {
      SNAPSHOT_TEXT: '12',
    }).out
  );
  for (const expected of [
    'Evidence outside main',
    'Evidence inside main',
    'Evidence after main',
  ]) {
    if (!text.includes(expected)) throw new Error(`Missing ${expected}`);
  }
});
test('snapshot text: CSS and ref roots bound the saved text', () => {
  for (const kind of ['css', 'ref', 'region']) {
    const initial = open('page-snapshot.mjs', 'text-scope.html').out;
    const saved = artifact(initial, 'PAGE_SNAPSHOT');
    const root =
      kind === 'css'
        ? '#second'
        : Object.entries(kind === 'ref' ? saved.refs : saved.regions).find(
            ([, row]) =>
              row.name === (kind === 'ref' ? 'Second article' : 'Second region')
          )?.[0];
    if (!root) throw new Error(`Missing ${kind} root`);
    const text = savedPageText(
      success(
        check('page-snapshot.mjs', {
          SNAPSHOT_TEXT: '12',
          SNAPSHOT_ROOT: root,
        })
      ).out
    );
    if (!text.includes('Second article'))
      throw new Error('Scoped text missing');
    refute(text, /First article|Evidence before|Evidence after/);
    if (kind !== 'ref' && !text.includes('Second article evidence'))
      throw new Error('Scoped article body missing');
  }
});
test('snapshot text: extraction error is explicit, not an empty artifact', () => {
  const result = check('page-snapshot.mjs', { SNAPSHOT_TEXT: '12' }, [
    '--new-tab',
    `${live.BASE}/text-scope-error.html`,
  ]);
  if (result.code === 0) throw new Error('Text extraction failure was hidden');
  expect(result.out, /PAGE_TEXT_UNAVAILABLE/);
  refute(result.out, /\[ARTIFACT\] PAGE_TEXT /);
});
