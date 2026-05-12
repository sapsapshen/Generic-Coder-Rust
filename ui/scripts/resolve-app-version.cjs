#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');

const UI_DIR = path.resolve(__dirname, '..');
const REPO_ROOT = path.resolve(UI_DIR, '..');
const PACKAGE_JSON_PATH = path.join(UI_DIR, 'package.json');
const SEMVER_PATTERN = /^v?(\d+)\.(\d+)\.(\d+)$/;

function normalizeSemver(value) {
  const trimmed = String(value || '').trim();
  const match = trimmed.match(SEMVER_PATTERN);
  if (!match) {
    return null;
  }

  return `${match[1]}.${match[2]}.${match[3]}`;
}

function incrementPatch(version) {
  const match = version.match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!match) {
    throw new Error(`Invalid semver version: ${version}`);
  }

  return `${match[1]}.${match[2]}.${Number(match[3]) + 1}`;
}

function compareVersions(left, right) {
  const leftParts = left.split('.').map(Number);
  const rightParts = right.split('.').map(Number);

  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const difference = (leftParts[index] || 0) - (rightParts[index] || 0);
    if (difference !== 0) {
      return difference;
    }
  }

  return 0;
}

function readPackageVersion() {
  const raw = fs.readFileSync(PACKAGE_JSON_PATH, 'utf8');
  const pkg = JSON.parse(raw);
  const version = normalizeSemver(pkg.version);
  if (!version) {
    throw new Error(`package.json version is not a supported semver value: ${pkg.version}`);
  }

  return version;
}

function readLatestTagVersion() {
  try {
    const output = execFileSync('git', ['-C', REPO_ROOT, 'tag', '--sort=-version:refname'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });

    const tags = output
      .split(/\r?\n/)
      .map(tag => normalizeSemver(tag))
      .filter(Boolean);

    return tags[0] || null;
  } catch {
    return null;
  }
}

function resolveAppVersion() {
  const explicitVersion = normalizeSemver(process.env.GENERIC_CODER_APP_VERSION);
  if (explicitVersion) {
    return explicitVersion;
  }

  const packageVersion = readPackageVersion();
  const latestTagVersion = readLatestTagVersion();
  if (latestTagVersion) {
    const nextTagVersion = incrementPatch(latestTagVersion);
    return compareVersions(packageVersion, nextTagVersion) > 0 ? packageVersion : nextTagVersion;
  }

  return packageVersion;
}

module.exports = {
  resolveAppVersion,
};

if (require.main === module) {
  process.stdout.write(resolveAppVersion());
}