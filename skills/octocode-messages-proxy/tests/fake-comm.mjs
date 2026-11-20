import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

/**
 * Installs a stateful fake octocode-agents-communication CLI in dir. `run` prints a ready event and stays alive as
 * the proxy; `send_message` queues the proxy's answer "PONG: <inner text>" in the operator inbox. Modes:
 * schema 'body' (an old package), proxy 'error' (a vendor error before ready), reply 'none' (the worker never answers).
 * Returns a reader for the recorded state: calls, joins, leaves, sent messages, completions and the proxy pid.
 */
export function fakeComm(dir, { peers = [], schema = 'parts', proxy = 'ready', reply = 'answer' } = {}) {
  const state = path.join(dir, 'comm-state.json');
  writeFileSync(state, JSON.stringify({ calls: [], sent: [], completed: [], left: [], inbox: [] }));
  const options = JSON.stringify({ peers, schema, proxy, reply, state });
  writeFileSync(path.join(dir, 'octocode-agents-communication'), `#!/usr/bin/env python3
import json, os, sys, time
options = json.loads(${JSON.stringify(options)})
argv = sys.argv[2:]
command = argv[0]
payload = json.loads(sys.stdin.read() or 'null') if '-' in argv else None
session = argv[argv.index('--session') + 1] if '--session' in argv else None
with open(options['state']) as f:
    state = json.load(f)
state['calls'].append([command, session])
def save():
    with open(options['state'], 'w') as f:
        json.dump(state, f)
def out(value):
    save()
    print(json.dumps(value))
if command == 'schema':
    out({'inputSchema': {'required': [options['schema'], 'reasoning']}})
elif command == 'join':
    state['joined'] = payload
    out({'id': 'operator-1', 'name': payload['name'], 'vendor': payload['vendor']})
elif command == 'binding':
    out({'active': session == 'mine', 'id': session, 'vendor': 'octocode'})
elif command == 'heartbeat':
    out({'alive': True})
elif command == 'peers':
    out({'items': options['peers']})
elif command == 'run':
    state['proxyPid'] = os.getpid()
    state['run'] = argv
    save()
    if options['proxy'] == 'error':
        print("Vendor error: {'codexErrorInfo': 'usageLimitExceeded'}", flush=True)
        sys.exit(1)
    print(json.dumps({'type': 'ready', 'session': 'proxy-1', 'vendor': 'claude'}), flush=True)
    time.sleep(120)
elif command == 'send_message':
    state['sent'].append(payload)
    inner = payload['parts'][0]['data']
    if options['reply'] == 'answer':
        state['inbox'].append({'id': 7, 'replyTo': 1, 'replyRequired': False, 'outcome': 'completed',
                               'parts': [{'text': 'PONG: ' + inner['parts'][0]['text']}]})
    out({'id': 1})
elif command in ('inbox', 'inbox-wait'):
    out({'items': [item for item in state['inbox'] if item['id'] not in state['completed']]})
elif command == 'complete':
    state['completed'].append(payload['message'])
    out({'completed': True, 'count': 1})
elif command == 'leave':
    state['left'].append(session)
    out({'left': True})
else:
    sys.exit('unexpected command ' + command)
`, { mode: 0o700 });
  return () => JSON.parse(readFileSync(state, 'utf8'));
}
