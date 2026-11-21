import test from 'node:test';
import assert from 'node:assert/strict';
import { measure } from '../../tools/measure-surface.mjs';
import { fixture, data } from './agent-fixture.mjs';
import { chromeFlagDenylist } from '../../dist/engine/chrome-contract.mjs';

// Invariants of the schema token cost clients load on connect (see docs/mcp-cli.md, Token budget).
const [slim, research, full] = await Promise.all(
  ['slim', 'research', 'full'].map(measure)
);
const size = (surface, name) =>
  surface.rows.find(row => row.name === name).schemaChars;

test('presets cost less the smaller they are', () => {
  assert.ok(slim.chars < String(research.chars));
  assert.ok(research.chars < full.chars);
});

test('the default surface stays at most 78% of the full surface (12 tools incl. cdp, protocol and group)', () => {
  assert.ok(research.chars <= full.chars * 0.78);
});

test('the slim surface stays at most 52% of the default surface', () => {
  assert.ok(slim.chars <= research.chars * 0.52);
});

test('flow does not embed a copy of the plan schema', () => {
  // run advertises a compact plan (ops and options only), so flow, which takes a plan record, never outweighs it.
  assert.ok(size(research, 'flow') <= size(research, 'run'));
});

test('the shared connection object is one compact copy, identical in every tool', () => {
  const sizes = full.rows
    // targets and protocol advertise the port alone, so their connection is smaller by design.
    .filter(
      row =>
        row.connectionChars > 2 &&
        !'group targets protocol'.split(' ').includes(row.name)
    )
    .map(row => row.connectionChars);
  assert.ok(sizes.length >= 4, 'tools that take a connection');
  assert.equal(new Set(sizes).size, 1);
  // About 211 tokens when measured; it must stay a small fraction of the surface.
  assert.ok(sizes[0] <= full.chars * 0.04, String(sizes[0]));
  // The compact plan shrank run, so the repeated connection is a larger share of the whole.
  assert.ok(sizes[0] * sizes.length <= full.chars * 0.45);
});

test('the default surface stays under a documented ceiling', () => {
  // Measured 2026-11: about 13.3k chars (3.3k tokens at chars/4) for the default surface (12 tools; no MCP preset exports snapshot or screenshot, run advertises a compact plan, schemas carry no $schema or integer bounds); ceiling is that value + 10%.
  const measuredDefaultChars = 13300;
  assert.ok(
    research.chars <= measuredDefaultChars * 1.1,
    String(research.chars)
  );
});

test('open and check advertise plain objects while the contract still validates them', () =>
  fixture(
    async ({ client }) => {
      const listed = await client.listTools();
      const props = name =>
        listed.tools.find(tool => tool.name === name).inputSchema.properties;
      for (const [tool, field] of [
        ['open', 'launch'],
        ['check', 'options'],
      ]) {
        const advertised = props(tool)[field];
        assert.equal(advertised.type, 'object', field);
        assert.equal(advertised.properties, undefined, field);
        assert.ok(advertised.description.split(/\s+/).length <= 25, field);
      }
      for (const key of [
        'enableFeatures',
        'disableFeatures',
        'blinkFeatures',
        'windowSize',
        'userAgent',
        'proxy',
        'chromePath',
        'chromeFlags',
      ])
        assert.equal(props('open')[key], undefined, key);
      // Every denylisted flag is still refused, by name, through MCP.
      for (const denied of chromeFlagDenylist) {
        const flag = denied.prefix ? `${denied.flag}x` : denied.flag;
        const refused = await client.callTool({
          name: 'open',
          arguments: { launch: { chromeFlags: [flag] } },
        });
        assert.equal(refused.isError, true, flag);
        assert.match(JSON.stringify(refused), new RegExp(`${flag} is refused`));
        assert.ok(JSON.stringify(refused).includes(denied.reason), flag);
      }
      const attach = await client.callTool({
        name: 'open',
        arguments: {
          attach: true,
          acknowledge: true,
          port: 9,
          launch: { windowSize: '800x600' },
        },
      });
      assert.equal(attach.isError, true);
      assert.match(
        JSON.stringify(attach),
        /attach cannot combine with windowSize/
      );
      // A recipe option is still validated per recipe, with the same messages.
      const missing = await client.callTool({
        name: 'check',
        arguments: { recipe: 'source-map', options: {} },
      });
      assert.equal(missing.isError, true);
      assert.match(
        JSON.stringify(missing),
        /source-map requires SOURCEMAP_SCRIPT/
      );
      const foreign = await client.callTool({
        name: 'check',
        arguments: { recipe: 'page-pdf', options: { SCRIPTS_MATCH: 'x' } },
      });
      assert.match(
        JSON.stringify(foreign),
        /page-pdf does not read SCRIPTS_MATCH/
      );
      // The schema tool serves the fields the advertised objects leave out.
      const open = data(
        await client.callTool({
          name: 'schema',
          arguments: { command: 'open' },
        })
      );
      assert.ok(open.data.launch.fields.chromeFlags.type);
      const recipe = data(
        await client.callTool({
          name: 'schema',
          arguments: { command: 'check', recipe: 'source-map' },
        })
      );
      const guide = recipe.data.checks['source-map'];
      assert.deepEqual(guide.required, ['SOURCEMAP_SCRIPT']);
      assert.ok(guide.fields.SOURCEMAP_SCRIPT.type);
      assert.ok(guide.example.options.SOURCEMAP_SCRIPT);
    },
    { full: true }
  ));
