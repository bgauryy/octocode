#!/usr/bin/env node
// The `octocode` bin as published: start the Node launcher. The build copies
// this file to out/octocode; the postinstall step replaces that copy with the
// native binary when it can, so the command then runs without Node. A dynamic
// import loads the launcher whether Node reads this extensionless file as an
// ES module or as CommonJS.
void import('./octocode.js');
