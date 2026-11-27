#!/usr/bin/env node
import { main } from '../query.ts';

await main(process.argv.slice(2));
