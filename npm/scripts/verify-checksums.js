#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const REQUIRED_ARTIFACTS = [
  "restflow-aarch64-apple-darwin.tar.gz",
  "restflow-x86_64-apple-darwin.tar.gz",
  "restflow-aarch64-unknown-linux-gnu.tar.gz",
  "restflow-x86_64-unknown-linux-gnu.tar.gz",
  "restflow-x86_64-pc-windows-msvc.zip",
];

const checksumsPath =
  process.env.RESTFLOW_CHECKSUMS_PATH ||
  path.join(__dirname, "..", "checksums.json");
const checksums = JSON.parse(fs.readFileSync(checksumsPath, "utf8"));
const dryRun = process.env.npm_config_dry_run === "true";
const missing = [];

for (const artifact of REQUIRED_ARTIFACTS) {
  const checksum = checksums[artifact];
  if (typeof checksum !== "string" || !/^[0-9a-f]{64}$/.test(checksum)) {
    missing.push(artifact);
  }
}

if (missing.length > 0) {
  if (dryRun) {
    console.warn(
      `Skipping embedded checksum verification during npm dry-run; missing ${missing.length} generated checksum(s).`,
    );
    process.exit(0);
  }
  throw new Error(`Missing embedded checksum for ${missing[0]}`);
}
