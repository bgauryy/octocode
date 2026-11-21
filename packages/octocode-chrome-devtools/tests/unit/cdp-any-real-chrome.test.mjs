// Any CDP method through the MCP `cdp` and `run` tools on a real isolated headless Chrome: scope
// routing, dynamic enable, the schema invariants over every command of the installed protocol, one
// harmless method per domain, and the wrapped-call consent. Skipped without Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createConnection } from 'node:net';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { skip, withSite } from './tool-depth-fixture.mjs';
import { freePort } from '../fixtures/free-port.mjs';
import { classify } from '../../dist/engine/cdp-policy.mjs';
import { loadSchema, validateParams } from '../../dist/engine/cdp-schema.mjs';
import {
  commandsOf,
  harmlessPerDomain,
  kindOf,
  paramsFor,
  wrongValueFor,
} from '../fixtures/cdp-params.mjs';

const listening = port =>
  new Promise(resolve => {
    const socket = createConnection({ port, host: '127.0.0.1' });
    socket.once('connect', () => (socket.destroy(), resolve(true)));
    socket.once('error', () => resolve(false));
  });

/** Package refusals and transport failures are failures; Chrome's own errors are acceptable answers. */
const PACKAGE_OR_TRANSPORT =
  /\[POLICY\]|\[INVALID\]|\[ENABLE\]|\[CDP_CLI\]|\[SANDBOX\]|WebSocket|CDP timeout|Session closed|No Chrome|ECONN/;

test(
  'any CDP method runs through the MCP: routing, enable, invariants, every domain, wrapped consent',
  { skip, timeout: 290000 },
  async t => {
    await withSite(async ({ cwd, base, port, connection, call, ok, read }) => {
      const step = reply => reply.flow.steps[0];
      /** The step's result: inline data, or the artifact the reply points to. */
      const result = row =>
        row.data?.next?.query?.file
          ? JSON.parse(read(row.data.next.query.file))
          : row.data;
      const cacheDir = mkdtempSync(join(tmpdir(), 'octo-any-schema-'));
      const schema = await loadSchema(port, { cacheDir });

      await t.test(
        'schema invariants: generated params are never refused and a wrong type always is',
        () => {
          const commands = commandsOf(schema);
          assert.ok(
            commands.length > 100,
            'the live protocol has its commands'
          );
          let withRequired = 0;
          for (const entry of commands) {
            const params = paramsFor(schema, entry);
            const valid = validateParams(schema, entry.method, params);
            assert.equal(
              valid.ok,
              true,
              `${entry.method} false refusal: ${valid.errors?.join('; ')}`
            );
            const required = (entry.command.parameters ?? []).filter(
              field => !field.optional
            );
            if (!required.length) continue;
            withRequired++;
            const [field] = required;
            const wrong = wrongValueFor(kindOf(schema, entry.domain, field));
            const mutated = { ...params };
            // A parameter of type any takes every value: dropping it is the clear error there.
            if (wrong === undefined) delete mutated[field.name];
            else mutated[field.name] = wrong;
            assert.equal(
              validateParams(schema, entry.method, mutated).ok,
              false,
              `${entry.method}.${field.name} mutation accepted`
            );
          }
          assert.ok(withRequired > 50);
        }
      );

      await t.test(
        'protocol * lists every domain, paged with executable continuations',
        async () => {
          const first = await ok('protocol', { member: '*', connection });
          const names = first.data.domains.map(row => row.name);
          assert.equal(first.data.total, schema.domains.length);
          let next = first.data.next;
          assert.ok(next, 'a first page of 25 leaves a continuation');
          while (next) {
            const { command, args } = next.continue;
            const page = JSON.parse(
              spawnSync(command, args, { encoding: 'utf8' }).stdout
            );
            names.push(...page.domains.map(row => row.name));
            next = page.next;
          }
          assert.deepEqual(
            names,
            schema.domains.map(domain => domain.domain)
          );
          const row = first.data.domains.find(entry => entry.name === 'DOM');
          assert.equal(row.needsEnable, true);
          assert.deepEqual(Object.keys(row).sort(), [
            'commands',
            'dependencies',
            'events',
            'experimental',
            'name',
            'needsEnable',
          ]);
          const css = (await ok('protocol', { member: 'CSS', connection }))
            .data;
          assert.equal(css.enable.available, true);
          assert.equal(css.enable.bare, true);
          assert.deepEqual(css.enable.dependencies, ['DOM', 'Page']);
          assert.ok(css.startsEmitting.includes('styleSheetAdded'));
          assert.equal(
            css.docsUrl,
            'https://chromedevtools.github.io/devtools-protocol/tot/CSS/'
          );
          assert.equal(
            css.docsHash,
            'https://chromedevtools.github.io/devtools-protocol/#/CSS'
          );
        }
      );

      await t.test(
        'a browser-only method routes to the browser connection on scope auto',
        async () => {
          const info = await ok('cdp', {
            method: 'SystemInfo.getInfo',
            connection,
          });
          assert.equal(step(info).scope, 'browser');
          assert.ok(
            Object.keys(result(step(info))).length > 0,
            'SystemInfo answered'
          );
        }
      );

      await t.test(
        'browser contexts are created, listed and disposed and the count returns to zero',
        async () => {
          const count = async () =>
            result(
              step(
                await ok('cdp', {
                  method: 'Target.getBrowserContexts',
                  connection,
                })
              )
            ).browserContextIds;
          assert.deepEqual(await count(), []);
          const made = await ok('cdp', {
            method: 'Target.createBrowserContext',
            connection,
          });
          assert.equal(step(made).scope, 'browser');
          const { browserContextId } = result(step(made));
          assert.equal(typeof browserContextId, 'string');
          assert.deepEqual(await count(), [browserContextId]);
          const disposed = await ok('cdp', {
            method: 'Target.disposeBrowserContext',
            params: { browserContextId },
            acknowledge: true,
            connection,
          });
          assert.equal(step(disposed).scope, 'browser');
          assert.deepEqual(await count(), []);
        }
      );

      await t.test(
        "scope page shows Chrome's error, scope browser is forced",
        async () => {
          const page = await call('cdp', {
            method: 'SystemInfo.getInfo',
            scope: 'page',
            connection,
          });
          assert.match(
            page.refused,
            /SystemInfo\.getInfo is only supported on the browser target/
          );
          assert.doesNotMatch(page.refused, /\[UNSUPPORTED\]/);
          const forced = await ok('cdp', {
            method: 'Browser.getVersion',
            scope: 'browser',
            connection,
          });
          assert.equal(step(forced).scope, 'browser');
          const auto = await ok('cdp', {
            method: 'Browser.getVersion',
            connection,
          });
          assert.equal(
            step(auto).scope,
            undefined,
            'a method the page serves stays on the page'
          );
        }
      );

      await t.test(
        'a method no scope serves reports that this Chrome does not support it',
        async () => {
          const reply = await call('cdp', {
            method: 'SmartCardEmulation.enable',
            acknowledge: true,
            connection,
          });
          assert.match(
            reply.refused,
            /\[UNSUPPORTED\] SmartCardEmulation\.enable/
          );
          assert.match(
            reply.refused,
            /devtools-protocol\/tot\/SmartCardEmulation/
          );
        }
      );

      await t.test(
        'a plan reaches CSS through DOM with no explicit enable',
        async () => {
          await ok('cdp', {
            method: 'Page.navigate',
            params: { url: `${base}/` },
            connection,
          });
          const plan = await ok('run', {
            connection,
            plan: {
              steps: [
                { op: 'cdp', method: 'DOM.getDocument' },
                {
                  op: 'cdp',
                  method: 'DOM.querySelector',
                  params: {
                    nodeId: { $step: 1, pointer: '/root/nodeId' },
                    selector: 'h1',
                  },
                },
                {
                  op: 'cdp',
                  method: 'CSS.getComputedStyleForNode',
                  params: { nodeId: { $step: 2, pointer: '/nodeId' } },
                },
              ],
            },
          });
          assert.equal(plan.flow.completedSteps, 3);
          const css = plan.flow.steps[2];
          assert.ok(
            css.enabled.includes('CSS'),
            "CSS was enabled from Chrome's own message"
          );
          assert.ok(
            result(css).computedStyle.some(entry => entry.name === 'display'),
            'the computed style came back'
          );
        }
      );

      await t.test('explicit enable puts DOM before CSS', async () => {
        const reply = await ok('cdp', {
          method: 'DOM.getDocument',
          enable: ['CSS'],
          connection,
        });
        assert.deepEqual(step(reply).enabled, ['DOM', 'CSS']);
        const unknown = await call('cdp', {
          method: 'DOM.getDocument',
          enable: ['Dom'],
          connection,
        });
        assert.match(unknown.refused, /not a domain.*did you mean DOM/s);
      });

      await t.test(
        'a listener on Network.requestWillBeSent enables Network by itself',
        async () => {
          const reply = await ok('run', {
            connection,
            plan: {
              steps: [
                { op: 'listen', id: 'req', event: 'Network.requestWillBeSent' },
                { op: 'goto', url: `${base}/plain` },
                { op: 'waitEvent', listener: 'req' },
              ],
            },
          });
          const [events] = reply.flow.events;
          assert.equal(events.event, 'Network.requestWillBeSent');
          assert.ok(
            events.observed >= 1,
            'requests were observed without Network.enable'
          );
        }
      );

      await t.test(
        'plan-level enable runs before step 1, so no step needs an automatic enable',
        async () => {
          const reply = await ok('run', {
            connection,
            plan: {
              enable: ['CSS'],
              steps: [
                { op: 'cdp', method: 'DOM.getDocument' },
                {
                  op: 'cdp',
                  method: 'DOM.querySelector',
                  params: {
                    nodeId: { $step: 1, pointer: '/root/nodeId' },
                    selector: 'h1',
                  },
                },
                {
                  op: 'cdp',
                  method: 'CSS.getComputedStyleForNode',
                  params: { nodeId: { $step: 2, pointer: '/nodeId' } },
                },
              ],
            },
          });
          assert.equal(reply.flow.completedSteps, 3);
          const css = reply.flow.steps[2];
          assert.equal(
            css.enabled,
            undefined,
            'CSS was already enabled by the plan'
          );
          assert.ok(result(css).computedStyle.length > 0);
        }
      );

      await t.test(
        "one harmless method per domain answers or fails with Chrome's own error",
        async () => {
          const picks = harmlessPerDomain(schema, method => classify(method));
          assert.equal(
            picks.size,
            schema.domains.length,
            'a method is exercised for every live domain'
          );
          const failures = [];
          let exercised = 0;
          for (const entry of picks.values()) {
            const reply = await call('cdp', {
              method: entry.method,
              params: paramsFor(schema, entry),
              ...(entry.acknowledge ? { acknowledge: true } : {}),
              connection,
            });
            exercised++;
            if (reply.refused === undefined) continue;
            if (/\[UNSUPPORTED\]/.test(reply.refused)) continue;
            if (PACKAGE_OR_TRANSPORT.test(reply.refused))
              failures.push(`${entry.method}: ${reply.refused.slice(0, 300)}`);
          }
          assert.equal(exercised, schema.domains.length);
          assert.deepEqual(failures, []);
        }
      );

      await t.test(
        'a wrapped Browser.close is refused without acknowledge',
        async () => {
          const wrapped = (method, extra = {}) => ({
            method: 'Target.sendMessageToTarget',
            params: {
              sessionId: 'none',
              message: JSON.stringify({ id: 1, method, params: {} }),
            },
            connection,
            ...extra,
          });
          const refused = await call('cdp', wrapped('Browser.close'));
          assert.match(refused.refused, /Browser\.close .*user's permission/s);
          const malformed = await call('cdp', {
            ...wrapped('Browser.close'),
            params: { sessionId: 'none', message: 'not json' },
          });
          assert.match(malformed.refused, /not valid JSON/);
          assert.equal(
            await listening(port),
            true,
            'the browser is still there'
          );
        }
      );

      await t.test(
        'an acknowledged Browser.close closes a throwaway browser (last)',
        async () => {
          const own = await freePort();
          await ok('open', { port: own, headless: true });
          const refused = await call('cdp', {
            method: 'Browser.close',
            connection: { port: own },
          });
          assert.match(refused.refused, /user's permission/);
          assert.equal(await listening(own), true);
          await call('cdp', {
            method: 'Browser.close',
            acknowledge: true,
            connection: { port: own },
          });
          for (let tries = 0; tries < 50 && (await listening(own)); tries++)
            await new Promise(resolve => setTimeout(resolve, 100));
          assert.equal(await listening(own), false);
          await call('cleanup', { port: own });
        }
      );

      rmSync(cacheDir, { recursive: true, force: true });
      void cwd;
    });
  }
);
