#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER_TAG = /^v\d+\.\d+\.\d+$/;
const RELEASE_LITERAL = /\bv\d+\.\d+\.\d+\b/g;
const RELEASE_URL_LITERAL = /releases\/(?:tag|download)\/v\d+\.\d+\.\d+/g;
const REQUIRED_RELEASE_FILES = [
  "README.md",
  "VALIDATOR_GUIDE.md",
  "docs/VALIDATOR_WINDOWS_TUTORIAL.md",
  "website/src/lib/releases.ts",
];

function fail(message) {
  console.error(`::error::${message}`);
  process.exitCode = 1;
}

const expectedTag = process.argv[2] ?? process.env.RELEASE_TAG ?? process.env.GITHUB_REF_NAME ?? "";
if (!SEMVER_TAG.test(expectedTag)) {
  fail(`Expected a release tag in vMAJOR.MINOR.PATCH format; received ${expectedTag || "<empty>"}`);
  process.exit();
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const releaseSourcePath = resolve(root, "website/src/lib/releases.ts");
const releaseSource = readFileSync(releaseSourcePath, "utf8");
const releaseTagMatch = releaseSource.match(
  /export const RELEASE_TAG\s*=\s*["'](v\d+\.\d+\.\d+)["']/,
);

if (!releaseTagMatch) {
  fail("website/src/lib/releases.ts does not declare a parseable RELEASE_TAG constant");
} else if (releaseTagMatch[1] !== expectedTag) {
  fail(
    `Release tag mismatch: CI expects ${expectedTag}, but website/src/lib/releases.ts defines ${releaseTagMatch[1]}`,
  );
}

let trackedFiles;
try {
  trackedFiles = execFileSync("git", ["ls-files", "-z"], {
    cwd: root,
    encoding: "utf8",
  })
    .split("\0")
    .filter(Boolean);
} catch (error) {
  fail(`Unable to enumerate tracked files: ${error.message}`);
  process.exit();
}

const releaseReferences = [];
for (const relativePath of trackedFiles) {
  const absolutePath = resolve(root, relativePath);
  let contents;
  try {
    contents = readFileSync(absolutePath);
  } catch (error) {
    fail(`Unable to read tracked file ${relativePath}: ${error.message}`);
    continue;
  }

  if (contents.includes(0)) {
    continue;
  }

  const text = contents.toString("utf8");
  for (const match of text.matchAll(RELEASE_LITERAL)) {
    releaseReferences.push({ path: relativePath, value: match[0], index: match.index });
  }

  for (const match of text.matchAll(RELEASE_URL_LITERAL)) {
    const value = match[0].match(SEMVER_TAG)?.[0];
    if (value) {
      releaseReferences.push({ path: relativePath, value, index: match.index, url: true });
    }
  }
}

for (const requiredPath of REQUIRED_RELEASE_FILES) {
  const hasExpectedReference = releaseReferences.some(
    (reference) => reference.path === requiredPath && reference.value === expectedTag,
  );
  if (!hasExpectedReference) {
    fail(`${requiredPath} does not contain the expected release tag ${expectedTag}`);
  }
}

const mismatches = releaseReferences.filter((reference) => reference.value !== expectedTag);
for (const mismatch of mismatches) {
  fail(
    `${mismatch.path} contains ${mismatch.value} instead of ${expectedTag}${mismatch.url ? " in a release URL" : ""}`,
  );
}

if (process.exitCode) {
  process.exit();
}

console.log(`Release version consistency check passed for ${expectedTag}.`);
console.log(`Checked ${trackedFiles.length} tracked files and ${releaseReferences.length} release references.`);
