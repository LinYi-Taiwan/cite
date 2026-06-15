#!/usr/bin/env node
// Postinstall: download the prebuilt `cite` native binary that matches this
// platform/arch from the matching GitHub Release, verify its sha256 against the
// uploaded checksum, and extract it into ./bin/. The asset-name template here is
// the single contract with .github/workflows/release.yml (archive: cite-$tag-$target).

'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { spawnSync } = require('child_process');

const REPO = 'LinYi-Taiwan/cite';
const { version } = require('./package.json');
const tag = `v${version}`;

// process.platform + process.arch -> Rust target triple. Must mirror the
// release.yml build matrix exactly; an unmapped combo fails loudly below.
const TARGETS = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'linux-x64': 'x86_64-unknown-linux-musl',
};

async function main() {
  const key = `${process.platform}-${process.arch}`;
  const target = TARGETS[key];
  if (!target) {
    fail(
      `Unsupported platform/arch: ${key}.\n` +
        `cite ships prebuilt binaries for: ${Object.keys(TARGETS).join(', ')}.\n` +
        `Build from source instead: https://github.com/${REPO}`
    );
  }

  const archive = `cite-${tag}-${target}.tar.gz`;
  const base = `https://github.com/${REPO}/releases/download/${tag}`;
  const archiveUrl = `${base}/${archive}`;
  const checksumUrl = `${archiveUrl}.sha256`;

  const binDir = path.join(__dirname, 'bin');
  fs.mkdirSync(binDir, { recursive: true });

  console.log(`cite: downloading ${archive} ...`);
  const tarball = await download(archiveUrl);

  console.log('cite: verifying sha256 ...');
  const expected = parseChecksum(await downloadText(checksumUrl), archive);
  const actual = crypto.createHash('sha256').update(tarball).digest('hex');
  if (actual !== expected) {
    fail(`Checksum mismatch for ${archive}.\n  expected: ${expected}\n  actual:   ${actual}`);
  }

  // Stage the tarball and extract just the `cite` binary with the system tar.
  // Throw (don't fail()/exit) inside the try so the `finally` always reclaims the
  // temp dir — process.exit() would skip cleanup and leak it on every failed install.
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'cite-'));
  try {
    const tarPath = path.join(tmp, archive);
    fs.writeFileSync(tarPath, tarball);

    const res = spawnSync('tar', ['-xzf', tarPath, '-C', binDir, 'cite'], { stdio: 'inherit' });
    if (res.error) {
      throw new Error(`Failed to run tar (is it on PATH?): ${res.error.message}`);
    }
    if (res.status !== 0) {
      throw new Error(`Failed to extract ${archive} (tar exit ${res.status}).`);
    }

    const binPath = path.join(binDir, 'cite');
    if (!fs.existsSync(binPath)) {
      throw new Error(`Extraction succeeded but ${binPath} is missing.`);
    }
    fs.chmodSync(binPath, 0o755);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
  console.log(`cite ${tag} installed for ${target}.`);
}

// Fetch a URL into a Buffer, following GitHub's redirect to the CDN.
async function download(url) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok) {
    fail(`Download failed: ${url} -> HTTP ${res.status} ${res.statusText}`);
  }
  return Buffer.from(await res.arrayBuffer());
}

async function downloadText(url) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok) {
    fail(`Download failed: ${url} -> HTTP ${res.status} ${res.statusText}`);
  }
  return res.text();
}

// Extract the expected sha256 for `archive`. Accept either the standard
// `sha256sum` format ("<hex>  <filename>", possibly multi-asset) by matching the
// line that names our archive, OR an unambiguous bare single-hash file. Reject
// anything else — a blind "first hex token wins" fallback would let an HTML error
// page or a redirect body that happens to contain a 64-hex run smuggle in an
// attacker-chosen digest and bypass the integrity check.
function parseChecksum(text, archive) {
  const lines = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter(Boolean);
  const named = lines.find((l) => l.includes(archive));
  let hex;
  if (named) {
    hex = named.split(/\s+/)[0];
  } else if (lines.length === 1 && /^[0-9a-f]{64}$/i.test(lines[0])) {
    hex = lines[0];
  } else {
    fail(`Checksum file does not reference ${archive}:\n${text}`);
  }
  if (!/^[0-9a-f]{64}$/i.test(hex)) {
    fail(`Could not parse sha256 from checksum file:\n${text}`);
  }
  return hex.toLowerCase();
}

function fail(msg) {
  console.error(`\ncite install error:\n${msg}\n`);
  process.exit(1);
}

main().catch((err) => fail(err && err.stack ? err.stack : String(err)));
