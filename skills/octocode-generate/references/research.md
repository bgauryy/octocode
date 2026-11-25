# Research and recovery

Load before researching scaffold patterns, and when a validation gate fails.

## Progressive discovery

Package → repository → structure → pattern → content.

| Need | Tool | Example |
|---|---|---|
| Existing workspace conventions | `structureSearch`, then `localFetch` | `package.json`, `tsconfig.json`, lint config |
| Package version and source repository | `artifactSearch` | resolve `create-next-app`, then read its repository |
| Template repositories | `ghSearchRepo` | keywords `nextjs tailwind starter`, sorted by stars |
| Official examples | `ghStructure` | `vercel/next.js`, path `examples` |
| Config or integration patterns | `ghSearchCode` | keywords `createTRPCNext`, extension `ts` |
| Read an implementation | `ghGetFileContent` | `matchString` on the function or config key |

| From | Need | Go to |
|---|---|---|
| `structureSearch` | Existing patterns | `localFetch` |
| `artifactSearch` | Source code | `ghStructure`, then `ghGetFileContent` |
| `ghSearchRepo` | Layout | `ghStructure` |
| `ghSearchRepo` | Patterns | `ghSearchCode` |
| `ghStructure` | File content | `ghGetFileContent` |
| `ghSearchCode` | Full context | `ghGetFileContent` |

## Research dimensions

| Dimension | Goal |
|---|---|
| Official examples | Canonical patterns from the framework's own repository |
| Popular templates | Community starters and templates |
| Integration code | How libraries connect (tRPC with Next.js, Prisma with auth) |
| Config files | Tooling setup (tsconfig, eslint, tailwind, framework config) |
| Structure | Folder organization and file naming |

## Quality guards

- Prefer repositories updated within the last 6 months.
- Prefer repositories with more than 500 stars for community patterns.
- Check active maintenance: recent commits and pull requests.
- Match the pattern to the framework version you install; patterns differ across major versions.

## Error recovery

| Error | Action |
|---|---|
| Build fails | Read the error; fix imports or types; re-validate |
| Missing types | Install `@types/<package>` as a dev dependency |
| Version conflict | Research compatible versions of the peer dependencies |
| Config error | Research the correct config for the installed version |
| CLI fails | Check the CLI's current flags in its repository or docs |
| Research finds nothing | Try other spellings, broaden scope, then ask the user |
