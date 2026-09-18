import { AWARENESS_CONCEPTS, listAwarenessOperationDescriptors } from '../schema/operation-catalog.js';

const operationsByConcept = Object.fromEntries(AWARENESS_CONCEPTS.map(concept => [
  concept,
  listAwarenessOperationDescriptors()
    .filter(descriptor => descriptor.concept === concept)
    .map(descriptor => descriptor.operation.slice(concept.length + 1)),
])) as Record<(typeof AWARENESS_CONCEPTS)[number], string[]>;

const conceptHelp = AWARENESS_CONCEPTS
  .map(concept => `    ${concept.padEnd(8)} ${operationsByConcept[concept].join(' · ')}`)
  .join('\n');
const compactConceptHelp = AWARENESS_CONCEPTS
  .map(concept => `${concept}=${operationsByConcept[concept].join('|')}`)
  .join('; ');

export const HELP = `  🐙 Octocode Awareness

  ONE SURFACE · FIVE CONCEPTS
${conceptHelp}

  RUNNER  npx @octocodeai/octocode-awareness <concept> <operation> [options]
  SCHEMA  schema commands --compact
  EXACT   schema command <concept> <operation> --compact
  ENTITY  schema entities --compact
  VIEW    view [--workspace <repo>] [--out <file>] [--no-open]
  OPERATE history evidence [--action report|reclaim] [--confirm reclaim]
          maintenance retention [--action report|apply] [--confirm apply-retention]
          maintenance store-retire [--action report|apply] [--confirm retire]
  AGENTS  instructions [--section start|observe|advise|feedback|coordination|trust|schema]
  IMPORT  getAwarenessAgentInstructions from @octocodeai/octocode-awareness
  CONTEXT --workspace <repo> · --agent-id <id> · --session-id <run> · --db-scope repo|global · --db <path>
  ACTIVE  context orient → interpreted run_state and brief regulation.nudge
  PASSIVE context observe --acquisition passive → receipt and new-episode nudge
  OUTPUT  --compact lean JSON; partial results include executable next calls`;

export const HELP_COMPACT = `octocode-awareness: one direct surface; call context orient once, then <concept> <operation>.
concepts: ${compactConceptHelp}
schema: schema commands --compact; exact: schema command <concept> <operation> --compact; entities: schema entities --compact; operator: view [--out <file>] [--no-open] | history evidence [--action report|reclaim] [--confirm reclaim] | maintenance retention [--action report|apply] [--confirm apply-retention]
agents: instructions [--section <name>] (repeatable; sections: start|observe|advise|feedback|coordination|trust|schema); API: getAwarenessAgentInstructions
flows: active=context orient; passive=context observe --acquisition passive (session required; new-episode nudge only)`;
