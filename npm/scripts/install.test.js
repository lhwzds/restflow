#!/usr/bin/env node

const assert = require("assert");
const { computeSha256, parseChecksums } = require("./install");

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

function main() {
  testComputeSha256();
  testParseChecksums();
  console.log("install.js tests passed");
}

main();
