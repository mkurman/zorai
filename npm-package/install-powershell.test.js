"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

test("powershell installer uses GitHub release zip assets and full binary set", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.ps1");
  const script = fs.readFileSync(scriptPath, "utf8");

  assert.match(script, /\$GitHubOwner\s*=\s*"mkurman"/);
  assert.match(script, /\$GitHubRepo\s*=\s*"zorai"/);
  assert.match(script, /api\.github\.com\/repos\/\$GitHubOwner\/\$GitHubRepo/);
  assert.match(script, /github\.com\/\$GitHubOwner\/\$GitHubRepo\/releases\/download/);
  assert.match(script, /zorai-windows-\$script:ArchName\.zip/);
  assert.match(script, /SHA256SUMS-windows-\$script:ArchName\.txt/);
  assert.match(script, /zorai-gateway\.exe/);
  assert.match(script, /zorai-mcp\.exe/);
  assert.match(script, /zorai-desktop\.exe/);
  assert.doesNotMatch(script, /gitlab\.com\/api\/v4\/projects/);
  assert.doesNotMatch(script, /zorai-binaries-/);
});

test("powershell installer provisions bundled skills into canonical zorai root", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.ps1");
  const script = fs.readFileSync(scriptPath, "utf8");

  assert.match(script, /\$SkillsDir = if \(\$env:ZORAI_SKILLS_DIR\) \{ \$env:ZORAI_SKILLS_DIR \} else \{ Join-Path \$HOME "\.zorai\\skills" \}/);
  assert.match(script, /\$GuidelinesDir = if \(\$env:ZORAI_GUIDELINES_DIR\) \{ \$env:ZORAI_GUIDELINES_DIR \} else \{ Join-Path \$HOME "\.zorai\\guidelines" \}/);
  assert.match(script, /Extracting binaries, skills, and guidelines/);
  assert.match(script, /\$BundledPluginsDir = if \(\$env:ZORAI_BUNDLED_PLUGINS_DIR\)/);
  assert.match(script, /function Install-BundledPlugins/);
  assert.match(script, /^\s+Install-BundledPlugins$/m);
  assert.match(script, /Copy-Item -Path \(Join-Path \$script:ExtractDir "skills\\\*"\) -Destination \$SkillsDir -Recurse -Force/);
  assert.match(script, /function Install-Guidelines/);
  assert.match(script, /if \(Test-Path \$targetPath\) \{/);
  assert.match(script, /function Install-CliAlias/);
  assert.match(script, /Copy-Item -Path \(Join-Path \$InstallDir "zorai\.exe"\) -Destination \(Join-Path \$InstallDir "zoi\.exe"\) -Force/);
});

test("powershell installer accepts archive-only checksum manifests", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.ps1");
  const script = fs.readFileSync(scriptPath, "utf8");

  assert.match(script, /function Test-FileChecksum/);
  assert.match(script, /\$archiveHash = \$script:Checksums\[\$script:ArchiveName\]/);
  assert.match(script, /if \(\$archiveHash\) \{/);
  assert.match(script, /Test-FileChecksum -Path \$script:ArchivePath -ExpectedHash \$archiveHash -Label \$script:ArchiveName/);
  assert.match(script, /\$script:VerifyExtractedBinaries = \$false/);
  assert.match(script, /if \(\$script:VerifyExtractedBinaries\) \{/);
});

test("powershell installer migrates legacy tamux runtime roots before provisioning data", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.ps1");
  const script = fs.readFileSync(scriptPath, "utf8");

  assert.match(script, /function Migrate-LegacyTamuxRoot/);
  assert.match(script, /Join-Path \$HOME "\.tamux"/);
  assert.match(script, /Join-Path \$HOME "\.zorai"/);
  assert.match(script, /Join-Path \$env:LOCALAPPDATA "tamux"/);
  assert.match(script, /Join-Path \$env:LOCALAPPDATA "zorai"/);
  assert.match(script, /Move-Item -Path \$legacyRoot -Destination \$targetRoot/);
  assert.match(script, /Migrate-LegacyTamuxRoot/);
  assert.match(script, /Install-CustomAuthTemplate/);
});
