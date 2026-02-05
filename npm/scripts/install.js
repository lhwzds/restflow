#!/usr/bin/env node

const https = require("https");
const fs = require("fs");
const path = require("path");
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

function download(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (response) => {
        if (response.statusCode === 302 || response.statusCode === 301) {
          download(response.headers.location).then(resolve).catch(reject);
          return;
        }

        if (response.statusCode !== 200) {
          reject(new Error(`Failed to download: ${response.statusCode}`));
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

    console.log(`Downloading restflow for ${target}...`);
    console.log(`URL: ${url}`);

    const buffer = await download(url);

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

main();
