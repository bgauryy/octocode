#!/usr/bin/env node
import { Express } from "express";

//#region src/server.d.ts
declare const PID_FILE: string;
declare function createServer(): Promise<Express>;
declare function startServer(): Promise<void>;
//#endregion
export { PID_FILE, createServer, startServer };