import { describe, expect, it } from 'vitest';
import { awarenessEntityCatalog } from '../src/schema/entities.js';

describe('entity lifecycle catalog', () => {
  it('publishes one owner and explicit storage lifecycle semantics for every relation', () => {
    const entities = awarenessEntityCatalog().entities;
    expect(entities).toHaveLength(30);
    for (const entity of entities) {
      expect(entity.owner, entity.name).toMatch(/^(storage|work|message|memory|context|history|host|infrastructure)$/);
      expect(entity.lifecycle, entity.name).toMatchObject({
        access: expect.stringMatching(/^(read-write|derived-index)$/),
        retention: expect.any(String),
        deletion: expect.any(String),
        cleanup_operation: expect.stringMatching(/^(maintenance retention|maintenance store-retire)$/),
      });
    }
    expect(entities.find(entity => entity.name === 'awareness_agents')).toMatchObject({
      owner: 'work', lifecycle: { access: 'read-write', retention: 'domain-lifecycle', deletion: 'store-only' },
    });
    expect(entities.find(entity => entity.name === 'awareness_locks')).toMatchObject({
      owner: 'work', lifecycle: { retention: 'lease-bound', deletion: 'expiry-maintenance' },
    });
    expect(entities.find(entity => entity.name === 'memories_fts')).toMatchObject({
      owner: 'memory', lifecycle: { access: 'derived-index', deletion: 'index-rebuild' },
    });
    expect(entities.find(entity => entity.name === 'event_outbox')).toMatchObject({
      owner: 'infrastructure', lifecycle: { retention: 'retention-class' },
    });
    expect(entities.find(entity => entity.name === 'sessions')).toMatchObject({
      owner: 'work', lifecycle: { retention: 'end-state', deletion: 'store-only' },
    });
    expect(entities.find(entity => entity.name === 'local_history_restores')).toMatchObject({
      owner: 'history', lifecycle: {
        retention: 'lease-and-receipt', deletion: 'expired-ready-maintenance', cleanup_operation: 'maintenance retention',
      },
    });
    expect(entities.find(entity => entity.name === 'signals')?.lifecycle.cleanup_operation).toBe('maintenance retention');
    expect(entities.find(entity => entity.name === 'signal_reads')?.lifecycle.cleanup_operation).toBe('maintenance retention');
    expect(entities.find(entity => entity.name === 'memory_refs')?.lifecycle.cleanup_operation).toBe('maintenance retention');
    expect(entities.find(entity => entity.name === 'pending_interactions')).toMatchObject({
      lifecycle: {
        retention: 'domain-lifecycle', deletion: 'retention-maintenance', cleanup_operation: 'maintenance retention',
      },
    });
    expect(entities.find(entity => entity.name === 'authorization_receipts')?.lifecycle.cleanup_operation).toBe('maintenance store-retire');
    expect(entities.find(entity => entity.name === 'awareness_plans')?.lifecycle.cleanup_operation).toBe('maintenance store-retire');
  });
});
