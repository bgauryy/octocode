# Chrome DevTools skill

Guidance only. Browser execution, CLI, MCP, references and tests live in the `@octocodeai/octocode-chrome-devtools` package.

Run `octocode-chrome-devtools /cli --help` to discover commands and `octocode-chrome-devtools /cli skill --json` to read complete operating guidance. Default `octocode-chrome-devtools` starts MCP stdio. Run both from the workspace cwd.

In this monorepo, build with `yarn workspace @octocodeai/octocode-chrome-devtools build`. Without a linked executable, use `node /absolute/repo/packages/octocode-chrome-devtools/bin/octocode-chrome-devtools.mjs /cli --help`. The package is currently private and is not published to npm; local installation uses its built folder.

The skill folder has no scripts, dependencies or copied implementation. Install the package separately from this guidance. Agent instructions are in [SKILL.md](SKILL.md).
