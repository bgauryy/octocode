export const HELP = `  🐙 Octocode Awareness

  ONE SURFACE · FIVE CONCEPTS · NINETEEN OPERATIONS
    context  orient
    work     create · list · show · claim · update · depend · protect · verify
    message  list · send · reply · resolve
    memory   recall · record
    history  status · timeline · read · restore

  RUNNER  npx @octocodeai/octocode-awareness <concept> <operation> [options]
  SCHEMA  schema commands --compact
  EXACT   schema command <concept> <operation> --compact
  ENTITY  schema entities --compact
  CONTEXT --workspace <repo> · --agent-id <id> · --db-scope repo|global · --db <path>
  OUTPUT  --compact lean JSON; partial results include executable next calls`;

export const HELP_COMPACT = `octocode-awareness: one direct surface; call context orient once, then <concept> <operation>.
concepts: context=orient; work=create|list|show|claim|update|depend|protect|verify; message=list|send|reply|resolve; memory=recall|record; history=status|timeline|read|restore
schema: schema commands --compact; exact: schema command <concept> <operation> --compact; entities: schema entities --compact`;
