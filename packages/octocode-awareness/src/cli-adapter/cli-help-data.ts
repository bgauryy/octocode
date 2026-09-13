export const HELP = `  🐙 Octocode Awareness

  ONE SURFACE · FIVE CONCEPTS
    context  orient · observe · feedback
    work     create · list · show · claim · update · depend · protect · verify
    message  list · send · reply · resolve
    memory   recall · record · set · get · revalidate
    history  status · timeline · read · restore · experience

  RUNNER  npx @octocodeai/octocode-awareness <concept> <operation> [options]
  SCHEMA  schema commands --compact
  EXACT   schema command <concept> <operation> --compact
  ENTITY  schema entities --compact
  AGENTS  instructions [--section start|observe|advise|feedback|coordination|trust|schema]
  IMPORT  getAwarenessAgentInstructions from @octocodeai/octocode-awareness
  CONTEXT --workspace <repo> · --agent-id <id> · --session-id <run> · --db-scope repo|global · --db <path>
  ACTIVE  context orient → interpreted run_state and brief regulation.nudge
  PASSIVE context observe --acquisition passive → receipt and new-episode nudge
  OUTPUT  --compact lean JSON; partial results include executable next calls`;

export const HELP_COMPACT = `octocode-awareness: one direct surface; call context orient once, then <concept> <operation>.
concepts: context=orient|observe|feedback; work=create|list|show|claim|update|depend|protect|verify; message=list|send|reply|resolve; memory=recall|record|set|get|revalidate; history=status|timeline|read|restore|experience
schema: schema commands --compact; exact: schema command <concept> <operation> --compact; entities: schema entities --compact
agents: instructions [--section <name>] (repeatable; sections: start|observe|advise|feedback|coordination|trust|schema); API: getAwarenessAgentInstructions
flows: active=context orient; passive=context observe --acquisition passive (session required; new-episode nudge only)`;
