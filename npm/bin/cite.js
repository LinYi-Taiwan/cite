#!/usr/bin/env node
// Shim: spawn the native `cite` binary (placed here by install.js), forwarding
// argv and propagating its exit code / terminating signal.

'use strict';

const path = require('path');
const fs = require('fs');
const { spawnSync } = require('child_process');

const binPath = path.join(__dirname, 'cite');
if (!fs.existsSync(binPath)) {
  console.error(
    'cite: native binary not found. The postinstall download may have failed — ' +
      'reinstall the package, or build from source: https://github.com/LinYi-Taiwan/cite'
  );
  process.exit(1);
}

const res = spawnSync(binPath, process.argv.slice(2), { stdio: 'inherit' });
if (res.error) {
  console.error(`cite: failed to launch native binary: ${res.error.message}`);
  process.exit(1);
}
if (res.signal) {
  process.kill(process.pid, res.signal);
}
process.exit(res.status === null ? 1 : res.status);
