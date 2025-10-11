#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

function findRepoRoot(startDir) {
  let current = startDir;
  while (true) {
    if (fs.existsSync(path.join(current, 'web', 'package.json'))) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return null;
    }
    current = parent;
  }
}

console.log('Checking frontend build...');

const repoRoot = findRepoRoot(process.cwd());
if (!repoRoot) {
  console.error('Unable to locate web/package.json; aborting Tauri build.');
  process.exit(1);
}

const webDir = path.join(repoRoot, 'web');
const distDir = path.join(webDir, 'dist');

try {
  if (fs.existsSync(distDir) && fs.readdirSync(distDir).length > 0) {
    console.log('Frontend already built');
    process.exit(0);
  }
} catch (error) {
  console.warn(`Unable to inspect dist directory: ${error.message}`);
}

console.log(`Running npm run build in ${webDir}`);
const npmBin = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const result = spawnSync(npmBin, ['run', 'build'], {
  cwd: webDir,
  stdio: 'inherit',
});

if (result.error) {
  console.error(result.error.message);
}

process.exit(result.status ?? 1);
