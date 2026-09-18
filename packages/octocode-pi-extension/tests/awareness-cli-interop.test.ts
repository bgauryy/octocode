import { spawnSync } from 'node:child_process';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { openPersistentAwareness } from '../src/tools/storage-policy.js';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
import { registerBashTool } from '../src/tools/bash-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import type { PiContext, ToolDefinition } from '../src/types.js';

// Exercise the real public parser in a separate process, against the same built
// package consumed by Pi. Rebuild Awareness before running this integration test.
const cliPath = fileURLToPath(
  new URL('../../octocode-awareness/out/octocode-awareness.js', import.meta.url)
);

describe.each(['global', 'repo'] as const)(
  'Pi native Awareness adapter / external CLI interoperability (%s storage)',
  storageScope => {
    let root: string;
    let workspace: string;
    let otherWorkspace: string;
    let restoreProcesses: () => void;
    const piAgent = 'pi:interop-session';
    const cliAgent = 'external-skill-agent';

    beforeEach(() => {
      restoreProcesses = allowLocalFixtureProcesses();
      root = mkdtempSync(path.join(tmpdir(), 'pi-cli-awareness-'));
      workspace = path.join(root, 'repository');
      otherWorkspace = path.join(root, 'other-repository');
      for (const directory of [workspace, otherWorkspace]) {
        mkdirSync(directory);
        expect(spawnSync('git', ['init', '--quiet', directory]).status).toBe(0);
        mkdirSync(path.join(directory, '.octocode'));
        writeFileSync(
          path.join(directory, '.octocode', 'awareness.json'),
          JSON.stringify({
            version: 1,
            storage: { repository: storageScope, memory: storageScope },
            hooks: { profile: 'full' },
          })
        );
      }
      vi.stubEnv('OCTOCODE_HOME', path.join(root, 'home'));
      vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
      vi.stubEnv('OCTOCODE_AGENT_ID', piAgent);
      vi.stubEnv('OCTOCODE_AWARENESS_DB_PATH', undefined);
    });

    afterEach(() => {
      restoreProcesses();
      vi.unstubAllEnvs();
      rmSync(root, { recursive: true, force: true });
    });

    function cli(args: string[], cwd = workspace, expectedCode = 0): any {
      const result = spawnSync(
        process.execPath,
        [cliPath, ...args, '--compact'],
        {
          cwd,
          env: { ...process.env, OCTOCODE_AGENT_ID: cliAgent },
          encoding: 'utf8',
          timeout: 20_000,
        }
      );
      expect(
        result.error,
        `${args.join(' ')}: ${result.stderr}`
      ).toBeUndefined();
      expect(
        result.status,
        `${args.join(' ')}: ${result.stdout}\n${result.stderr}`
      ).toBe(expectedCode);
      return JSON.parse(result.stdout);
    }

    function native<T>(
      fn: (store: ReturnType<typeof openPersistentAwareness>) => T,
      cwd = workspace
    ): T {
      const store = openPersistentAwareness({ workspace: cwd });
      try {
        return fn(store);
      } finally {
        store.close();
      }
    }

    it('shares directed messages, read state and a threaded reply across processes', () => {
      const sent = native(store =>
        store.sendMessage({
          fromAgentId: piAgent,
          toAgentId: cliAgent,
          topic: 'EVIDENCE',
          text: 'The parser check passed.',
        })
      );
      const inbox = cli([
        'message',
        'list',
        '--agent-id',
        cliAgent,
        '--include-bodies',
        '--mark-read',
      ]);
      expect(inbox.signals.map((signal: any) => signal.signal_id)).toContain(
        sent.messageId
      );
      expect(
        inbox.signals.find((signal: any) => signal.signal_id === sent.messageId)
          .body
      ).toBe('The parser check passed.');
      expect(
        native(store => store.listMessages({ agentId: cliAgent }))
      ).toEqual([]);
      cli([
        'message',
        'reply',
        '--agent-id',
        cliAgent,
        '--in-reply-to',
        sent.messageId,
        '--subject',
        'Re: EVIDENCE',
        '--body',
        'I independently reproduced it.',
      ]);
      const reply = native(store => store.listMessages({ agentId: piAgent }));
      expect(reply).toHaveLength(1);
      expect(reply[0]!.text).toBe('I independently reproduced it.');
      expect(reply[0]!.toAgentId).toBe(piAgent);
      native(store =>
        store.markMessageRead({
          agentId: piAgent,
          messageId: reply[0]!.messageId,
        })
      );
      expect(cli(['message', 'list', '--agent-id', piAgent]).signals).toEqual(
        []
      );
    });

    it('routes CLI-directed and broadcast messages to the native inbox without crossing workspaces', () => {
      cli([
        'message',
        'send',
        '--agent-id',
        cliAgent,
        '--to-agent',
        piAgent,
        '--kind',
        'fyi',
        '--subject',
        'Directed',
        '--body',
        'Only the named Pi agent.',
      ]);
      cli([
        'message',
        'send',
        '--agent-id',
        cliAgent,
        '--kind',
        'fyi',
        '--subject',
        'Broadcast',
        '--body',
        'All peers in this repository.',
      ]);
      cli(
        [
          'message',
          'send',
          '--agent-id',
          cliAgent,
          '--kind',
          'fyi',
          '--subject',
          'Other repository',
        ],
        otherWorkspace
      );
      const inbox = native(store => store.listMessages({ agentId: piAgent }));
      expect(inbox.map(message => message.topic).sort()).toEqual([
        'Broadcast',
        'Directed',
      ]);
      expect(
        native(store => store.listMessages({ agentId: 'unrelated-peer' })).map(
          message => message.topic
        )
      ).toEqual(['Broadcast']);
      native(store =>
        store.markMessageRead({
          agentId: piAgent,
          messageId: inbox.find(message => message.topic === 'Broadcast')!
            .messageId,
        })
      );
      expect(
        cli(['message', 'list', '--agent-id', 'another-peer']).signals.map(
          (signal: any) => signal.subject
        )
      ).toEqual(['Broadcast']);
      expect(
        native(
          store => store.listMessages({ agentId: piAgent }),
          otherWorkspace
        ).map(message => message.topic)
      ).toEqual(['Other repository']);
    });

    it(
      'executes CLI continuations and covers every native message once while marking reads',
      { timeout: 30_000 },
      () => {
        const ids = native(store =>
          Array.from(
            { length: 13 },
            (_, index) =>
              store.sendMessage({
                fromAgentId: piAgent,
                toAgentId: cliAgent,
                topic: 'EVIDENCE',
                text: `receipt ${index}`,
              }).messageId
          )
        );
        let args = [
          'message',
          'list',
          '--agent-id',
          cliAgent,
          '--limit',
          '4',
          '--mark-read',
        ];
        const seen: string[] = [];
        for (let page = 0; page < 10; page++) {
          const result = cli(args);
          seen.push(...result.signals.map((signal: any) => signal.signal_id));
          if (!result.partial) break;
          expect(result.next.list.operation, JSON.stringify(result.next)).toBe('message.list');
          const params = result.next.list.params as Record<string, unknown>;
          args = ['message', 'list', '--agent-id', cliAgent];
          for (const [key, value] of Object.entries(params)) {
            const flag = `--${key.replaceAll('_', '-')}`;
            if (value === true) args.push(flag);
            else if (value !== false && value !== undefined) {
              for (const item of Array.isArray(value) ? value : [value]) args.push(flag, String(item));
            }
          }
        }
        expect(seen).toHaveLength(ids.length);
        expect([...seen].sort()).toEqual([...ids].sort());
        expect(
          native(store => store.countMessages({ agentId: cliAgent }))
        ).toBe(0);
      }
    );

    it('shares exclusive peer locks and makes CLI verification debt visible natively', () => {
      writeFileSync(path.join(workspace, 'shared.txt'), 'verified fixture\n');
      const locked = native(store =>
        store.acquireLock({
          filePath: 'shared.txt',
          agentId: piAgent,
          reason: 'Exclusive interop fixture',
          testPlan: 'Verify peer lock visibility',
          ttlSeconds: 60,
        })
      );
      cli(
        [
          'work',
          'protect',
          '--action',
          'acquire',
          '--agent-id',
          cliAgent,
          '--target-file',
          'shared.txt',
          '--rationale',
          'Must be blocked by Pi lock',
          '--test-plan',
          'Read fixture',
        ],
        workspace,
        2
      );
      native(store =>
        store.releaseLock({
          filePath: 'shared.txt',
          agentId: piAgent,
          runId: locked.runId,
        })
      );
      const work = cli([
        'work',
        'create',
        '--kind',
        'standalone',
        '--agent-id',
        cliAgent,
        '--file',
        'shared.txt',
        '--rationale',
        'Interop fixture assertion',
        '--test-plan',
        'Check the fixture content',
      ]);
      expect(
        native(store => store.listWork({ agentId: cliAgent })).map(
          item => item.runId
        )
      ).toContain(work.run_id);
      cli(['work', 'update', '--transition', 'end', '--agent-id', cliAgent, '--run-id', work.run_id]);
      const audit = native(store => store.auditChecks({ agentId: cliAgent }));
      expect(JSON.stringify(audit)).toContain(work.run_id);
      expect(readFileSync(path.join(workspace, 'shared.txt'), 'utf8')).toBe(
        'verified fixture\n'
      );
      cli([
        'work',
        'verify',
        '--action',
        'mark',
        '--agent-id',
        cliAgent,
        '--run-id',
        work.run_id,
        '--status',
        'SUCCESS',
        '--message',
        'Interop fixture exists and contains the asserted content.',
      ]);
      cli(['work', 'verify', '--action', 'audit', '--agent-id', cliAgent]);
    });

    it('runs the quoted Awareness runner through the real Pi bash tool with native storage and identity', async () => {
      let bash: ToolDefinition | undefined;
      registerBashTool(
        {
          registerTool: tool => {
            bash = tool;
          },
        },
        new Set(),
        registerUniqueTool
      );
      const result = await bash!.execute(
        'awareness-interop',
        {
          queries: [
            {
              reasoning:
                'Send a fixture message through the installed Awareness CLI using native Pi bindings.',
              command:
                '"$OCTOCODE_NODE" "$OCTOCODE_AWARENESS_CLI" --db "$OCTOCODE_AWARENESS_DB" message send --agent-id "$OCTOCODE_AGENT_ID" --workspace "$OCTOCODE_AWARENESS_WORKSPACE" --to-agent external-skill-agent --kind fyi --subject "Bash bridge" --body "Native identity and database" --compact',
              timeout: 10,
            },
          ],
        },
        undefined,
        undefined,
        {
          cwd: workspace,
          sessionManager: { getSessionId: () => 'interop-session' },
        } as PiContext
      );
      const text = result.content
        .map(item => ('text' in item ? item.text : ''))
        .join('\n');
      expect(result.isError, text).toBeFalsy();
      const inbox = cli([
        'message',
        'list',
        '--agent-id',
        cliAgent,
        '--include-bodies',
      ]);
      expect(inbox.signals).toHaveLength(1);
      const message = native(store =>
        store.listMessages({ agentId: cliAgent })
      );
      expect(message).toHaveLength(1);
      expect(message[0]!.fromAgentId).toBe(piAgent);
      expect(message[0]!.text).toBe('Native identity and database');
      expect(message[0]!.messageId).toBe(inbox.signals[0].signal_id);
    });
  }
);
