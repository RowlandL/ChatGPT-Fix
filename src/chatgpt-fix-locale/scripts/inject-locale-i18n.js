#!/usr/bin/env node
// inject-locale-i18n.js
//
// Persists the Chinese UI independently of the network. The desktop app
// decides whether to translate its UI from a Statsig layer
// (layer 72216192, keys: enable_i18n / locale_source) fetched from
// chatgpt.com. When that fetch is impossible (offline / blocked network)
// the cached layer expires and the UI silently falls back to English.
//
// This script patches the app's own renderer bundle inside the launcher-owned
// app.asar COPY so the `enable_i18n` flag defaults to TRUE instead of FALSE:
// with the flag enabled the app applies translations from the locale
// override (`[desktop] localeOverride = "zh-CN"`) or the system locale,
// with no server round-trip. The official WindowsApps package is never
// touched. Usage:
//
//   node inject-locale-i18n.js check <app.asar>
//   node inject-locale-i18n.js fix <app.asar> <out.asar> [<unpacked-dir>]
//
// Fail-closed: any step error exits non-zero.

const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const asar = require("@electron/asar");

const PATCH_FROM = "get(`enable_i18n`,!1)";
const PATCH_TO = "get(`enable_i18n`,!0)";
// Renderer entry bundles live under webview\assets\app-initial-*.js; the
// exact hash suffix varies per app build, so match by prefix.
const ENTRY_RE = /^webview[\\/]assets[\\/]app-initial-.*\.js$/;

function sha256(p) {
  return crypto.createHash("sha256").update(fs.readFileSync(p)).digest("hex").toUpperCase();
}

function fail(msg) {
  console.error("locale_inject_failed: " + msg);
  process.exit(3);
}

function findEntry(entries) {
  const matches = entries
    .map((e) => e.replace(/^\\+/, ""))
    .filter((e) => ENTRY_RE.test(e));
  if (matches.length === 0) return null;
  return matches[0];
}

function readEntry(asarPath, entry) {
  const data = asar.extractFile(asarPath, entry);
  if (data === undefined || data === null) {
    fail("cannot extract renderer entry: " + entry);
  }
  return data.toString("utf8");
}

function entryStatus(text) {
  if (text.includes(PATCH_TO)) return "patched";
  if (text.includes(PATCH_FROM)) return "needs_patch";
  return "pattern_not_found";
}

function main() {
  const [mode, inAsar, outAsar, unpackedDir] = process.argv.slice(2);
  if (!mode || !inAsar) {
    console.error(
      "Usage: inject-locale-i18n.js check <app.asar> | fix <app.asar> <out.asar> [<unpacked-dir>]"
    );
    process.exit(2);
  }
  if (!fs.existsSync(inAsar)) fail("input app.asar not found: " + inAsar);

  const entries = asar.listPackage(inAsar);
  const entry = findEntry(entries);
  if (!entry) fail("renderer entry (webview/assets/app-initial-*.js) not found in archive");

  if (mode === "check") {
    const text = readEntry(inAsar, entry);
    const status = entryStatus(text);
    console.log(
      JSON.stringify({
        schema: "chatgpt_fix.locale_status.v1",
        entry,
        status,
        patch_from: PATCH_FROM,
        patch_to: PATCH_TO,
      })
    );
    // Always exit 0: the caller decides based on the `status` field.
    process.exit(0);
  }

  if (mode !== "fix") {
    console.error("unknown mode: " + mode);
    process.exit(2);
  }
  if (!outAsar) fail("fix requires <out.asar>");

  const before = sha256(inAsar);
  const work = fs.mkdtempSync(path.join(os.tmpdir(), "locale-inject-"));

  const cleanup = () => {
    if (!process.env.LOCALE_KEEP_TMP) {
      fs.rmSync(work, { recursive: true, force: true });
    }
  };

  try {
    if (unpackedDir && fs.existsSync(unpackedDir)) {
      const dstUnpacked = path.join(work, path.basename(inAsar) + ".unpacked");
      fs.cpSync(unpackedDir, dstUnpacked, { recursive: true });
    }

    let extracted = true;
    try {
      asar.extractAll(inAsar, work);
    } catch (_e) {
      extracted = false;
    }
    if (!extracted) {
      let count = 0;
      for (const raw of entries) {
        const clean = raw.replace(/^\\+/, "");
        if (!clean) continue;
        try {
          const data = asar.extractFile(inAsar, clean);
          if (data === undefined || data === null) continue;
          const outPath = path.join(work, clean);
          fs.mkdirSync(path.dirname(outPath), { recursive: true });
          fs.writeFileSync(outPath, data);
          count++;
        } catch (_e) {
          // unpacked/missing entry -> skip
        }
      }
      if (count === 0) fail("asar extraction produced no files");
    }

    const entryPath = path.join(work, entry);
    if (!fs.existsSync(entryPath)) fail("entry missing after extraction: " + entry);
    const original = fs.readFileSync(entryPath, "utf8");
    const status = entryStatus(original);
    if (status === "patched") {
      fail("renderer entry is already patched; nothing to do");
    }
    if (status === "pattern_not_found") {
      fail(
        "patch pattern not found in renderer entry " +
          entry +
          " (app version may have changed the bundle shape)"
      );
    }
    // Same-length replacement keeps the archive layout valid. Replace every
    // occurrence (the minifier may emit duplicate function copies).
    const patched = original.split(PATCH_FROM).join(PATCH_TO);
    if (!patched.includes(PATCH_TO)) fail("patch application failed");
    fs.writeFileSync(entryPath, patched);

    asar
      .createPackage(work, outAsar)
      .then(() => {
        const after = sha256(outAsar);
        console.log(
          JSON.stringify({
            schema: "chatgpt_fix.locale_inject_receipt.v1",
            ok: true,
            before_sha256: before,
            after_sha256: after,
            artifact: path.basename(inAsar),
            entry,
            generated_by: "inject-locale-i18n.js",
          })
        );
        cleanup();
      })
      .catch((e) => {
        cleanup();
        fail("repack failed: " + e.message);
      });
  } catch (e) {
    cleanup();
    fail("inject failed: " + ((e && e.message) || e));
  }
}

main();
