'use strict';

// Tests for the release-artifact resolution in scripts/postinstall.js.
//
// Run from npm/@gilbertwong1996-ado:  node --test
//
// Requiring the postinstall script runs its install path, so opt out of both
// the completion install and the download before loading it.

process.env.ADO_NO_COMPLETION = process.env.ADO_NO_COMPLETION || '1';
process.env.ADO_NO_DOWNLOAD = process.env.ADO_NO_DOWNLOAD || '1';

const assert = require('node:assert/strict');
const { execFileSync } = require('child_process');
const fs = require('fs');
const http = require('http');
const os = require('os');
const path = require('path');
const { test } = require('node:test');

const POSTINSTALL = path.join(__dirname, '..', 'scripts', 'postinstall.js');

const {
  artifactFor,
  artifactUrl,
  binaryName,
  downloadFile,
  extractArchive,
  installPlatformBinary,
  platformBinaryPath
} = require(POSTINSTALL);

function tempDir(label) {
  // Realpath'd so paths compare equal to the ones Node hands out through
  // __dirname / require.resolve (macOS temp dirs are behind symlinks).
  return fs.realpathSync(
    fs.mkdtempSync(path.join(os.tmpdir(), `ado-artifact-test-${label}-`))
  );
}

function hasTar() {
  try {
    execFileSync('tar', ['--version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function hostArtifactSupported() {
  try {
    artifactFor(process.platform, process.arch);
    return true;
  } catch {
    return false;
  }
}

// Builds a release archive in the shape cargo-dist ships: one top-level
// `ado-<target>/` directory holding the binary.
function makeArchive({ dir, file, target, binary, payload, mode }) {
  const staging = path.join(dir, `staging-${target}`);
  fs.mkdirSync(path.join(staging, target), { recursive: true });
  fs.writeFileSync(path.join(staging, target, binary), payload, { mode });
  fs.writeFileSync(path.join(staging, target, 'CHANGELOG.md'), 'changelog\n');

  const archive = path.join(dir, file);
  const tarArgs = file.endsWith('.zip')
    ? ['--format=zip', '-cf', archive, '-C', staging, target]
    : ['-czf', archive, '-C', staging, target];
  execFileSync('tar', tarArgs);
  return archive;
}

// ── artifact resolution ──────────────────────────────────────────────

test('darwin arm64 -> ado-aarch64-apple-darwin.tar.gz', () => {
  assert.deepEqual(artifactFor('darwin', 'arm64'), {
    file: 'ado-aarch64-apple-darwin.tar.gz',
    target: 'ado-aarch64-apple-darwin'
  });
});

test('darwin x64 -> ado-x86_64-apple-darwin.tar.gz', () => {
  assert.deepEqual(artifactFor('darwin', 'x64'), {
    file: 'ado-x86_64-apple-darwin.tar.gz',
    target: 'ado-x86_64-apple-darwin'
  });
});

test('linux x64 -> ado-x86_64-unknown-linux-musl.tar.gz', () => {
  assert.deepEqual(artifactFor('linux', 'x64'), {
    file: 'ado-x86_64-unknown-linux-musl.tar.gz',
    target: 'ado-x86_64-unknown-linux-musl'
  });
});

test('linux arm64 -> ado-aarch64-unknown-linux-musl.tar.gz', () => {
  assert.deepEqual(artifactFor('linux', 'arm64'), {
    file: 'ado-aarch64-unknown-linux-musl.tar.gz',
    target: 'ado-aarch64-unknown-linux-musl'
  });
});

test('win32 x64 -> ado-x86_64-pc-windows-msvc.zip', () => {
  assert.deepEqual(artifactFor('win32', 'x64'), {
    file: 'ado-x86_64-pc-windows-msvc.zip',
    target: 'ado-x86_64-pc-windows-msvc'
  });
});

test('unknown platform throws with a readable message', () => {
  assert.throws(
    () => artifactFor('freebsd', 'x64'),
    /no ado release artifact for freebsd\/x64 \(supported: .*win32-x64\)/
  );
});

test('unknown arch throws with a readable message', () => {
  assert.throws(
    () => artifactFor('win32', 'arm64'),
    /no ado release artifact for win32\/arm64/
  );
});

test('the mapping only knows the five R60 platform/arch pairs', () => {
  const pairs = [
    ['darwin', 'arm64'],
    ['darwin', 'x64'],
    ['linux', 'arm64'],
    ['linux', 'x64'],
    ['win32', 'x64']
  ];
  for (const [platform, arch] of pairs) {
    const { file } = artifactFor(platform, arch);
    assert.ok(file.endsWith('.tar.gz') || file.endsWith('.zip'), file);
  }
  assert.equal(artifactFor('darwin', 'arm64').file.includes('-macos-'), false);
  assert.equal(artifactFor('darwin', 'x64').file.includes('-macos-'), false);
});

// ── release URL ──────────────────────────────────────────────────────

test('the release URL points at the v<version> tag', () => {
  assert.equal(
    artifactUrl('1.0.0-rc.0', 'linux', 'x64'),
    'https://github.com/gilbertwong96/ado_cli/releases/download/v1.0.0-rc.0/ado-x86_64-unknown-linux-musl.tar.gz'
  );
});

test('the release URL requires a version', () => {
  assert.throws(() => artifactUrl('', 'darwin', 'arm64'), /version/);
});

test('the release URL rejects an unsupported platform', () => {
  assert.throws(
    () => artifactUrl('1.0.0', 'freebsd', 'x64'),
    /no ado release artifact for freebsd\/x64/
  );
});

test('platformBinaryPath resolves the repo staging package', () => {
  // The package copy in the repo is not an npm install: its platform packages
  // are the npm/ directories scripts/npm-publish.sh populates.
  assert.equal(
    platformBinaryPath('darwin', 'arm64'),
    path.join(
      __dirname,
      '..',
      '..',
      '@gilbertwong1996-ado-darwin-arm64',
      'bin',
      'ado'
    )
  );
  assert.equal(
    platformBinaryPath('win32', 'x64'),
    path.join(
      __dirname,
      '..',
      '..',
      '@gilbertwong1996-ado-win32-x64',
      'bin',
      'ado.exe'
    )
  );
});

// ── --dry-run ────────────────────────────────────────────────────────

test(
  '--dry-run prints the resolved URL and exits 0 without downloading',
  { skip: !hostArtifactSupported() },
  () => {
    const stdout = execFileSync(process.execPath, [POSTINSTALL, '--dry-run'], {
      encoding: 'utf8',
      env: { ...process.env, ADO_NO_DOWNLOAD: '' }
    });

    const { file, target } = artifactFor(process.platform, process.arch);
    assert.match(stdout, /dry run/);
    assert.ok(stdout.includes(file), `expected ${file} in:\n${stdout}`);
    assert.ok(stdout.includes(target), `expected ${target} in:\n${stdout}`);
    assert.ok(
      stdout.includes(platformBinaryPath(process.platform, process.arch)),
      `expected the extraction destination in:\n${stdout}`
    );
    assert.doesNotMatch(stdout, /^ado: downloaded /m);
  }
);

// ── download and extraction ──────────────────────────────────────────

test('downloadFile writes the response body to disk', async () => {
  const dir = tempDir('download');
  const server = http.createServer((req, res) => {
    res.writeHead(200, { 'content-type': 'application/octet-stream' });
    res.end('archive-bytes');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

  try {
    const dest = path.join(dir, 'ado-x86_64-apple-darwin.tar.gz');
    await downloadFile(
      `http://127.0.0.1:${server.address().port}/ado-x86_64-apple-darwin.tar.gz`,
      dest
    );
    assert.equal(fs.readFileSync(dest, 'utf8'), 'archive-bytes');
  } finally {
    server.close();
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('downloadFile fails on a non-2xx response', async () => {
  const dir = tempDir('download-404');
  const server = http.createServer((req, res) => {
    res.writeHead(404);
    res.end('not found');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

  try {
    await assert.rejects(
      downloadFile(
        `http://127.0.0.1:${server.address().port}/missing.tar.gz`,
        path.join(dir, 'missing.tar.gz')
      ),
      /HTTP 404/
    );
  } finally {
    server.close();
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('downloadFile gives up on a stalled response', async () => {
  const dir = tempDir('download-stall');
  const server = http.createServer(() => {
    // Never responds: the timeout must end the fetch.
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

  try {
    await assert.rejects(
      downloadFile(
        `http://127.0.0.1:${server.address().port}/stalled.tar.gz`,
        path.join(dir, 'stalled.tar.gz'),
        50
      ),
      /timeout|abort/i
    );
  } finally {
    server.closeAllConnections();
    server.close();
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test(
  'extractArchive unpacks ado-<target>/ado from a .tar.gz',
  { skip: !hasTar() },
  () => {
    const dir = tempDir('tar');
    try {
      const { file, target } = artifactFor('darwin', 'arm64');
      const archive = makeArchive({
        dir,
        file,
        target,
        binary: 'ado',
        payload: '#!/bin/sh\necho ado\n',
        mode: 0o755
      });

      const out = path.join(dir, 'out');
      extractArchive(archive, out);

      const binary = path.join(out, target, 'ado');
      assert.ok(fs.existsSync(binary), `expected ${binary} to exist`);
      assert.equal(fs.readFileSync(binary, 'utf8'), '#!/bin/sh\necho ado\n');
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }
);

test('extractArchive unpacks ado-<target>/ado.exe from a .zip', (t) => {
  const dir = tempDir('zip');
  try {
    const { file, target } = artifactFor('win32', 'x64');
    let archive;
    try {
      archive = makeArchive({
        dir,
        file,
        target,
        binary: 'ado.exe',
        payload: 'MZ fake pe\n'
      });
    } catch {
      t.skip('tar on this host cannot create zip archives');
      return;
    }

    const out = path.join(dir, 'out');
    extractArchive(archive, out);

    const binary = path.join(out, target, 'ado.exe');
    assert.ok(fs.existsSync(binary), `expected ${binary} to exist`);
    assert.equal(fs.readFileSync(binary, 'utf8'), 'MZ fake pe\n');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test(
  'installPlatformBinary installs the binary the launcher resolves',
  async (t) => {
    if (!hasTar()) {
      t.skip('tar is not available');
      return;
    }
    if (!hostArtifactSupported()) {
      t.skip(`no artifact for ${process.platform}/${process.arch}`);
      return;
    }

    const platform = process.platform;
    const dest = platformBinaryPath(platform, process.arch);
    if (fs.existsSync(dest)) {
      t.skip('the platform binary is already present in the checkout');
      return;
    }

    const dir = tempDir('install');
    const { file, target } = artifactFor(platform, process.arch);
    const payload = 'native-binary\n';
    let archive;
    try {
      archive = makeArchive({
        dir,
        file,
        target,
        binary: binaryName(platform),
        payload,
        mode: 0o644 // not executable: the installer must set the mode itself
      });
    } catch {
      t.skip('tar on this host cannot create the archive');
      fs.rmSync(dir, { recursive: true, force: true });
      return;
    }

    const bytes = fs.readFileSync(archive);
    const server = http.createServer((req, res) => {
      res.writeHead(200);
      res.end(bytes);
    });
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

    const realFetch = globalThis.fetch;
    let requestedUrl;

    try {
      // Serve the release archive locally: the URL the installer builds is
      // asserted below, only its host is redirected.
      globalThis.fetch = (url, options) => {
        requestedUrl = url;
        return realFetch(
          String(url).replace(
            /^https:\/\/github\.com\/[^/]+\/[^/]+\/releases\/download\//,
            `http://127.0.0.1:${server.address().port}/`
          ),
          options
        );
      };

      const result = await installPlatformBinary('1.0.0-rc.0');

      assert.equal(
        requestedUrl,
        artifactUrl('1.0.0-rc.0', platform, process.arch)
      );
      assert.equal(result.url, requestedUrl);
      assert.equal(result.dest, dest);
      assert.equal(fs.readFileSync(dest, 'utf8'), payload);
      if (platform !== 'win32') {
        assert.equal(fs.statSync(dest).mode & 0o777, 0o755);
      }
    } finally {
      globalThis.fetch = realFetch;
      server.close();
      fs.rmSync(dest, { force: true });
      try {
        fs.rmdirSync(path.dirname(dest));
      } catch {
        // bin/ is not empty or already gone; both are fine.
      }
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }
);

// ── install-path guards ──────────────────────────────────────────────

function rootCopyPath() {
  const rootCopy = path.join(
    __dirname,
    '..',
    '..',
    '..',
    'scripts',
    'postinstall.js'
  );
  return fs.existsSync(rootCopy) ? rootCopy : null;
}

test('the repo-root copy resolves the same artifact and never fetches', (t) => {
  const rootCopy = rootCopyPath();
  if (!rootCopy) {
    t.skip('not running from the repo checkout');
    return;
  }

  const stdout = execFileSync(process.execPath, [rootCopy, '--dry-run'], {
    encoding: 'utf8',
    // The flag must not be what stops the fetch.
    env: { ...process.env, ADO_NO_DOWNLOAD: '' }
  });

  const { file, target } = artifactFor(process.platform, process.arch);
  assert.ok(stdout.includes(file), `expected ${file} in:\n${stdout}`);
  assert.ok(stdout.includes(target), `expected ${target} in:\n${stdout}`);
  assert.ok(
    stdout.includes(platformBinaryPath(process.platform, process.arch)),
    `expected the npm staging path in:\n${stdout}`
  );
  assert.doesNotMatch(stdout, /^ado: downloaded /m);
});

test('the repo copies skip the fetch in place', (t) => {
  const rootCopy = rootCopyPath();
  if (!rootCopy) {
    t.skip('not running from the repo checkout');
    return;
  }

  // Both repo copies stage their platform packages in npm/ and must never
  // reach the network, with or without ADO_NO_DOWNLOAD.
  for (const copy of [rootCopy, POSTINSTALL]) {
    const stdout = execFileSync(process.execPath, [copy], {
      encoding: 'utf8',
      env: {
        ...process.env,
        ADO_NO_COMPLETION: '1',
        ADO_NO_DOWNLOAD: '',
        ADO_BIN: ''
      }
    });

    assert.match(stdout, /running from the source tree, skipping the platform/);
    assert.doesNotMatch(stdout, /^ado: downloaded /m);
  }
});

// ── installed layout ─────────────────────────────────────────────────

// Lays out node_modules/@gilbertwong1996/ado the way npm installs it, with the
// real manifest, launcher and postinstall inside.
function simulatedInstallLayout(label) {
  const root = tempDir(label);
  const packageRoot = path.join(
    root,
    'node_modules',
    '@gilbertwong1996',
    'ado'
  );
  fs.mkdirSync(path.join(packageRoot, 'scripts'), { recursive: true });
  fs.mkdirSync(path.join(packageRoot, 'bin'), { recursive: true });
  fs.copyFileSync(
    POSTINSTALL,
    path.join(packageRoot, 'scripts', 'postinstall.js')
  );
  fs.copyFileSync(
    path.join(__dirname, '..', 'package.json'),
    path.join(packageRoot, 'package.json')
  );
  fs.copyFileSync(
    path.join(__dirname, '..', 'bin', 'ado'),
    path.join(packageRoot, 'bin', 'ado')
  );
  return { root, packageRoot };
}

// The launcher's lookup, as bin/ado performs it.
function launcherResolves(packageRoot, platform, binary) {
  return fs.realpathSync(
    require.resolve(
      `@gilbertwong1996/ado-${platform}-${process.arch}/bin/${binary}`,
      { paths: [packageRoot] }
    )
  );
}

test(
  'the installed layout installs where the launcher resolves',
  async (t) => {
    if (!hasTar()) {
      t.skip('tar is not available');
      return;
    }
    if (!hostArtifactSupported()) {
      t.skip(`no artifact for ${process.platform}/${process.arch}`);
      return;
    }

    const platform = process.platform;
    const binary = binaryName(platform);
    const { root, packageRoot } = simulatedInstallLayout('layout');
    const sim = require(path.join(packageRoot, 'scripts', 'postinstall.js'));

    try {
      // 1. No platform package installed (--omit=optional): the destination is
      // the hoisted path npm would have used — the scope directory beside
      // this package, never a scoped-looking name inside it.
      const dest = sim.platformBinaryPath(platform, process.arch);
      assert.equal(
        dest,
        path.join(
          root,
          'node_modules',
          '@gilbertwong1996',
          `ado-${platform}-${process.arch}`,
          'bin',
          binary
        )
      );

      // 2. Installed there, the launcher's own lookup finds it — with no
      // package.json beside it, exactly as the fetch leaves it.
      fs.mkdirSync(path.dirname(dest), { recursive: true });
      fs.writeFileSync(dest, 'stub-binary\n');
      assert.equal(launcherResolves(packageRoot, platform, binary), dest);

      // 3. With the binary present the postinstall fetches nothing.
      const stdout = execFileSync(
        process.execPath,
        [path.join(packageRoot, 'scripts', 'postinstall.js')],
        {
          encoding: 'utf8',
          env: { ...process.env, ADO_NO_COMPLETION: '1', ADO_NO_DOWNLOAD: '' }
        }
      );
      assert.doesNotMatch(stdout, /^ado: downloaded /m);
      assert.doesNotMatch(stdout, /could not download the platform binary/);

      // 4. The fetch itself lands on that same path.
      const payload = 'fetched-binary\n';
      const { file, target } = artifactFor(platform, process.arch);
      const archive = makeArchive({
        dir: root,
        file,
        target,
        binary,
        payload,
        mode: 0o644
      });
      const bytes = fs.readFileSync(archive);
      const server = http.createServer((req, res) => {
        res.writeHead(200);
        res.end(bytes);
      });
      await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

      const realFetch = globalThis.fetch;
      try {
        globalThis.fetch = (url, options) =>
          realFetch(
            String(url).replace(
              /^https:\/\/github\.com\/[^/]+\/[^/]+\/releases\/download\//,
              `http://127.0.0.1:${server.address().port}/`
            ),
            options
          );

        const result = await sim.installPlatformBinary('1.0.0-rc.0');

        assert.equal(result.dest, dest);
        assert.equal(launcherResolves(packageRoot, platform, binary), dest);
        assert.equal(fs.readFileSync(dest, 'utf8'), payload);
        if (platform !== 'win32') {
          assert.equal(fs.statSync(dest).mode & 0o777, 0o755);
        }
      } finally {
        globalThis.fetch = realFetch;
        server.close();
      }

      // 5. ADO_NO_DOWNLOAD=1 opts out when the binary is gone again.
      fs.rmSync(dest, { force: true });
      const optedOut = execFileSync(
        process.execPath,
        [path.join(packageRoot, 'scripts', 'postinstall.js')],
        {
          encoding: 'utf8',
          env: { ...process.env, ADO_NO_COMPLETION: '1', ADO_NO_DOWNLOAD: '1' }
        }
      );
      assert.match(optedOut, /ADO_NO_DOWNLOAD=1 set, skipping/);
      assert.doesNotMatch(optedOut, /^ado: downloaded /m);
      assert.equal(fs.existsSync(dest), false);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  }
);

// ── repo invariant ───────────────────────────────────────────────────

test('the repo-root postinstall copy matches the published one', (t) => {
  const rootCopy = rootCopyPath();
  if (!rootCopy) {
    t.skip('not running from the repo checkout');
    return;
  }
  // scripts/npm-publish.sh copies the repo-root script into this package
  // before publishing, so the two files must stay identical.
  assert.equal(
    fs.readFileSync(rootCopy, 'utf8'),
    fs.readFileSync(POSTINSTALL, 'utf8')
  );
});
