#!/usr/bin/env node

const assert = require("assert");
const { spawnSync } = require("child_process");
const fs = require("fs");
const os = require("os");
const path = require("path");
const {
  computeSha256,
  expectedChecksum,
  getPlatformTarget,
  parseChecksums,
} = require("./install");

function testComputeSha256() {
  const hash = computeSha256(Buffer.from("abc", "utf8"));
  assert.strictEqual(
    hash,
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
}

function testParseChecksums() {
  const text = `
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  restflow-x86_64-apple-darwin.tar.gz
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  restflow-x86_64-pc-windows-msvc.zip
`;
  const map = parseChecksums(text);
  assert.strictEqual(
    map.get("restflow-x86_64-apple-darwin.tar.gz"),
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
  assert.strictEqual(
    map.get("restflow-x86_64-pc-windows-msvc.zip"),
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
}

function testInstallerUsesEmbeddedChecksums() {
  const installer = fs.readFileSync(path.join(__dirname, "install.js"), "utf8");
  assert.doesNotMatch(installer, /checksums\.txt/);
  assert.doesNotMatch(installer, /getChecksumUrl/);
  assert.throws(
    () => expectedChecksum("missing-artifact.tar.gz"),
    /Embedded checksum not found/,
  );
}

function testWindowsArm64FallsBackToX64Artifact() {
  const originalPlatform = Object.getOwnPropertyDescriptor(process, "platform");
  const originalArch = Object.getOwnPropertyDescriptor(process, "arch");
  Object.defineProperty(process, "platform", { value: "win32" });
  Object.defineProperty(process, "arch", { value: "arm64" });
  try {
    assert.strictEqual(getPlatformTarget(), "x86_64-pc-windows-msvc");
  } finally {
    Object.defineProperty(process, "platform", originalPlatform);
    Object.defineProperty(process, "arch", originalArch);
  }
}

function testWrapperFailsClearlyWhenNativeBinaryIsMissing() {
  const wrapper = path.join(__dirname, "..", "bin", "restflow");
  for (const platform of ["linux", "win32"]) {
    const result = spawnSync(process.execPath, [wrapper, "--version"], {
      encoding: "utf8",
      env: {
        ...process.env,
        RESTFLOW_NPM_TEST_PLATFORM: platform,
      },
    });
    assert.notStrictEqual(result.status, 0);
    assert.match(result.stderr, /Native RestFlow binary is not installed/);
  }
}

function testWrapperTreatsSignalExitAsFailure() {
  if (process.platform === "win32") {
    return;
  }

  const root = fs.mkdtempSync(path.join(os.tmpdir(), "restflow-wrapper-"));
  const binDir = path.join(root, "bin");
  fs.mkdirSync(binDir);
  const wrapper = path.join(binDir, "wrapper");
  const native = path.join(binDir, "restflow");
  fs.copyFileSync(path.join(__dirname, "..", "bin", "restflow"), wrapper);
  fs.writeFileSync(
    native,
    '#!/usr/bin/env node\nprocess.kill(process.pid, "SIGTERM");\n',
  );
  fs.chmodSync(native, 0o755);

  try {
    const result = spawnSync(process.execPath, [wrapper], {
      encoding: "utf8",
      env: {
        ...process.env,
        RESTFLOW_NPM_TEST_PLATFORM: "linux",
      },
    });
    assert.strictEqual(result.status, 1);
    assert.match(result.stderr, /terminated by signal SIGTERM/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function testInstallerDoesNotUseShellExtractionCommands() {
  const installer = fs.readFileSync(path.join(__dirname, "install.js"), "utf8");
  assert.doesNotMatch(installer, /\bexecSync\s*\(/);
  assert.match(installer, /execFileSync\("tar", \["-xzf", tmpFile, "-C", destDir\]/);
  assert.match(installer, /execFileSync\("unzip", \["-o", tmpZip, "-d", destDir\]/);
}

function testChecksumVerifierAllowsNpmDryRunBeforeReleaseEmbedding() {
  const verifier = path.join(__dirname, "verify-checksums.js");
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "restflow-checksum-test-"));
  const checksums = path.join(root, "checksums.json");
  fs.writeFileSync(checksums, "{}");

  const result = spawnSync(process.execPath, [verifier], {
    encoding: "utf8",
    env: {
      ...process.env,
      npm_config_dry_run: "true",
      RESTFLOW_CHECKSUMS_PATH: checksums,
    },
  });
  try {
    assert.strictEqual(result.status, 0);
    assert.match(result.stderr, /Skipping embedded checksum verification/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function main() {
  testComputeSha256();
  testParseChecksums();
  testInstallerUsesEmbeddedChecksums();
  testWindowsArm64FallsBackToX64Artifact();
  testWrapperFailsClearlyWhenNativeBinaryIsMissing();
  testWrapperTreatsSignalExitAsFailure();
  testInstallerDoesNotUseShellExtractionCommands();
  testChecksumVerifierAllowsNpmDryRunBeforeReleaseEmbedding();
  console.log("install.js tests passed");
}

main();
