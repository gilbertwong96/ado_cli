#!/usr/bin/env node
// Smoke tests for scripts/postinstall.js. Run with `node scripts/test_postinstall.js`.
//
// These tests don't mock the filesystem; they use a temp HOME
// directory and clean up after themselves.

'use strict';

const { execFileSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { test } = require('node:test');
const assert = require('node:assert/strict');

const POSTINSTALL = path.join(__dirname, 'postinstall.js');
const ADO_BIN = process.env.ADO_BIN;

function freshHome() {
  const dir = path.join(
    os.tmpdir(),
    `ado_postinstall_test_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
  );
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

function runPostinstall({ home, shell }) {
  // Run the postinstall in a child process so it doesn't mutate
  // our test process's CliMate state.
  return execFileSync(
    process.execPath,
    [POSTINSTALL],
    {
      env: {
        ...process.env,
        HOME: home,
        SHELL: shell,
        // Explicitly clear the env var that would skip the
        // install (so tests can exercise both code paths).
        ADO_NO_COMPLETION: '',
        ADO_BIN: ADO_BIN || ''
      },
      shell: true
    }
  ).toString();
}

test('detects bash and writes XDG completion file', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    const out = runPostinstall({ home, shell: '/bin/bash' });
    assert.match(out, /detected shell 'bash'/);
    assert.match(out, /wrote completion script to/);
    const path = require('path').join(
      home,
      '.local',
      'share',
      'bash-completion',
      'completions',
      'ado'
    );
    assert.ok(fs.existsSync(path), `Expected ${path} to exist`);
    const content = fs.readFileSync(path, 'utf8');
    assert.match(content, /complete -F _ado_completion ado/);
    assert.match(content, /"/);
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('detects zsh and appends fpath to .zshrc', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    const out = runPostinstall({ home, shell: '/bin/zsh' });
    assert.match(out, /detected shell 'zsh'/);
    const zshrc = fs.readFileSync(path.join(home, '.zshrc'), 'utf8');
    assert.match(zshrc, /fpath=.*\.zsh\/completions/);
    assert.match(zshrc, /ado shell completion/);
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('detects fish and writes standard completion path', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    const out = runPostinstall({ home, shell: '/usr/local/bin/fish' });
    assert.match(out, /detected shell 'fish'/);
    const fishPath = path.join(
      home,
      '.config',
      'fish',
      'completions',
      'ado.fish'
    );
    assert.ok(fs.existsSync(fishPath));
    assert.match(fs.readFileSync(fishPath, 'utf8'), /__fish_use_subcommand/);
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('ADO_NO_COMPLETION=1 skips the install', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    const out = execFileSync(
      process.execPath,
      [POSTINSTALL],
      {
        env: {
          ...process.env,
          HOME: home,
          ADO_NO_COMPLETION: '1',
          ADO_BIN: ADO_BIN || ''
        },
        shell: true
      }
    ).toString();
    assert.match(out, /ADO_NO_COMPLETION=1 set, skipping/);
    // No completion files should be created
    assert.equal(
      fs.existsSync(path.join(home, '.local', 'share', 'bash-completion', 'completions', 'ado')),
      false
    );
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('second run is idempotent (no duplicate config lines)', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    runPostinstall({ home, shell: '/bin/zsh' });
    runPostinstall({ home, shell: '/bin/zsh' });
    const zshrc = fs.readFileSync(path.join(home, '.zshrc'), 'utf8');
    // The marker should appear exactly once
    const matches = zshrc.match(/# ado shell completion/g) || [];
    assert.equal(matches.length, 1, 'config marker should appear exactly once');
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});
