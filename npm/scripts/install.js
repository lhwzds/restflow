#!/usr/bin/env node

const https = require("https");
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { execSync } = require("child_process");

const VERSION = require("../package.json").version;
const REPO = "lhwzds/restflow";

const PLATFORM_MAP = {
  darwin: {
    x64: "x86_64-apple-darwin",
    arm64: "aarch64-apple-darwin",
  },
  linux: {
    x64: "x86_64-unknown-linux-gnu",
    arm64: "aarch64-unknown-linux-gnu",
  },
  win32: {
    x64: "x86_64-pc-windows-msvc",
  },
};

function getPlatformTarget() {
  const platform = process.platform;
  const arch = process.arch;

  const targets = PLATFORM_MAP[platform];
  if (!targets) {
    throw new Error(`Unsupported platform: ${platform}`);
  }

  const target = targets[arch];
  if (!target) {
    throw new Error(`Unsupported architecture: ${arch} on ${platform}`);
  }

  return target;
}

function getDownloadUrl(target) {
  const ext = process.platform === "win32" ? "zip" : "tar.gz";
  return `https://github.com/${REPO}/releases/download/cli-v${VERSION}/restflow-${target}.${ext}`;
}

function getChecksumUrl() {
  return `https://github.com/${REPO}/releases/download/cli-v${VERSION}/checksums.txt`;
}

function download(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (response) => {
        if (response.statusCode === 302 || response.statusCode === 301) {
          download(response.headers.location).then(resolve).catch(reject);
          return;
        }

        if (response.statusCode !== 200) {
          reject(new Error(`Failed to download ${url}: ${response.statusCode}`));
          return;
        }

        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("end", () => resolve(Buffer.concat(chunks)));
        response.on("error", reject);
      })
      .on("error", reject);
  });
}

function computeSha256(buffer) {
  return crypto.createHash("sha256").update(buffer).digest("hex");
}

function parseChecksums(text) {
  const map = new Map();
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const parts = trimmed.split(/\s+/);
    if (parts.length < 2) continue;
    const hash = parts[0];
    const filename = parts[1].replace(/^\*?/, "");
    map.set(filename, hash);
  }
  return map;
}

async function verifyChecksum(buffer, filename) {
  const checksumUrl = getChecksumUrl();
  const checksumText = (await download(checksumUrl)).toString("utf8");
  const checksums = parseChecksums(checksumText);
  const expected = checksums.get(filename);
  if (!expected) {
    throw new Error(`Checksum not found for ${filename}`);
  }
  const actual = computeSha256(buffer);
  if (actual !== expected) {
    throw new Error(`Checksum mismatch for ${filename}`);
  }
}

async function extractTarGz(buffer, destDir) {
  const tmpFile = path.join(destDir, "tmp.tar.gz");
  fs.writeFileSync(tmpFile, buffer);

  try {
    execSync(`tar -xzf "${tmpFile}" -C "${destDir}"`, { stdio: "inherit" });
  } finally {
    fs.unlinkSync(tmpFile);
  }
}

async function extractZip(buffer, destDir) {
  const tmpZip = path.join(destDir, "tmp.zip");
  fs.writeFileSync(tmpZip, buffer);

  try {
    if (process.platform === "win32") {
      execSync(`powershell -command "Expand-Archive -Path '${tmpZip}' -DestinationPath '${destDir}' -Force"`, {
        stdio: "inherit",
      });
    } else {
      execSync(`unzip -o "${tmpZip}" -d "${destDir}"`, { stdio: "inherit" });
    }
  } finally {
    fs.unlinkSync(tmpZip);
  }
}

async function main() {
  try {
    const target = getPlatformTarget();
    const url = getDownloadUrl(target);
    const binDir = path.join(__dirname, "..", "bin");
    const filename = path.basename(url);

    console.log(`Downloading restflow for ${target}...`);
    console.log(`URL: ${url}`);

    const buffer = await download(url);
    await verifyChecksum(buffer, filename);

    if (!fs.existsSync(binDir)) {
      fs.mkdirSync(binDir, { recursive: true });
    }

    if (process.platform === "win32") {
      await extractZip(buffer, binDir);
    } else {
      await extractTarGz(buffer, binDir);
    }

    const binaryName = process.platform === "win32" ? "restflow.exe" : "restflow";
    const binaryPath = path.join(binDir, binaryName);

    if (process.platform !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }

    console.log(`restflow installed successfully!`);
  } catch (error) {
    console.error(`Failed to install restflow: ${error.message}`);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}

module.exports = {
  computeSha256,
  parseChecksums,
};
