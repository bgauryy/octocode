import assert from 'node:assert/strict';
import { once } from 'node:events';
import net from 'node:net';
import test from 'node:test';
import { urlPolicyChecks } from './classification-network.mjs';
import { tokenFor } from './surfaces.mjs';

test('a policy refusal cannot hide an attempted connection or provider work', async () => {
  for (const effect of ['none', 'connection', 'http', 'inference']) {
    const provider = { sent: [] }, mock = { requestsFrom: [] };
    let called = false;
    const checks = await urlPolicyChecks({
      mock, q: query => ({ queries: [query] }),
      call: async input => {
        if (!called) {
          called = true;
          if (effect === 'connection') {
            const url = new URL(input.queries[0].source.url);
            const socket = net.connect({ host: url.hostname, port: Number(url.port) });
            await once(socket, 'close');
          }
          if (effect === 'http') mock.requestsFrom.push({ request: 'GET /unexpected', token: tokenFor('clasify', 'cli') });
          if (effect === 'inference') provider.sent.push({ request: {} });
        }
        return { json: { error: 'Refused.', errorCode: 'securityValidationFailed' } };
      },
    }, 'cli', provider);
    assert.equal(checks.find(([label]) => label === 'urlPolicy.source.loopback')?.[1], effect === 'none', effect);
  }
});
