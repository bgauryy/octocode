# Octocode Chrome DevTools

Collect live Chrome DevTools Protocol evidence (DOM actionability, HAR, console, performance, storage, authenticated pages) into files, not chat. Agent rules live in [SKILL.md](SKILL.md).

## Install

Requires Chrome and Node.js 24+ (sandboxed `--allow-net` needs 25+).

```bash
npx -y octocode skill install octocode-chrome-devtools
```

## Maintainer verification

Run the suites named in the last line of `SKILL.md`, then the `octocode-skills` review against this folder.
