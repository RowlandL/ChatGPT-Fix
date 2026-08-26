#!/usr/bin/env node
// inject-native-token-cost.js
//
// Injects the Codex Live Token Cost userscript into a ChatGPT/Codex app.asar
// COPY (launcher-owned baseline). The official WindowsApps package is never
// touched. Usage:
//
//   node inject-native-token-cost.js <app.asar> <userscript.js> <out.asar> [<unpacked-dir>]
//
// Notes:
//   - Native modules live in app.asar.unpacked NEXT TO the asar. When given,
//     <unpacked-dir> is copied beside the extracted tree so extractAll does
//     not fail on missing unpacked files. If omitted and the asar references
//     unpacked files, extraction falls back to a per-entry extract.
//   - Steps: extract (with unpacked support) -> add resources/native-token-cost
//     -> prepend a document-start loader into the main entry -> repack.
//   - Fail-closed: any step error exits non-zero.
//
// Renderer-efficiency patch: the upstream userscript registers a
// whole-document MutationObserver that re-queries the DOM
// (project-context-row selector) on every mutation with only a 50 ms
// debounce. On long conversations (large DOM, constant streaming mutations,
// software rendering) this saturates the renderer main thread and the UI
// degrades until it freezes. The loader applies two behavior-preserving
// adjustments to the userscript source before embedding it:
//
//   1. debounce 50 ms -> 300 ms (same semantics, far fewer syncs);
//   2. skip the expensive full-document query while the HUD is hidden
//      (the query result only matters when the hub is visible).
//
// The patched userscript is used only at runtime for this launcher-owned
// copy; the upstream file is never modified on disk. The patch is
// fail-closed: if the exact source patterns are absent (upstream changed),
// the injection refuses to continue rather than silently shipping a
// possibly-wrong patch.

const PATCHES = [
  [
    // v1.0.4 (plan): replace the whole scheduler with an rAF-batched,
    // 500 ms-throttled version. Upstream fired on every document mutation
    // (50 ms debounce); even 300 ms still saturates the renderer during
    // streaming. rAF merges same-frame mutations; the 500 ms floor keeps
    // the full-document sync from hammering layout.
    "function scheduleHubVisibilitySync(delay = 50) {\n    if (state.hubVisibilityTimer) return;\n    state.hubVisibilityTimer = window.setTimeout(() => {\n      state.hubVisibilityTimer = 0;\n      syncHubVisibility();\n    }, delay);\n  }",
    "function scheduleHubVisibilitySync(delay = 300) {\n    if (state.hubVisibilityRaf) return;\n    state.hubVisibilityRaf = window.requestAnimationFrame(() => {\n      state.hubVisibilityRaf = 0;\n      const now = Date.now();\n      if (!state.hubVisibilityLastSyncAt || now - state.hubVisibilityLastSyncAt >= 500) {\n        state.hubVisibilityLastSyncAt = now;\n        syncHubVisibility();\n        return;\n      }\n      if (state.hubVisibilityTimer) return;\n      state.hubVisibilityTimer = window.setTimeout(() => {\n        state.hubVisibilityTimer = 0;\n        state.hubVisibilityLastSyncAt = Date.now();\n        syncHubVisibility();\n      }, Math.max(0, 500 - (now - state.hubVisibilityLastSyncAt)));\n    });\n  }",
    "scheduleHubVisibilitySync -> rAF batch + 500ms throttle",
  ],
  [
    "const projectContextRow = hasCodexProjectContextRow(doc);",
    "const projectContextRow = hubVisible() ? hasCodexProjectContextRow(doc) : false;",
    "skip full-document query while HUD hidden",
  ],
  [
    // v1.0.4 (plan 2): avoid toggling HUD display when visibility did not
    // change. syncHubVisibility flips display (none <-> "") on every call;
    // during thread switches the DOM rebuild re-runs it, and the display
    // flip changes layout, which re-triggers the desktop app's own
    // ResizeObserver -> layout loop on 26.814+/26.818 builds. Cache the last
    // visibility and skip all DOM writes when it is unchanged.
    "const target = root?.style ? root : state.root;\n    if (!target?.style) return visible;",
    "const target = root?.style ? root : state.root;\n    if (!target?.style) return visible;\n    if (state.hubVisibilityLastVisible === visible) return visible;\n    state.hubVisibilityLastVisible = visible;",
    "syncHubVisibility state cache (skip DOM writes when visibility unchanged)",
  ],
];

function patchUserscriptForRendererEfficiency(source) {
  for (const [from, to, label] of PATCHES) {
    if (!source.includes(from)) {
      fail("userscript efficiency patch pattern not found: " + label);
    }
  }
  let patched = source;
  for (const [from, to] of PATCHES) {
    patched = patched.split(from).join(to);
  }
  return patched;
}

const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const asar = require("@electron/asar");

function sha256(p) {
  return crypto.createHash("sha256").update(fs.readFileSync(p)).digest("hex").toUpperCase();
}

function fail(msg) {
  console.error("inject_failed: " + msg);
  process.exit(3);
}

function main() {
  const [inAsar, userscript, outAsar, unpackedDir] = process.argv.slice(2);
  if (!inAsar || !userscript || !outAsar) {
    console.error(
      "Usage: inject-native-token-cost.js <app.asar> <userscript.js> <out.asar> [<unpacked-dir>]"
    );
    process.exit(2);
  }
  if (!fs.existsSync(inAsar)) fail("input app.asar not found: " + inAsar);
  if (!fs.existsSync(userscript)) fail("userscript not found: " + userscript);
  const before = sha256(inAsar);

  const work = fs.mkdtempSync(path.join(os.tmpdir(), "ntc-inject-"));
  try {
    // 1a. If an unpacked dir is provided, place it next to the extraction so
    //     extractAll can resolve the native modules. The runtime asar is named
    //     "app.asar" (the launcher-owned baseline copy), so Electron resolves
    //     native modules from a sibling "<asar>.unpacked" whose internal
    //     reference prefix is "app.asar.unpacked". We must therefore copy the
    //     unpacked dir as <work>/app.asar.unpacked (FIXED name), NOT as
    //     <work>/<input-src-name>.unpacked — using the latter makes the asar
    //     header carry a bogus "app.asar.bak-no-ntc.unpacked/<...>" prefix and
    //     the app then fails to load better-sqlite3.
    if (unpackedDir && fs.existsSync(unpackedDir)) {
      const dstUnpacked = path.join(work, "app.asar.unpacked");
      fs.cpSync(unpackedDir, dstUnpacked, { recursive: true });
    }

    // 1b. Extract. extractAll throws when an unpacked file is missing and no
    //     unpacked dir was provided; in that case fall back to per-entry
    //     extraction that skips unpacked entries.
    let extracted = true;
    try {
      asar.extractAll(inAsar, work);
    } catch (e) {
      extracted = false;
    }
    if (!extracted) {
      let count = 0;
      for (const entry of asar.listPackage(inAsar)) {
        const clean = entry.replace(/^\\+/, "");
        if (!clean) continue;
        try {
          const data = asar.extractFile(inAsar, clean);
          if (data === undefined || data === null) continue;
          const outPath = path.join(work, clean);
          fs.mkdirSync(path.dirname(outPath), { recursive: true });
          fs.writeFileSync(outPath, data);
          count++;
        } catch (_) {
          // unpacked/missing entry -> skip
        }
      }
      if (count === 0) fail("asar extraction produced no files");
    }

    // 2. Place the userscript under resources/native-token-cost/.
    const resDir = path.join(work, "resources", "native-token-cost");
    fs.mkdirSync(resDir, { recursive: true });
    const dst = path.join(resDir, "codex-live-token-cost.js");
    fs.copyFileSync(userscript, dst);

    // 3. Preload loader: prepend to the renderer's first script.
    const pkg = JSON.parse(
      fs.readFileSync(path.join(work, "package.json"), "utf8")
    );
    const mainEntry = pkg.main || ".vite/build/early-bootstrap.js";
    const entryPath = path.join(work, mainEntry);
    if (!fs.existsSync(entryPath)) fail("main entry not found: " + mainEntry);
    // Inline the userscript source and execute it DIRECTLY via
    // contents.executeJavaScript: the page CSP forbids inline <script>
    // (script-src 'self' 'sha256-...' 'wasm-unsafe-eval', no unsafe-inline),
    // which blocks any s.textContent approach. executeJavaScript is a
    // privileged main-process API and bypasses CSP. The userscript is an
    // IIFE that calls scheduleStart() itself, so no host is needed.
    const userscriptContent = patchUserscriptForRendererEfficiency(
      fs.readFileSync(userscript, "utf8")
    );
    const loader = [
      "// NTC-NATIVE-20260801 loader (launcher-owned overlay)",
      "try {",
      "  console.error('[ntc] loader loaded in main process');",
      "  const { app, webContents } = require('electron');",
      "  const ntcSource = " + JSON.stringify(userscriptContent) + ";",
      "  function attachNtcToContents(contents) {",
      "    if (!contents || contents.__chatgptFixNtcAttached) return;",
      "    contents.__chatgptFixNtcAttached = true;",
      "    let injected = false;",
      "    const inject = () => {",
      "      if (injected || contents.isDestroyed()) return;",
      "      injected = true;",
      "      contents.executeJavaScript(ntcSource, true).catch((error) => {",
      "        injected = false;",
      "        console.error('[ntc] execute failed', error);",
      "      });",
      "    };",
      "    contents.once('dom-ready', inject);",
      "    contents.once('did-finish-load', inject);",
      "    if (!contents.isLoading()) setTimeout(inject, 0);",
      "  }",
      "  app.on('web-contents-created', (_event, contents) => attachNtcToContents(contents));",
      "  app.whenReady().then(() => {",
      "    for (const contents of webContents.getAllWebContents()) attachNtcToContents(contents);",
      "  });",
      "} catch (e) { console.error('[ntc] loader failed', e); }",
      "",
    ].join("\n");
    const original = fs.readFileSync(entryPath, "utf8");
    // Idempotent re-injection: if a previous run already prepended our NTC
    // loader, strip it (from the leading marker through its closing
    // "} catch (e) {...}" line) so we never stack two loaders / two
    // ntcSource copies on top of each other. ntc-reapply may re-run on the
    // same baseline after updating the userscript or PATCHES.
    const LOADER_MARK = "// NTC-NATIVE-20260801 loader";
    if (original.includes(LOADER_MARK)) {
      const markStart = original.indexOf(LOADER_MARK);
      const firstLineEnd = original.indexOf("\n", markStart);
      const loaderEnd = original.indexOf(
        "console.error('[ntc] loader failed', e); }",
        markStart
      );
      const cutTo = loaderEnd >= 0 ? loaderEnd + "console.error('[ntc] loader failed', e); }".length : firstLineEnd;
      const stripped = original.slice(0, markStart) + original.slice(cutTo);
      fs.writeFileSync(entryPath, loader + stripped);
    } else {
      fs.writeFileSync(entryPath, loader + original);
    }

    // 4. Repack (async). The temp dir must stay alive until the promise
    //    settles; cleanup happens in the promise handlers, NOT in a finally
    //    that runs before createPackage finishes.
    const cleanup = () => {
      if (!process.env.NTC_KEEP_TMP) {
        fs.rmSync(work, { recursive: true, force: true });
      }
    };
    // Repack with native-module preservation (v1.0.4 plan 2, verified):
    // plain createPackage PACKS every file into the asar body, including
    // better-sqlite3.node and other node-pty/@serialport natives. That is
    // actually CORRECT for the launcher-owned baseline: the app loads native
    // modules from the asar body (and the sibling app.asar.unpacked on disk
    // carries the originals), so no unpack glob is needed and no native is
    // lost. Verified: extracting a baseline asar and repacking with {}
    // produces a working app.asar (better-sqlite3 loads, tslib resolves).
    // A broad unpack glob like "**/node_modules/**" is WRONG: it also unpacks
    // tslib and other pure-JS deps, and the app then fails to resolve them.
    asar
      .createPackageWithOptions(work, outAsar, {})
      .then(() => {
        const after = sha256(outAsar);
        console.log(
          JSON.stringify({
            schema: "chatgpt_fix.ntc_inject_receipt.v1",
            ok: true,
            before_sha256: before,
            after_sha256: after,
            artifact: path.basename(inAsar),
            generated_by: "inject-native-token-cost.js",
          })
        );
        cleanup();
      })
      .catch((e) => {
        cleanup();
        fail("repack failed: " + e.message);
      });
  } catch (e) {
    if (!process.env.NTC_KEEP_TMP) {
      try {
        fs.rmSync(work, { recursive: true, force: true });
      } catch (_) {}
    }
    fail("inject failed: " + (e && e.message || e));
  }
}

main();
