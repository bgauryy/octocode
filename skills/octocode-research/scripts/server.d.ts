#!/usr/bin/env node
import { Express } from "express";

declare const PID_FILE: string;
declare function createServer(): Promise<Express>;
declare function startServer(): Promise<void>;
export { PID_FILE, createServer, startServer };