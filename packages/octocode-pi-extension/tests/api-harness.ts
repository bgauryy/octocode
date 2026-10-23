import http from 'node:http';
import type { ContextUsage, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { OctocodeApi } from '../src/api/bridge.js';
import { ApiClient } from '../src/api/client.js';
import { spilledMessage } from '../src/team/spill.js';
import { storedEntries, stub } from './doubles.js';
import { fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

export const fakeCtx = (idle: { value: boolean }, extra: unknown[] = []) =>
  stub<ExtensionContext>({
    cwd: '/work',
    model: stub<NonNullable<ExtensionContext['model']>>({ provider: 'anthropic', id: 'sonnet' }),
    isIdle: () => idle.value,
    hasPendingMessages: () => false,
    abort: () => void (idle.value = true),
    getContextUsage: () => stub<ContextUsage>({ tokens: 10 }),
    sessionManager: stub<ExtensionContext['sessionManager']>({
      getSessionId: () => 's1',
      getSessionName: () => undefined,
      getBranch: () =>
        storedEntries([
          { type: 'message', message: { role: 'user', content: 'hi' } },
          { type: 'message', message: { role: 'assistant', content: [{ type: 'text', text: 'hello' }] } },
          ...extra,
        ]),
    }),
  });

const running: OctocodeApi[] = [];
/** Stops every API `boot` started; each suite runs it after each test. */
export const stopApis = () => Promise.all(running.splice(0).map((api) => api.stop()));

export async function boot(options: { http?: number } = {}, idle = { value: true }, branch: unknown[] = []) {
  const env = { OCTOCODE_HOME: tmp('octo-api-') } as NodeJS.ProcessEnv;
  const { pi, sent, entries, emit: fire } = fakePi();
  const emit = (name: string, event: unknown) => void fire(name, event, undefined);
  const told: Array<[string, string]> = [];
  const api = new OctocodeApi(pi, { list: () => [{ id: 'main-1' }], tell: (to, text) => (told.push([to, text]), to === 'main-1' ? { sent: [to] } : { error: `No agent "${to}".` }), deliverable: spilledMessage }, () => '1.0.0', env);
  running.push(api);
  api.observe();
  await api.start(fakeCtx(idle, branch), { socket: true, ...options });
  return { api, sent, entries, emit, told, client: new ApiClient(api.instance!), env };
}
/** One round trip through the server: everything it wrote before answering has been handled. */
export const settle = (client: ApiClient) => client.call('ping');

/** Reads GET /events as raw text until closed; `ready` resolves once the stream's headers arrived. */
export function sse(url: string, token: string, headers: Record<string, string> = {}) {
  let text = '';
  let req!: http.ClientRequest;
  const ready = new Promise<number>((resolve, reject) => {
    req = http.get(`${url}/events${headers['query'] ?? ''}`, { headers: { authorization: `Bearer ${token}`, ...(headers['last-event-id'] ? { 'last-event-id': headers['last-event-id'] } : {}) } }, (res) => {
      res.setEncoding('utf8');
      res.on('data', (chunk: string) => (text += chunk));
      resolve(res.statusCode!);
    });
    req.on('error', reject);
  });
  const ids = () => [...text.matchAll(/^id: (\d+)$/gmu)].map((match) => Number(match[1]));
  return { ready, ids, text: () => text, close: () => req.destroy() };
}
