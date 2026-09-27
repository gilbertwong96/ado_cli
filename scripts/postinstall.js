#!/usr/bin/env node
// ado npm postinstall — fetches the platform binary, then auto-installs
// (and uninstalls) shell completion.
//
// Three modes, selected by argv or env:
//   * Default:  install completion after `npm install -g`, fetching the
//               release archive first when the optional platform package
//               did not provide a binary
//   * --uninstall:  remove completion when triggered by
//                   `npm uninstall -g @gilbertwong1996/ado`
//   * --dry-run:  print the resolved release URL and the extraction
//                 destination, then exit without downloading anything
//
// Install mode: detects the user's shell, generates the right
// completion script, and installs it to the standard auto-load
// location for that shell (e.g. ~/.config/fish/completions/ado.fish,
// ~/.zsh/completions/_ado, ~/.local/share/bash-completion/completions/ado).
// For shells that need an explicit fpath/source line in the user's
// shell config (zsh, powershell), appends the line to the config file.
//
// Uninstall mode: reverses the install — deletes the completion
// file and removes the appended lines from the user's shell config
// (using the same marker line that the install wrote). Idempotent
// and safe to run even if the install was never done.
//
// Honors the ADO_NO_COMPLETION env var to opt out of install:
//   ADO_NO_COMPLETION=1 npm install -g @gilbertwong1996/ado
// and ADO_NO_DOWNLOAD to skip the release-archive download:
//   ADO_NO_DOWNLOAD=1 npm install -g @gilbertwong1996/ado
//
// Idempotent: re-running the install just refreshes the completion
// script. Skips re-appending the config line if it's already there.
//
// Note: stdout from the postinstall is shown by npm to the user
// (unless --silent). We use stdout for the success messages and
// stderr for warnings/errors, so the user always sees what was done.

'use strict';

const { execFileSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

// ── Release artifacts ────────────────────────────────────────────────
//
// cargo-dist attaches one archive per target to the `v<version>` GitHub
// release. The normal delivery path is the optional platform package; the
// download below only runs when that package is missing (--omit=optional,
// a libc npm considers mismatched, a skipped optional download), and it
// unpacks with the system `tar` so the package keeps zero runtime deps.

const RELEASE_REPO = 'gilbertwong96/ado_cli';
const RELEASE_DOWNLOAD = `https://github.com/${RELEASE_REPO}/releases/download`;

// platform-arch → the asset names cargo-dist produces (`target` is the
// directory the archive unpacks to). Five targets, no alternates.
const ARTIFACTS = {
  'darwin-arm64': {
    file: 'ado-aarch64-apple-darwin.tar.gz',
    target: 'ado-aarch64-apple-darwin'
  },
  'darwin-x64': {
    file: 'ado-x86_64-apple-darwin.tar.gz',
    target: 'ado-x86_64-apple-darwin'
  },
  'linux-arm64': {
    file: 'ado-aarch64-unknown-linux-musl.tar.gz',
    target: 'ado-aarch64-unknown-linux-musl'
  },
  'linux-x64': {
    file: 'ado-x86_64-unknown-linux-musl.tar.gz',
    target: 'ado-x86_64-unknown-linux-musl'
  },
  'win32-x64': {
    file: 'ado-x86_64-pc-windows-msvc.zip',
    target: 'ado-x86_64-pc-windows-msvc'
  }
};

function artifactFor(platform, arch) {
  const artifact = ARTIFACTS[`${platform}-${arch}`];
  if (!artifact) {
    throw new Error(
      `no ado release artifact for ${platform}/${arch} ` +
        `(supported: ${Object.keys(ARTIFACTS).join(', ')})`
    );
  }
  return { ...artifact };
}

function artifactUrl(version, platform, arch) {
  if (!version) {
    throw new Error('a release version is required to build the artifact URL');
  }
  return `${RELEASE_DOWNLOAD}/v${version}/${artifactFor(platform, arch).file}`;
}

function binaryName(platform) {
  return platform === 'win32' ? 'ado.exe' : 'ado';
}

// This package's own directory: …/@gilbertwong1996/ado when installed,
// npm/@gilbertwong1996-ado in the source tree.
const PACKAGE_ROOT = path.join(__dirname, '..');

// Layout detection. An installed copy lives under node_modules — the only
// layout in which the launcher resolves the platform package. The two repo
// copies (npm/@gilbertwong1996-ado, and the repo-root one npm-publish.sh
// installs into it) stage their platform packages in npm/ and must never
// fetch; the repo-root copy has no manifest of its own, so it reads the
// package's.
const IN_NODE_MODULES = PACKAGE_ROOT.split(path.sep).includes('node_modules');
const HAS_OWN_MANIFEST = fs.existsSync(path.join(PACKAGE_ROOT, 'package.json'));
const MANIFEST = HAS_OWN_MANIFEST
  ? path.join(PACKAGE_ROOT, 'package.json')
  : path.join(PACKAGE_ROOT, 'npm', '@gilbertwong1996-ado', 'package.json');
const STAGING_DIR = HAS_OWN_MANIFEST
  ? path.join(PACKAGE_ROOT, '..')
  : path.join(PACKAGE_ROOT, 'npm');

// A stalled connection must not hold `npm install` until undici's own timeouts
// fire; the archives are around 1 MB, so a minute is generous.
const DOWNLOAD_TIMEOUT_MS = 60_000;

function packageVersion() {
  const manifest = JSON.parse(fs.readFileSync(MANIFEST, 'utf8'));
  return manifest.version;
}

function platformPackageName(platform, arch) {
  return `@gilbertwong1996/ado-${platform}-${arch}`;
}

// The platform package as npm installed it — hoisted beside this package or
// nested underneath it — so ask the resolver instead of guessing a directory.
function resolvePlatformPackageDir(platform, arch) {
  try {
    return path.dirname(
      require.resolve(`${platformPackageName(platform, arch)}/package.json`, {
        paths: [PACKAGE_ROOT]
      })
    );
  } catch {
    return null;
  }
}

// The launcher's own lookup (bin/ado:22), used as the presence guard: what
// matters is whether the binary resolves, not whether a path we guessed
// exists. A binary fetched into the hoisted location resolves even though no
// package.json sits beside it.
function resolvedPlatformBinary(platform, arch) {
  try {
    return require.resolve(
      `${platformPackageName(platform, arch)}/bin/${binaryName(platform)}`,
      { paths: [PACKAGE_ROOT] }
    );
  } catch {
    return null;
  }
}

// Where the binary has to land for bin/ado to find it.
function platformBinaryPath(platform, arch) {
  const binary = binaryName(platform);

  if (!IN_NODE_MODULES) {
    // Repo staging layout: scripts/npm-publish.sh puts the published binaries
    // in these directories.
    return path.join(
      STAGING_DIR,
      `@gilbertwong1996-ado-${platform}-${arch}`,
      'bin',
      binary
    );
  }

  // Absent (--omit=optional, a libc npm skipped): npm hoists the platform
  // package beside this one, which is where the fetch puts it.
  const packageDir =
    resolvePlatformPackageDir(platform, arch) ||
    path.join(PACKAGE_ROOT, '..', `ado-${platform}-${arch}`);

  return path.join(packageDir, 'bin', binary);
}

async function downloadFile(url, destPath, timeoutMs = DOWNLOAD_TIMEOUT_MS) {
  const response = await fetch(url, {
    redirect: 'follow',
    signal: AbortSignal.timeout(timeoutMs)
  });
  if (!response.ok) {
    throw new Error(`download failed: HTTP ${response.status} for ${url}`);
  }
  fs.mkdirSync(path.dirname(destPath), { recursive: true });
  fs.writeFileSync(destPath, Buffer.from(await response.arrayBuffer()));
  return destPath;
}

// macOS, Linux and Windows 10+ all ship a tar that reads .tar.gz; the Windows
// tar (bsdtar) also reads the only .zip this script ever asks it to open.
function extractArchive(archivePath, destDir) {
  fs.mkdirSync(destDir, { recursive: true });
  const flags = archivePath.endsWith('.zip') ? '-xf' : '-xzf';
  execFileSync('tar', [flags, archivePath, '-C', destDir], { stdio: 'pipe' });
  return destDir;
}

async function installPlatformBinary(
  version,
  platform = process.platform,
  arch = process.arch
) {
  const dest = platformBinaryPath(platform, arch);
  const { file, target } = artifactFor(platform, arch);
  const url = artifactUrl(version, platform, arch);
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ado-postinstall-'));

  try {
    const archive = path.join(tmpDir, file);
    await downloadFile(url, archive);
    extractArchive(archive, tmpDir);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.copyFileSync(path.join(tmpDir, target, binaryName(platform)), dest);
    if (platform !== 'win32') fs.chmodSync(dest, 0o755);
    return { url, dest };
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

// Set ADO_NO_DOWNLOAD=1 to opt out of the fallback fetch; a failure here must
// never fail the npm install.
async function ensurePlatformBinary() {
  // The guard is the launcher's lookup: if bin/ado would resolve it, the
  // install is complete and nothing is fetched.
  if (resolvedPlatformBinary(process.platform, process.arch)) return;

  const dest = platformBinaryPath(process.platform, process.arch);
  if (fs.existsSync(dest)) return;

  if (!IN_NODE_MODULES) {
    console.log(
      'ado: running from the source tree, skipping the platform binary ' +
        'download.'
    );
    return;
  }

  if (process.env.ADO_NO_DOWNLOAD === '1') {
    console.log(
      'ado: ADO_NO_DOWNLOAD=1 set, skipping the platform binary download.'
    );
    return;
  }

  try {
    const { url } = await installPlatformBinary(packageVersion());
    console.log(`ado: downloaded ${url}`);
    console.log(`ado: installed the platform binary at ${dest}`);
  } catch (err) {
    console.error(
      `ado: could not download the platform binary: ${err.message}`
    );
    console.error(
      '     Install a pre-built binary from ' +
        `https://github.com/${RELEASE_REPO}/releases instead.`
    );
  }
}

function isDryRun() {
  return process.argv.includes('--dry-run');
}

function dryRun() {
  const platform = process.platform;
  const arch = process.arch;
  const { file, target } = artifactFor(platform, arch);

  const url = artifactUrl(packageVersion(), platform, arch);

  console.log(`ado: dry run — platform ${platform}/${arch}`);
  console.log(`ado: dry run — release URL ${url}`);
  console.log(
    `ado: dry run — ${file} unpacks to ${target}/${binaryName(platform)}, ` +
      `installed at ${platformBinaryPath(platform, arch)}`
  );
  console.log('ado: dry run — nothing was downloaded');
}

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

// Remove a block of lines that was previously appended by
// `appendLines/2`. Idempotent: returns `{ skipped: ... }` if the
// file doesn't exist, the marker isn't found, or there's nothing
// to remove.
//
// We look for the marker line and then drop the next (blockSize - 1)
// lines as well, plus the trailing blank line if present. This
// preserves the original spacing in the user's config file.
//
// Args:
//   filePath  — path to the user's shell config (e.g. ~/.zshrc)
//   marker    — the first line of the block to remove
//   blockSize — total number of lines in the block (including the
//               marker). E.g. zsh's block is 3 lines:
//               [marker, 'fpath=...', 'autoload -U compinit ...']
function removeConfigBlock(filePath, marker, blockSize) {
  if (!fs.existsSync(filePath)) {
    return { skipped: true, reason: 'no config file' };
  }

  const original = fs.readFileSync(filePath, 'utf8');
  const lines = original.split('\n');
  const markerIdx = lines.findIndex((l) => l === marker);

  if (markerIdx === -1) {
    return { skipped: true, reason: 'not configured' };
  }

  // Walk backwards from the marker to absorb the leading blank
  // line that `appendLines/2` always inserts before the block
  // (so the block reads as a separate paragraph in the file).
  let startIdx = markerIdx;
  if (startIdx > 0 && lines[startIdx - 1] === '') {
    startIdx -= 1;
  }

  // Walk forwards to absorb the trailing blank line (the
  // `appendLines/2` join ends with '\n', which produces a blank
  // line after the block when split on '\n').
  let endIdx = markerIdx + blockSize; // exclusive
  if (endIdx < lines.length && lines[endIdx] === '') {
    endIdx += 1;
  }

  // Splice out [startIdx, endIdx).
  const newLines = lines.slice(0, startIdx).concat(lines.slice(endIdx));

  // If we removed the last non-blank line of the file, the
  // trailing newline would also be gone and the file would end
  // with a stray blank line. Strip a single trailing blank line
  // if the result ends with one.
  while (newLines.length > 0 && newLines[newLines.length - 1] === '') {
    newLines.pop();
  }
  newLines.push(''); // Re-add the final newline terminator

  fs.writeFileSync(filePath, newLines.join('\n'), 'utf8');
  return { removed: true, path: filePath };
}

// Same, but for the powershell case where the install wrote only
// a single line (`. '<ps1Path>'`) without a marker comment. The
// path is what the line sources, so we reconstruct the expected
// line and remove it if present.
function removeShellSourceLine(filePath, sourceLine) {
  if (!fs.existsSync(filePath)) {
    return { skipped: true, reason: 'no config file' };
  }

  const original = fs.readFileSync(filePath, 'utf8');
  const lines = original.split('\n');
  const lineIdx = lines.findIndex((l) => l === sourceLine);

  if (lineIdx === -1) {
    return { skipped: true, reason: 'not configured' };
  }

  // Same blank-line handling as removeConfigBlock, but for a single
  // line.
  let startIdx = lineIdx;
  if (startIdx > 0 && lines[startIdx - 1] === '') {
    startIdx -= 1;
  }
  let endIdx = lineIdx + 1;
  if (endIdx < lines.length && lines[endIdx] === '') {
    endIdx += 1;
  }

  const newLines = lines.slice(0, startIdx).concat(lines.slice(endIdx));
  while (newLines.length > 0 && newLines[newLines.length - 1] === '') {
    newLines.pop();
  }
  newLines.push('');

  fs.writeFileSync(filePath, newLines.join('\n'), 'utf8');
  return { removed: true, path: filePath };
}

// ── Mode dispatch ────────────────────────────────────────────────────

// npm doesn't pass --uninstall to the script directly, so we also
// honor the npm_lifecycle_event env var. Either signal flips the
// script into uninstall mode.
function isUninstallMode() {
  return (
    process.argv.includes('--uninstall') ||
    process.env.npm_lifecycle_event === 'uninstall'
  );
}

function uninstall() {
  const shell = detectShell();
  const cfg = SHELL_CONFIG[shell];

  if (!cfg) {
    console.log(
      `ado: detected shell '${shell}' — no completion to remove.`
    );
    return;
  }

  console.log(`ado: detected shell '${shell}' (uninstalling completion)`);

  // 1. Delete the completion script file (if any).
  //    We don't fail if it's missing — the user may have
  //    manually deleted it, or ADO_NO_COMPLETION=1 was set at
  //    install time so nothing was ever written.
  try {
    if (fs.existsSync(cfg.installPath)) {
      fs.unlinkSync(cfg.installPath);
      console.log(`ado: removed ${cfg.installPath}`);
    } else {
      console.log(
        `ado: no completion file at ${cfg.installPath} (already gone?)`
      );
    }
  } catch (err) {
    console.error(
      `ado: failed to remove ${cfg.installPath}: ${err.message}`
    );
  }

  // 2. Remove the appended lines from the user's shell config.
  //    We handle both formats:
  //      (a) the zsh multi-line block written by `appendLines/2`
  //          with the marker comment as its first line
  //      (b) the powershell single-line source line (`. '<ps1Path>'`)
  //          written by `appendLine/2` — which the older install
  //          path produced without a marker
  if (cfg.needsConfigEdit && cfg.configPath) {
    if (shell === 'zsh') {
      const result = removeConfigBlock(
        cfg.configPath,
        cfg.configMarker,
        cfg.configLines.length
      );
      if (result.removed) {
        console.log(`ado: removed completion block from ${result.path}`);
      } else {
        console.log(
          `ado: ${cfg.configPath} has no completion block (${result.reason})`
        );
      }
    } else if (shell === 'powershell') {
      // The .ps1 file is gone (step 1), so we just need to drop
      // the `. '<ps1Path>'` line from $PROFILE. We compute the
      // same path string the install would have written.
      const ps1Path = cfg.installPath;
      const sourceLine = `. '${ps1Path.replace(/'/g, "''")}'`;
      const result = removeShellSourceLine(cfg.configPath, sourceLine);
      if (result.removed) {
        console.log(`ado: removed . line from ${result.path}`);
      } else {
        console.log(
          `ado: ${cfg.configPath} has no . line (${result.reason})`
        );
      }
    }
  }

  console.log('');
  console.log('ado: shell completion removed!');
  console.log('     Restart your shell to clear the loaded completion.');
}

// ── Main ─────────────────────────────────────────────────────────────

async function main() {
  if (isDryRun()) {
    return dryRun();
  }

  if (isUninstallMode()) {
    return uninstall();
  }

  // Before the completion install, because generating the completion script
  // runs the binary and ADO_NO_COMPLETION is about the shell config, not about
  // a missing binary.
  await ensurePlatformBinary();

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
        'ado: shell config already has completion loader ' +
          `(${configResult.reason})`
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

if (require.main === module) {
  main().catch((err) => {
    console.error(`ado: postinstall failed: ${err.message}`);
    // Don't fail the install just because completion setup failed.
    // The binary still works; users can run `ado completion <shell>`
    // manually if they want.
    process.exit(0);
  });
}

module.exports = {
  artifactFor,
  artifactUrl,
  binaryName,
  downloadFile,
  extractArchive,
  installPlatformBinary,
  platformBinaryPath
};
