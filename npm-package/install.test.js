"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const install = require("./install");

test("getReleaseAssetInfo maps linux x64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("linux", "x64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "zorai-linux-x86_64.zip",
    checksumName: "SHA256SUMS-linux-x86_64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
    pluginsArchiveRoot: "plugins",
    requiredBinaries: [
      "zorai",
      "zorai-daemon",
      "zorai-tui",
      "zorai-gateway",
      "zorai-mcp",
      "zorai-desktop",
    ],
    requiredAssets: [],
  });
});

test("getReleaseAssetInfo maps linux arm64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("linux", "arm64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "zorai-linux-aarch64.zip",
    checksumName: "SHA256SUMS-linux-aarch64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
    pluginsArchiveRoot: "plugins",
    requiredBinaries: [
      "zorai",
      "zorai-daemon",
      "zorai-tui",
      "zorai-gateway",
      "zorai-mcp",
      "zorai-desktop",
    ],
    requiredAssets: [],
  });
});

test("getReleaseAssetInfo maps macOS targets to full desktop npm installs", function () {
  for (const arch of ["x64", "arm64"]) {
    const info = install.getReleaseAssetInfo("darwin", arch, "0.2.0");

    assert.ok(info, "expected macOS " + arch + " release info");
    assert.ok(
      info.requiredBinaries.includes("zorai-desktop"),
      "macOS " + arch + " should install a desktop launcher"
    );
    assert.deepEqual(info.requiredAssets, ["zorai-desktop.app.zip"]);
  }
});

test("getReleaseAssetInfo maps windows x64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("win32", "x64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "zorai-windows-x64.zip",
    checksumName: "SHA256SUMS-windows-x64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
    pluginsArchiveRoot: "plugins",
    requiredBinaries: [
      "zorai.exe",
      "zorai-daemon.exe",
      "zorai-tui.exe",
      "zorai-gateway.exe",
      "zorai-mcp.exe",
      "zorai-desktop.exe",
    ],
    requiredAssets: [],
  });
});

test("getReleaseAssetInfo maps windows arm64 to published zip asset names", function () {
  const info = install.getReleaseAssetInfo("win32", "arm64", "0.2.0");

  assert.deepEqual(info, {
    archiveName: "zorai-windows-arm64.zip",
    checksumName: "SHA256SUMS-windows-arm64.txt",
    bundleChecksumName: "SHA256SUMS.txt",
    skillsArchiveRoot: "skills",
    guidelinesArchiveRoot: "guidelines",
    pluginsArchiveRoot: "plugins",
    requiredBinaries: [
      "zorai.exe",
      "zorai-daemon.exe",
      "zorai-tui.exe",
      "zorai-gateway.exe",
      "zorai-mcp.exe",
      "zorai-desktop.exe",
    ],
    requiredAssets: [],
  });
});

test("getReleaseAssetInfo returns null for unsupported targets", function () {
  assert.equal(install.getReleaseAssetInfo("linux", "ppc64"), null);
});

test("npm package exposes zoi as an alias for the zorai launcher", function () {
  const pkg = require("./package.json");

  assert.equal(pkg.bin.zorai, "bin/zorai.js");
  assert.equal(pkg.bin.zoi, "bin/zorai.js");
});

test("getInstallUsageHint recommends npx for local installs", function () {
  assert.equal(
    install.getInstallUsageHint(false),
    "zor-ai: run with 'npx zor-ai --help' (or 'npm exec zor-ai -- --help') after a local install"
  );
});

test("getInstallUsageHint recommends zorai for global installs", function () {
  assert.equal(
    install.getInstallUsageHint(true),
    "zorai: run 'zorai --help' once your npm global bin directory is on PATH, and open a new shell if it is not recognized immediately"
  );
});

test("getGlobalBinDir appends bin on unix platforms", function () {
  assert.equal(
    install.getGlobalBinDir("/opt/homebrew", "darwin"),
    "/opt/homebrew/bin"
  );
  assert.equal(install.getGlobalBinDir("/usr/local", "linux"), "/usr/local/bin");
});

test("getInstallUsageHint includes explicit global bin directory when known", function () {
  assert.equal(
    install.getInstallUsageHint(true, "/opt/homebrew/bin"),
    "zorai: if 'zorai' is not found, add '/opt/homebrew/bin' to PATH, then open a new shell and run 'zorai --help'"
  );
});

test("prependDirectoryToPath prefixes PATH values", function () {
  const updated = install.prependDirectoryToPath(
    { PATH: "/usr/bin" },
    "/tmp/zorai-bin"
  );

  assert.equal(updated.PATH, "/tmp/zorai-bin:/usr/bin");
});

test("prependDirectoryToPath preserves existing PATH key casing", function () {
  const updated = install.prependDirectoryToPath(
    { Path: "C:\\Windows\\System32" },
    "C:\\zorai\\bin"
  );

  const expected = ["C:\\zorai\\bin", "C:\\Windows\\System32"].join(path.delimiter);
  assert.equal(updated.Path, expected);
  assert.equal(updated.PATH, expected);
});

test("getRuntimeZoraiRoot uses home on unix hosts", function () {
  assert.equal(
    install.getRuntimeZoraiRoot("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.zorai"
  );
});

test("getRuntimeZoraiRoot uses LOCALAPPDATA on windows hosts", function () {
  assert.equal(
    install.getRuntimeZoraiRoot("win32", {
      LOCALAPPDATA: "C:\\Users\\aline\\AppData\\Local",
    }),
    "C:\\Users\\aline\\AppData\\Local\\zorai"
  );
});

test("migrateLegacyTamuxRoot renames .tamux to .zorai on unix when zorai root is absent", function () {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-tamux-migrate-"));
  const legacyRoot = path.join(home, ".tamux");
  const nextRoot = path.join(home, ".zorai");
  fs.mkdirSync(legacyRoot, { recursive: true });
  fs.writeFileSync(path.join(legacyRoot, "state.txt"), "legacy state\n");

  const result = install.migrateLegacyTamuxRoot("linux", { HOME: home });

  assert.equal(result, nextRoot);
  assert.equal(fs.existsSync(legacyRoot), false);
  assert.equal(fs.readFileSync(path.join(nextRoot, "state.txt"), "utf8"), "legacy state\n");
});

test("migrateLegacyTamuxRoot leaves .tamux in place when .zorai already exists", function () {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-tamux-keep-"));
  const legacyRoot = path.join(home, ".tamux");
  const nextRoot = path.join(home, ".zorai");
  fs.mkdirSync(legacyRoot, { recursive: true });
  fs.mkdirSync(nextRoot, { recursive: true });
  fs.writeFileSync(path.join(legacyRoot, "state.txt"), "legacy state\n");
  fs.writeFileSync(path.join(nextRoot, "state.txt"), "zorai state\n");

  const result = install.migrateLegacyTamuxRoot("linux", { HOME: home });

  assert.equal(result, nextRoot);
  assert.equal(fs.readFileSync(path.join(legacyRoot, "state.txt"), "utf8"), "legacy state\n");
  assert.equal(fs.readFileSync(path.join(nextRoot, "state.txt"), "utf8"), "zorai state\n");
});

test("migrateLegacyTamuxRoot renames LOCALAPPDATA tamux root on windows", function () {
  const localAppData = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-tamux-win-"));
  const legacyRoot = path.join(localAppData, "tamux");
  const nextRoot = path.join(localAppData, "zorai");
  fs.mkdirSync(legacyRoot, { recursive: true });
  fs.writeFileSync(path.join(legacyRoot, "state.txt"), "legacy windows state\n");

  const result = install.migrateLegacyTamuxRoot("win32", { LOCALAPPDATA: localAppData });

  assert.equal(result, nextRoot);
  assert.equal(fs.existsSync(legacyRoot), false);
  assert.equal(fs.readFileSync(path.join(nextRoot, "state.txt"), "utf8"), "legacy windows state\n");
});

test("getRuntimeSkillsDir resolves to the canonical zorai skills root", function () {
  assert.equal(
    install.getRuntimeSkillsDir("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.zorai/skills"
  );
});

test("getRuntimeGuidelinesDir resolves beside the canonical zorai skills root", function () {
  assert.equal(
    install.getRuntimeGuidelinesDir("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.zorai/guidelines"
  );

  assert.equal(
    install.getRuntimeGuidelinesDir("win32", {
      LOCALAPPDATA: "C:\\Users\\aline\\AppData\\Local",
    }),
    "C:\\Users\\aline\\AppData\\Local\\zorai\\guidelines"
  );
});

test("getRuntimeCustomAuthPath resolves beside daemon runtime data", function () {
  assert.equal(
    install.getRuntimeCustomAuthPath("linux", {
      HOME: "/home/aline",
    }),
    "/home/aline/.zorai/custom-auth.yaml"
  );

  assert.equal(
    install.getRuntimeCustomAuthPath("win32", {
      LOCALAPPDATA: "C:\\Users\\aline\\AppData\\Local",
    }),
    "C:\\Users\\aline\\AppData\\Local\\zorai\\custom-auth.yaml"
  );
});

test("ensureCustomAuthTemplate creates default yaml without overwriting", function () {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-custom-auth-"));
  const customAuthPath = install.getRuntimeCustomAuthPath("linux", { HOME: root });

  install.ensureCustomAuthTemplate("linux", { HOME: root });

  assert.equal(
    fs.readFileSync(customAuthPath, "utf8"),
    "# Add named custom providers here. The daemon reloads this file before\n" +
      "# provider/model setup in the TUI and desktop app.\n" +
      "# Prefer api_key_env for secrets, for example:\n" +
      "# providers:\n" +
      "#   - id: local-openai\n" +
      "#     name: Local OpenAI-Compatible\n" +
      "#     default_base_url: http://127.0.0.1:11434/v1\n" +
      "#     default_model: llama3.3\n" +
      "#     api_key_env: LOCAL_OPENAI_API_KEY\n" +
      "#     models:\n" +
      "#       - id: llama3.3\n" +
      "#         name: Llama 3.3\n" +
      "#         context_window: 128000\n" +
      "providers: []\n"
  );

  fs.writeFileSync(customAuthPath, "providers:\n  - id: already-here\n");
  install.ensureCustomAuthTemplate("linux", { HOME: root });

  assert.equal(
    fs.readFileSync(customAuthPath, "utf8"),
    "providers:\n  - id: already-here\n"
  );
});

test("release bundle metadata includes bundled skills payload", function () {
  const info = install.getReleaseAssetInfo("linux", "x64", "0.2.0");

  assert.equal(info.skillsArchiveRoot, "skills");
  assert.equal(info.guidelinesArchiveRoot, "guidelines");
});

test("extractBundledGuidelines copies missing defaults without overwriting user files", function () {
  const AdmZip = require("adm-zip");
  const zip = new AdmZip();
  zip.addFile("guidelines/coding-task.md", Buffer.from("# bundled coding\n"));
  zip.addFile("guidelines/research-task.md", Buffer.from("# bundled research\n"));

  const root = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-guidelines-"));
  const guidelinesDir = path.join(root, "guidelines");
  fs.mkdirSync(guidelinesDir, { recursive: true });
  fs.writeFileSync(path.join(guidelinesDir, "coding-task.md"), "# user coding\n");

  install.extractBundledGuidelines(
    zip.toBuffer(),
    { guidelinesArchiveRoot: "guidelines" },
    guidelinesDir
  );

  assert.equal(
    fs.readFileSync(path.join(guidelinesDir, "coding-task.md"), "utf8"),
    "# user coding\n"
  );
  assert.equal(
    fs.readFileSync(path.join(guidelinesDir, "research-task.md"), "utf8"),
    "# bundled research\n"
  );
});

test("stageBundledPlugins replaces the staged bundle so plugins dropped from the app disappear", function () {
  const AdmZip = require("adm-zip");
  const zip = new AdmZip();
  zip.addFile("plugins/zorai-plugin-voice-lab/voice-lab/plugin.json", Buffer.from('{"name":"voice-lab"}'));
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-bundle-"));
  const bundle = path.join(root, "bundled-plugins");
  fs.mkdirSync(path.join(bundle, "zorai-plugin-retired"), { recursive: true });
  fs.writeFileSync(path.join(bundle, "zorai-plugin-retired", "plugin.json"), "{}");

  assert.equal(install.stageBundledPlugins(zip.toBuffer(), { pluginsArchiveRoot: "plugins" }, bundle), true);
  assert.ok(fs.existsSync(path.join(bundle, "zorai-plugin-voice-lab", "voice-lab", "plugin.json")));
  assert.ok(!fs.existsSync(path.join(bundle, "zorai-plugin-retired")));
});

test("stageBundledPlugins leaves the staged bundle alone when an older release has no plugins", function () {
  const AdmZip = require("adm-zip");
  const zip = new AdmZip();
  zip.addFile("skills/a.md", Buffer.from("a"));
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-bundle-"));
  const bundle = path.join(root, "bundled-plugins");
  fs.mkdirSync(path.join(bundle, "kept"), { recursive: true });

  assert.equal(install.stageBundledPlugins(zip.toBuffer(), { pluginsArchiveRoot: "plugins" }, bundle), false);
  assert.ok(fs.existsSync(path.join(bundle, "kept")));
});

test("bundled plugins are staged where the daemon looks, which on Windows is the user profile, not LOCALAPPDATA", function () {
  assert.equal(install.getBundledPluginsDir("linux", { HOME: "/home/me" }), "/home/me/.zorai/bundled-plugins");
  assert.equal(install.getBundledPluginsDir("win32", { USERPROFILE: "C:\\Users\\me", LOCALAPPDATA: "C:\\Users\\me\\AppData\\Local" }), "C:\\Users\\me\\.zorai\\bundled-plugins");
  assert.equal(install.getBundledPluginsDir("linux", { HOME: "/home/me", ZORAI_BUNDLED_PLUGINS_DIR: "/opt/zorai-plugins" }), "/opt/zorai-plugins");
});

test("installMacosDesktopApp expands bundled app zip during npm install", function () {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-macos-app-"));
  const appZip = path.join(root, "zorai-desktop.app.zip");
  fs.writeFileSync(appZip, "zip payload\n");
  const calls = [];

  install.installMacosDesktopApp("darwin", root, {
    spawnSync: function (command, args) {
      calls.push([command, args]);
      const appBinary = path.join(root, "zorai-desktop.app", "Contents", "MacOS", "zorai");
      fs.mkdirSync(path.dirname(appBinary), { recursive: true });
      fs.writeFileSync(appBinary, "desktop binary\n");
      return { status: 0 };
    },
  });

  assert.deepEqual(calls, [["ditto", ["-x", "-k", appZip, root]]]);
});

test("installMacosDesktopApp skips non-macOS installs", function () {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "zorai-nonmac-app-"));
  let called = false;

  install.installMacosDesktopApp("linux", root, {
    spawnSync: function () {
      called = true;
      return { status: 0 };
    },
  });

  assert.equal(called, false);
});

test("global npm install stops processes before replacing binaries and restarts daemon after success", async function () {
  const events = [];

  await install.maybeRefreshDaemonAfterInstall(
    {
      isGlobalInstall: true,
      platform: "linux",
      binDir: "/tmp/zorai-bin",
    },
    async function () {
      events.push("install");
    },
    {
      stopProcesses: async function (platform) {
        events.push("stop:" + platform);
      },
      startDaemon: function (platform, binDir) {
        events.push("start:" + platform + ":" + binDir);
        return path.join(binDir, "zorai-daemon");
      },
    }
  );

  assert.deepEqual(events, [
    "stop:linux",
    "install",
    "start:linux:/tmp/zorai-bin",
  ]);
});

test("local npm install does not stop or restart daemon", async function () {
  const events = [];

  await install.maybeRefreshDaemonAfterInstall(
    {
      isGlobalInstall: false,
      platform: "linux",
      binDir: "/tmp/zorai-bin",
    },
    async function () {
      events.push("install");
    },
    {
      stopProcesses: async function () {
        events.push("stop");
      },
      startDaemon: function () {
        events.push("start");
      },
    }
  );

  assert.deepEqual(events, ["install"]);
});
