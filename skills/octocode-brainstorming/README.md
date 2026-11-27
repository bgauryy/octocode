# Octocode Brainstorming

Explore an idea, claim, or decision from several directions and recommend from quote-verified evidence: generate options, try to refute each, find best practices, or compare options.

Start with [SKILL.md](SKILL.md).

Setup: the search keys `TAVILY_API_KEY`, `EXA_API_KEY`, `SERPER_API_KEY` and `DEEPSEEK_API_KEY` are optional; without them the host's web tools still work. Each key is read from the process environment, then `~/.octocode/.env` (or `$OCTOCODE_HOME/.env`), then `<workspace>/.octocode/.env`. From the workspace root, check all four with `node scripts/keys.mjs get TAVILY_API_KEY EXA_API_KEY SERPER_API_KEY DEEPSEEK_API_KEY` and save one with `node scripts/keys.mjs set TAVILY_API_KEY --stdin`.

`scripts/keys.mjs` is built from `@octocodeai/config` by this folder's private `package.json` (`yarn build`); edit `src/keys.mjs` or the package, never the built file.
