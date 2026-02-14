#!/usr/bin/env node

/**
 * Development helper: starts the atomic-server API and the Vite web frontend concurrently.
 *
 * Usage:
 *   npm run dev:server                         # uses ./atomic.db, API on :8080, Vite on :1420
 *   npm run dev:server -- --db-path /path/to/db --port 9000
 *
 * Both processes are killed together on Ctrl+C.
 */

import { spawn } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');

// Forward any extra CLI args to atomic-server (e.g. --db-path, --port)
const serverArgs = process.argv.slice(2);

const children = [];

function startProcess(name, command, args, opts = {}) {
  const proc = spawn(command, args, {
    cwd: root,
    stdio: 'pipe',
    env: { ...process.env, ...opts.env },
  });

  const prefix = name.padEnd(8);

  proc.stdout.on('data', (data) => {
    for (const line of data.toString().split('\n').filter(Boolean)) {
      console.log(`\x1b[36m[${prefix}]\x1b[0m ${line}`);
    }
  });

  proc.stderr.on('data', (data) => {
    for (const line of data.toString().split('\n').filter(Boolean)) {
      console.log(`\x1b[33m[${prefix}]\x1b[0m ${line}`);
    }
  });

  proc.on('exit', (code) => {
    console.log(`\x1b[90m[${prefix}] exited (code ${code})\x1b[0m`);
  });

  children.push(proc);
  return proc;
}

// Start atomic-server (cargo run)
startProcess('api', 'cargo', ['run', '-p', 'atomic-server', '--', ...serverArgs], {});

// Start Vite dev server in web mode
startProcess('web', 'npx', ['vite', '--host'], {
  env: { VITE_BUILD_TARGET: 'web' },
});

// Clean shutdown on Ctrl+C
function cleanup() {
  for (const child of children) {
    if (!child.killed) {
      child.kill('SIGTERM');
    }
  }
}

process.on('SIGINT', cleanup);
process.on('SIGTERM', cleanup);
