#!/usr/bin/env node
// ado npm postinstall — auto-installs shell completion.
//
// Runs after `npm install -g @gilbertwong1996/ado`. Detects the
// user's shell, generates the right completion script, and
// installs it to the standard auto-load location for that shell
// (e.g. ~/.config/fish/completions/ado.fish, ~/.zsh/completions/_ado,
// ~/.local/share/bash-completion/completions/ado). For shells that
// need an explicit fpath/source line in the user's shell config
// (zsh, powershell), appends the line to the config file.
//
// Honors the ADO_NO_COMPLETION env var to opt out:
//   ADO_NO_COMPLETION=1 npm install -g @gilbertwong1996/ado
//
// Idempotent: re-running just refreshes the completion script.
// Skips re-appending the config line if it's already there.
//
// Note: stdout from the postinstall is shown by npm to the user
// (unless --silent). We use stdout for the success messages and
// stderr for warnings/errors, so the user always sees what was done.

'use strict';

const { execFileSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

// ── Configuration ────────────────────────────────────────────────────

// Standard auto-load locations for completion scripts per shell.
// All paths are XDG-friendly and don't require sudo.
const HOME = os.homedir();
const PLATFORM = process.platform;

const SHELL_CONFIG = {
  bash: {
    installPath: path.join(
      HOME,
      '.local',
      'share',
      'bash-completion',
      'completions',
      'ado'
    ),
    needsConfigEdit: false,
    // bash-completion's standard location. The bash-completion
    // package (https://github.com/scop/bash-completion) auto-loads
    // any file in this directory. Most package managers install
    // bash-completion by default. If it's not installed, the file
    // just sits there harmlessly.
    autoLoadNote:
      'Works automatically with the bash-completion package. ' +
      'If <TAB> does nothing, install it via your package manager ' +
      '(e.g. `brew install bash-completion`, `apt install bash-completion`).'
  },

  zsh: {
    installPath: path.join(HOME, '.zsh', 'completions', '_ado'),
    needsConfigEdit: true,
    configPath: path.join(HOME, '.zshrc'),
    configMarker: '# ado shell completion (added by npm postinstall)',
    configLines: [
      '# ado shell completion (added by npm postinstall)',
      'fpath=($HOME/.zsh/completions $fpath)',
      'autoload -U compinit && compinit'
    ]
  },

  fish: {
    installPath: path.join(
      HOME,
      '.config',
      'fish',
      'completions',
      'ado.fish'
    ),
    needsConfigEdit: false,
    // Fish auto-loads every file in this directory. Zero config.
    autoLoadNote: 'Works automatically. Restart your fish shell.'
  },

  powershell: {
    installPath:
      PLATFORM === 'win32'
        ? path.join(HOME, 'Documents', 'PowerShell', 'ado-completion.ps1')
        : path.join(HOME, '.config', 'powershell', 'ado-completion.ps1'),
    needsConfigEdit: true,
    configPath:
      PLATFORM === 'win32'
        ? path.join(
            HOME,
            'Documents',
            'PowerShell',
            'Microsoft.PowerShell_profile.ps1'
          )
        : path.join(
            HOME,
            '.config',
            'powershell',
            'Microsoft.PowerShell_profile.ps1'
          ),
    configMarker: '# ado shell completion (added by npm postinstall)',
    configLines: []
  }
};

// ── Shell detection ───────────────────────────────────────────────────

function detectShell() {
  // On Windows, default to PowerShell.
  if (PLATFORM === 'win32') return 'powershell';

  // On Unix, use $SHELL.
  const sh = (process.env.SHELL || '').toLowerCase();
  if (sh.endsWith('/bash') || sh.endsWith('/sh')) return 'bash';
  if (sh.endsWith('/zsh')) return 'zsh';
  if (sh.endsWith('/fish')) return 'fish';
  if (sh.includes('pwsh') || sh.includes('powershell')) return 'powershell';

  // Default to bash if undetectable.
  return 'bash';
}

// ── Completion script generation ─────────────────────────────────────

function findAdoWrapper() {
  // The Node.js wrapper that spawns the platform-specific binary.
  // We try multiple paths because the postinstall runs in
  // different cwd contexts:
  //   1. In an installed npm package:
  //      node_modules/@gilbertwong1996/ado/scripts/postinstall.js
  //      -> bin/ado is at ../bin/ado
  //   2. In the project source tree (for local testing):
  //      scripts/postinstall.js
  //      -> bin/ado is at npm/@gilbertwong1996-ado/bin/ado
  //
  // 3. For end-to-end testing, callers can set ADO_BIN env var
  //    to point at a real binary (e.g. the dev escript at ./ado)
  //    to bypass the wrapper entirely.

  if (process.env.ADO_BIN && fs.existsSync(process.env.ADO_BIN)) {
    return process.env.ADO_BIN;
  }

  // Try the npm-resolvable path first (works when installed)
  try {
    return require.resolve('@gilbertwong1996/ado/bin/ado');
  } catch {
    // Fall through to local source paths
  }

  // Try the local source path (for testing)
  const localPath = path.join(
    __dirname,
    '..',
    'npm',
    '@gilbertwong1996-ado',
    'bin',
    'ado'
  );
  if (fs.existsSync(localPath)) return localPath;

  return null;
}

function generateCompletion(shell) {
  const wrapper = findAdoWrapper();
  if (!wrapper) {
    return {
      ok: false,
      error:
        'Could not find the ado binary. This usually means a ' +
        'broken npm install. Try: npm install -g @gilbertwong1996/ado --force'
    };
  }

  try {
    const stdout = execFileSync(wrapper, ['completion', shell], {
      stdio: ['ignore', 'pipe', 'pipe'],
      encoding: 'utf8'
    });
    return { ok: true, script: stdout };
  } catch (err) {
    return {
      ok: false,
      error: err.stderr || err.message,
      status: err.status
    };
  }
}

// ── Config file editing ──────────────────────────────────────────────

function ensureConfigLine(shell, script, cfg) {
  if (!cfg.needsConfigEdit) {
    return { skipped: true, reason: 'auto-load' };
  }

  const configPath = cfg.configPath;

  if (shell === 'powershell') {
    // For PowerShell, write a separate .ps1 file and source it
    // from $PROFILE. This way updating ado (which rewrites the
    // .ps1) doesn't touch the user's $PROFILE.
    const ps1Path = cfg.installPath;
    fs.mkdirSync(path.dirname(ps1Path), { recursive: true });
    // The .ps1 content IS the completion script (Register-ArgumentCompleter
    // call). The $PROFILE just . -sources it.
    fs.writeFileSync(
      ps1Path,
      "# ado shell completion (auto-generated, do not edit)\n" +
        "# Sourced by $PROFILE on shell startup.\n" +
        "# Re-generated by `npm install -g @gilbertwong1996/ado`.\n" +
        "\n" +
        script
    );
    return appendLine(configPath, `. '${ps1Path.replace(/'/g, "''")}'`);
  }

  if (shell === 'zsh') {
    // For zsh, append the fpath + compinit lines to .zshrc.
    return appendLines(configPath, cfg.configLines);
  }

  return { skipped: true, reason: 'unknown shell' };
}

function appendLine(path, line) {
  return appendLines(path, [line]);
}

function appendLines(filePath, lines) {
  const marker = lines[0]; // First line is the marker
  const content = fs.existsSync(filePath)
    ? fs.readFileSync(filePath, 'utf8')
    : '';

  if (content.includes(marker)) {
    return { skipped: true, reason: 'already configured' };
  }

  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.appendFileSync(
    filePath,
    (content.endsWith('\n') || content === '' ? '' : '\n') +
      '\n' +
      lines.join('\n') +
      '\n'
  );
  return { added: true, path: filePath };
}

// ── Main ─────────────────────────────────────────────────────────────

function main() {
  // Opt-out
  if (process.env.ADO_NO_COMPLETION === '1') {
    console.log(
      'ado: ADO_NO_COMPLETION=1 set, skipping shell completion install.'
    );
    console.log('     To install later, run: ado completion <shell>');
    return;
  }

  const shell = detectShell();
  const cfg = SHELL_CONFIG[shell];

  if (!cfg) {
    console.error(`ado: unsupported shell '${shell}', skipping completion install.`);
    return;
  }

  console.log(`ado: detected shell '${shell}'`);

  // Generate the completion script
  const result = generateCompletion(shell);
  if (!result.ok) {
    console.error(
      `ado: failed to generate completion script: ${result.error}`
    );
    console.error(
      '     You can install manually later with: ado completion ' + shell
    );
    return;
  }

  // Write the script to the install path
  try {
    fs.mkdirSync(path.dirname(cfg.installPath), { recursive: true });
    fs.writeFileSync(cfg.installPath, result.script, 'utf8');
    console.log(`ado: wrote completion script to ${cfg.installPath}`);
  } catch (err) {
    console.error(
      `ado: failed to write completion script to ${cfg.installPath}: ${err.message}`
    );
    return;
  }

  // Edit the user's shell config if needed. PowerShell needs
  // the generated script content too (to write it to a separate
  // .ps1 file that gets sourced from $PROFILE), so we pass the
  // script along.
  if (cfg.needsConfigEdit) {
    const configResult = ensureConfigLine(shell, result.script, cfg);
    if (configResult.added) {
      console.log(
        `ado: added completion loader line to ${configResult.path}`
      );
    } else if (configResult.skipped) {
      console.log(
        `ado: shell config already has completion loader (${configResult.reason})`
      );
    }
  }

  // Final hint
  console.log('');
  console.log('ado: shell completion installed!');
  if (cfg.autoLoadNote) {
    console.log('     ' + cfg.autoLoadNote);
  } else {
    console.log('     Restart your shell, or: source ' + cfg.configPath);
  }
  console.log('     Then press <TAB> after typing `ado ` to see it in action.');
}

try {
  main();
} catch (err) {
  console.error(`ado: postinstall failed: ${err.message}`);
  // Don't fail the install just because completion setup failed.
  // The binary still works; users can run `ado completion <shell>`
  // manually if they want.
  process.exit(0);
}
