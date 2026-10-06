export async function run(cdp) {
  const protocol = await cdp.protocol();
  console.log(`[METRIC] CDP domains=${protocol.domains.length} version=${JSON.stringify(protocol.version)}`);
  cdp.saveArtifact('cdp-protocol.json', protocol);
}
