// Domain playbooks: data shape offline, and every method and key parameter a playbook names against
// the live protocol of the installed Chrome (a throwaway headless Chrome). Skipped without Chrome.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  chromeBinary,
  startThrowawayChrome,
} from '../fixtures/throwaway-chrome.mjs';
import { describeDomain, playbookOf } from '../../dist/engine/cdp-schema.mjs';

const root = join(import.meta.dirname, '../..');
const playbooks = JSON.parse(
  readFileSync(join(root, 'src/engine/cdp-playbooks.json'), 'utf8')
);
const STEP = /^([A-Z][A-Za-z0-9]*)\.([A-Za-z0-9]+)(?: \{([A-Za-z0-9, ]+)\})?$/;
const skip = !chromeBinary && 'no Chrome installed';

test('every playbook has a short summary, 4 to 6 well-formed steps and about twenty domains exist', () => {
  const names = Object.keys(playbooks);
  assert.ok(names.length >= 20, `${names.length} playbooks`);
  for (const [domain, playbook] of Object.entries(playbooks)) {
    assert.ok(playbook.summary.split(/\s+/).length <= 20, `${domain} summary`);
    assert.ok(
      playbook.steps.length >= 4 && playbook.steps.length <= 6,
      `${domain} has ${playbook.steps.length} steps`
    );
    for (const step of playbook.steps)
      assert.match(step, STEP, `${domain}: ${step}`);
    assert.ok(
      Object.keys(playbook).every(key =>
        ['summary', 'steps', 'notes'].includes(key)
      )
    );
  }
  assert.equal(playbookOf('Network').summary, playbooks.Network.summary);
  assert.equal(playbookOf('NoSuchDomain'), undefined);
  assert.equal(playbookOf('constructor'), undefined);
});

test(
  'every method and key parameter a playbook names exists in the live protocol, and protocol Domain carries the playbook',
  { skip, timeout: 60000 },
  async () => {
    const chrome = await startThrowawayChrome();
    try {
      const schema = chrome.protocol;
      for (const [domain, playbook] of Object.entries(playbooks)) {
        const found = schema.domains.find(item => item.domain === domain);
        assert.ok(found, `${domain} is not a domain of ${chrome.version}`);
        for (const step of playbook.steps) {
          const [, stepDomain, method, keys = ''] = STEP.exec(step);
          const command = schema.domains
            .find(item => item.domain === stepDomain)
            ?.commands?.find(item => item.name === method);
          assert.ok(command, `${step}: no such method`);
          assert.ok(!command.deprecated, `${step} is deprecated`);
          const known = (command.parameters ?? []).map(item => item.name);
          for (const key of keys
            .split(',')
            .map(item => item.trim())
            .filter(Boolean))
            assert.ok(
              known.includes(key),
              `${step}: ${key} is not a parameter (${known.join(', ')})`
            );
        }
        assert.deepEqual(describeDomain(found, schema).playbook, playbook);
      }
      const bare = schema.domains.find(item => !playbooks[item.domain]);
      assert.equal('playbook' in describeDomain(bare, schema), false);
    } finally {
      await chrome.stop();
    }
  }
);
