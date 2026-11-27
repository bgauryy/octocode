#!/usr/bin/env node
import { main } from '../serve.ts';

await main(process.argv.slice(2));
