import { c, dim } from '../../../utils/colors.js';
import { input } from '../../../utils/prompts.js';
import { dirExists } from '../../../utils/fs.js';
import path from 'node:path';

function expandPath(inputPath: string): string {
  if (inputPath.startsWith('~')) {
    const home = process.env.HOME || process.env.USERPROFILE || '';
    return path.join(home, inputPath.slice(1));
  }
  return inputPath;
}

export async function promptCustomPath(): Promise<string | null> {
  console.log();
  console.log(
    `  ${c('blue', 'INFO')} Enter the full path to your MCP config file (JSON)`
  );
  console.log(`  ${dim('Leave empty to go back')}`);
  console.log();
  console.log(`  ${dim('Common paths:')}`);
  console.log(`    ${dim('•')} ~/.cursor/mcp.json ${dim('(Cursor)')}`);
  console.log(
    `    ${dim('•')} ~/Library/Application Support/Claude/claude_desktop_config.json`
  );
  console.log(`      ${dim('(Claude Desktop)')}`);
  console.log(`    ${dim('•')} ~/.claude.json ${dim('(Claude Code)')}`);
  console.log(
    `    ${dim('•')} ~/.config/opencode/config.json ${dim('(Opencode)')}`
  );
  console.log(
    `    ${dim('•')} ~/.codeium/windsurf/mcp_config.json ${dim('(Windsurf)')}`
  );
  console.log(
    `    ${dim('•')} ~/Library/Application Support/Trae/mcp.json ${dim('(Trae)')}`
  );
  console.log(
    `    ${dim('•')} ~/.gemini/antigravity/mcp_config.json ${dim('(Antigravity)')}`
  );
  console.log(`    ${dim('•')} ~/.config/zed/settings.json ${dim('(Zed)')}`);
  console.log(`    ${dim('•')} ~/.continue/config.json ${dim('(Continue)')}`);
  console.log(
    `    ${dim('•')} ~/.codex/config.toml ${dim('(Codex - TOML format)')}`
  );
  console.log(`    ${dim('•')} ~/.gemini/settings.json ${dim('(Gemini CLI)')}`);
  console.log(`    ${dim('•')} ~/.kiro/mcp.json ${dim('(Kiro)')}`);
  console.log();

  const customPath = await input({
    message: 'MCP config path (or press Enter to go back):',
    validate: (value: string) => {
      if (!value.trim()) {
        return true;
      }

      const expandedPath = expandPath(value);

      if (!expandedPath.endsWith('.json')) {
        return 'Path must be a .json file (e.g., mcp.json, config.json)';
      }

      if (!path.isAbsolute(expandedPath)) {
        return 'Please provide an absolute path (starting with / or ~)';
      }

      const parentDir = path.dirname(expandedPath);
      if (!dirExists(parentDir)) {
        return `Parent directory does not exist: ${parentDir}\nCreate it first or choose a different location.`;
      }

      return true;
    },
  });

  if (!customPath || !customPath.trim()) return null;

  return expandPath(customPath);
}
