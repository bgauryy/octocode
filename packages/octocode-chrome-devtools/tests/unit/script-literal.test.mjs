// A value spliced into a page expression evaluates to itself and cannot end the code around it.
import test from 'node:test';
import assert from 'node:assert/strict';
import { runInNewContext, runInThisContext } from 'node:vm';
import { scriptLiteral } from '../../dist/engine/cdp-checks/script-literal.mjs';
import { run as screenshot } from '../../dist/engine/cdp-checks/page-screenshot.mjs';
import { fixture } from './plan-fixture.mjs';

const values = [
  'a[title="</script><!--x-->"]',
  "p:not([title*='`${x}'])",
  '#a\\:b > .c',
  'line\u2028separators\u2029',
  'control \b\f\n\r\t\0 é✓ 😀',
  '',
  0,
  -1.5,
  true,
  null,
  ['</SCRIPT>', { nested: '<b>' }],
  { '<key>': '>value<', list: [1, '\u2028'] },
];

test('scriptLiteral evaluates to the JSON value, with no <, > or line separator left raw', () => {
  for (const value of values) {
    const literal = scriptLiteral(value);
    assert.doesNotMatch(literal, /[<>\u2028\u2029]/, literal);
    const evaluated = runInThisContext(`(${literal})`);
    assert.deepEqual(evaluated, JSON.parse(JSON.stringify(value)));
    assert.deepEqual(evaluated, value);
  }
});

test('page-screenshot hands SHOT_SELECTOR to querySelector unchanged', async () => {
  for (const selector of values.filter(
    value => typeof value === 'string' && value
  ))
    await fixture(async ({ cdp }) => {
      const seen = [];
      const element = {
        scrollIntoView() {},
        getBoundingClientRect: () => ({ x: 1, y: 2, width: 3, height: 4 }),
      };
      const page = {
        document: {
          readyState: 'complete',
          title: 'shot',
          querySelector: text => (seen.push(text), element),
        },
        scrollX: 0,
        scrollY: 0,
      };
      cdp.send = async (method, params) =>
        method === 'Runtime.evaluate'
          ? { result: { value: runInNewContext(params.expression, page) } }
          : method === 'Page.captureScreenshot'
            ? { data: Buffer.from('jpeg').toString('base64') }
            : {};
      await screenshot(cdp, { SHOT_SELECTOR: selector });
      assert.deepEqual(seen, [selector]);
    });
});
