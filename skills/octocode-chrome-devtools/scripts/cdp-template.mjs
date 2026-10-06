// Adapt this run to the question. Inspect the live schema before choosing methods.
export async function run(cdp) {
  const protocol = await cdp.protocol();
  cdp.saveArtifact('cdp-protocol.json', protocol);
  // Example read-only capture. Replace it with the methods and sessions the task needs.
  await cdp.send('DOMSnapshot.enable');
  const snapshot = await cdp.send('DOMSnapshot.captureSnapshot', {
    computedStyles: [], includeDOMRects: true, includePaintOrder: true,
  });
  cdp.saveArtifact('dom-snapshot.json', snapshot);
}
