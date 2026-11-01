// In-repo hub for the Chrome DevTools contract. Re-exports the authoritative
// source in @octocodeai/octocode-core on its own entry, so the chrome package
// does not load the research tool contract.
export * from "@octocodeai/octocode-core/chrome";
