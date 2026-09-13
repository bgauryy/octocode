import { describe, expect, it } from 'vitest';
import {
  AWARENESS_AGENT_INSTRUCTION_SECTIONS,
  AWARENESS_MESSAGE_PARAMETER_GUIDANCE,
  getAwarenessAgentInstructions,
  type AwarenessAgentInstructionSection,
} from '../src/agent-instructions.js';

describe('portable Awareness agent instructions', () => {
  it('renders the complete workflow without host bindings or database access', () => {
    const instructions = getAwarenessAgentInstructions();
    for (const section of AWARENESS_AGENT_INSTRUCTION_SECTIONS) {
      expect(instructions).toContain(`## ${section}\n`);
    }
    expect(instructions).toContain('context.observe');
    expect(instructions).toContain('context.feedback');
    expect(instructions).toContain('context.orient');
    expect(instructions).toContain('Unknown sensors do not imply degraded recovery');
    expect(instructions).toContain('advisory');
    expect(instructions).toContain(AWARENESS_MESSAGE_PARAMETER_GUIDANCE);
    expect(instructions).toContain('API parameters use snake_case');
    expect(instructions).toContain('include_bodies:true, not bodies');
    expect(instructions).toContain('to_agent (an array), file, and ref_id');
    expect(instructions).toContain('in_reply_to to the returned signal_id and requires its own subject; use in_reply_to, not notification_id');
    expect(instructions).toContain('The live operation descriptor is the field-level source of truth');
    expect(instructions).not.toContain('to_agents');
    expect(instructions).not.toContain('kinds(');
    expect(instructions).not.toContain('files(');
    expect(instructions).not.toContain('refs(');
    expect(instructions).not.toContain('schema below instead of the live descriptor');
  });

  it('composes selected sections once in stable order', () => {
    const instructions = getAwarenessAgentInstructions({ sections: ['feedback', 'observe', 'feedback'] });
    expect(instructions).toBe([
      getAwarenessAgentInstructions({ sections: ['observe'] }),
      getAwarenessAgentInstructions({ sections: ['feedback'] }),
    ].join('\n\n'));
    expect(instructions).not.toContain('## coordination');
    expect(getAwarenessAgentInstructions({ sections: [] })).toBe('');
  });

  it('rejects unsupported sections instead of silently dropping instructions', () => {
    expect(() => getAwarenessAgentInstructions({
      sections: ['execution' as AwarenessAgentInstructionSection],
    })).toThrow('Unknown Awareness instruction section: execution');
  });
});
