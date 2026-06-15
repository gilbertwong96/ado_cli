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

function runPostinstall({ home, shell, mode, extraEnv }) {
  // Run the postinstall in a child process so it doesn't mutate
  // our test process's CliMate state.
  const args = [POSTINSTALL];
  if (mode === 'uninstall') args.push('--uninstall');

  return execFileSync(
    process.execPath,
    args,
    {
      env: {
        ...process.env,
        HOME: home,
        SHELL: shell || '',
        // Explicitly clear the env var that would skip the
        // install (so tests can exercise both code paths).
        ADO_NO_COMPLETION: '',
        ADO_BIN: ADO_BIN || '',
        ...(extraEnv || {})
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

// ── Uninstall-mode tests ─────────────────────────────────────────────────────────────

test('uninstall removes bash completion file', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    // Install first, then uninstall.
    runPostinstall({ home, shell: '/bin/bash' });
    const file = path.join(
      home,
      '.local',
      'share',
      'bash-completion',
      'completions',
      'ado'
    );
    assert.ok(fs.existsSync(file), 'precondition: bash completion file should exist');

    const out = runPostinstall({ home, shell: '/bin/bash', mode: 'uninstall' });
    assert.match(out, /removed .*bash-completion.*completions\/ado/);
    assert.match(out, /shell completion removed!/);
    assert.equal(
      fs.existsSync(file),
      false,
      'bash completion file should be gone after uninstall'
    );
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('uninstall removes zsh completion file and .zshrc block', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    // Pre-populate .zshrc with some user content so we can verify
    // it survives the uninstall intact.
    const zshrc = path.join(home, '.zshrc');
    fs.writeFileSync(
      zshrc,
      '# user stuff\nexport PATH="$HOME/bin:$PATH"\n',
      'utf8'
    );

    // Install.
    runPostinstall({ home, shell: '/bin/zsh' });
    const zshFile = path.join(home, '.zsh', 'completions', '_ado');
    assert.ok(fs.existsSync(zshFile));
    let zshrcContent = fs.readFileSync(zshrc, 'utf8');
    assert.match(zshrcContent, /# ado shell completion/);
    assert.match(zshrcContent, /fpath=.*\.zsh\/completions/);

    // Uninstall.
    const out = runPostinstall({ home, shell: '/bin/zsh', mode: 'uninstall' });
    assert.match(out, /removed .*_ado/);
    assert.match(out, /removed completion block from/);

    // The completion file is gone.
    assert.equal(fs.existsSync(zshFile), false);
    // The .zshrc no longer has the marker or the fpath line.
    zshrcContent = fs.readFileSync(zshrc, 'utf8');
    assert.equal(
      zshrcContent.match(/# ado shell completion/g),
      null,
      '.zshrc should no longer contain the marker'
    );
    assert.equal(
      zshrcContent.match(/fpath=.*\.zsh\/completions/g),
      null,
      '.zshrc should no longer contain the fpath line'
    );
    // The user's original content is preserved.
    assert.match(zshrcContent, /# user stuff/);
    assert.match(zshrcContent, /export PATH=.*HOME\/bin/);
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('uninstall removes fish completion file', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    runPostinstall({ home, shell: '/usr/local/bin/fish' });
    const fishFile = path.join(home, '.config', 'fish', 'completions', 'ado.fish');
    assert.ok(fs.existsSync(fishFile));

    const out = runPostinstall({
      home,
      shell: '/usr/local/bin/fish',
      mode: 'uninstall'
    });
    assert.match(out, /removed .*ado\.fish/);
    assert.equal(fs.existsSync(fishFile), false);
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('uninstall on a clean HOME is a no-op (no errors)', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    // Nothing was ever installed. Uninstall should still succeed.
    const out = runPostinstall({
      home,
      shell: '/bin/zsh',
      mode: 'uninstall'
    });
    assert.match(out, /no completion file/);
    assert.match(out, /no completion block/);
    assert.match(out, /shell completion removed!/);
    // No .zshrc was created.
    assert.equal(
      fs.existsSync(path.join(home, '.zshrc')),
      false,
      'uninstall on a clean HOME should not create .zshrc'
    );
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});

test('uninstall is idempotent (running twice does not error)', { skip: !ADO_BIN }, () => {
  const home = freshHome();
  try {
    runPostinstall({ home, shell: '/bin/bash' });
    // Uninstall twice — the second run should be a no-op.
    runPostinstall({ home, shell: '/bin/bash', mode: 'uninstall' });
    const out = runPostinstall({
      home,
      shell: '/bin/bash',
      mode: 'uninstall'
    });
    assert.match(out, /already gone/);
    assert.match(out, /shell completion removed!/);
  } finally {
    fs.rmSync(home, { recursive: true, force: true });
  }
});
