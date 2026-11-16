#!/usr/bin/env node
// The package's postinstall: run the bundled installer (out/install.js). A
// source checkout installs its dependencies before anything is built, so
// without out/install.js there is nothing to do.
import { existsSync } from 'node:fs';

const installer = new URL('../out/install.js', import.meta.url);
if (existsSync(installer)) await import(installer.href);
