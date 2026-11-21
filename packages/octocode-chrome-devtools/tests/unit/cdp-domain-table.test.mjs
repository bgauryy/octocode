// The instruction map of CDP domains must not drift from the installed Chrome's own protocol.
import test from 'node:test';
import assert from 'node:assert/strict';
import { chromeDomainFamilies } from '@octocodeai/config/chrome';
import {
  chromeBinary,
  startThrowawayChrome,
} from '../fixtures/throwaway-chrome.mjs';

const skip = !chromeBinary && 'no Chrome installed';
const entries = chromeDomainFamilies.flatMap(({ family, domains }) =>
  domains.map(entry => ({ family, ...entry }))
);
const named = new Set(entries.map(entry => entry.name));

test(
  'the CDP domain map matches the live protocol',
  { skip, timeout: 60000 },
  async () => {
    const chrome = await startThrowawayChrome();
    try {
      const live = new Map(
        chrome.protocol.domains.map(domain => [
          domain.domain,
          domain.commands?.some(command => command.name === 'enable') ?? false,
        ])
      );
      for (const name of named)
        assert.ok(live.has(name), `${name} is not a domain of this Chrome`);
      for (const entry of entries.filter(item => item.mark))
        assert.ok(
          live.get(entry.name),
          `${entry.name} is marked E without an enable command`
        );
      for (const entry of entries.filter(
        item => item.family === 'needs-enable'
      ))
        assert.ok(entry.mark, `${entry.name} needs enable but carries no mark`);
    } finally {
      await chrome.stop();
    }
  }
);
