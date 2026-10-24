import type { CheckSession } from '../cdp-runner-session.mjs';

export async function run(cdp: CheckSession) {
  const protocol = await cdp.protocol();
  console.log(
    `[METRIC] CDP domains=${protocol.domains.length} version=${JSON.stringify(protocol.version)}`
  );
  cdp.saveArtifact('cdp-protocol.json', protocol);
}
