#!/usr/bin/env node

const path = require('path');
const { spawnSync } = require('child_process');
const { resolveAppVersion } = require('./resolve-app-version.cjs');

const UI_DIR = path.resolve(__dirname, '..');
const builderBinary = process.platform === 'win32'
  ? path.join(UI_DIR, 'node_modules', '.bin', 'electron-builder.cmd')
  : path.join(UI_DIR, 'node_modules', '.bin', 'electron-builder');

const appVersion = resolveAppVersion();
const args = [...process.argv.slice(2), `-c.extraMetadata.version=${appVersion}`];
const isWindows = process.platform === 'win32';

console.log(`Using app version ${appVersion}`);

const result = spawnSync(builderBinary, args, {
  cwd: UI_DIR,
  stdio: 'inherit',
  shell: isWindows,
  env: {
    ...process.env,
    GENERIC_CODER_APP_VERSION: appVersion,
  },
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status === null ? 1 : result.status);