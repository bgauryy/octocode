import { getOctocodeHome } from '../../../packages/octocode-config/src/home.ts';
import { propagateOctocodeEnv } from '../../../packages/octocode-config/src/dotenv.ts';

export function loadOctocodeEnv() {
  const env = process.env;
  return propagateOctocodeEnv({ home: getOctocodeHome(env), cwd: process.cwd(), env });
}
