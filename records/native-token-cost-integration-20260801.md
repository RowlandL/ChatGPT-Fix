# Native token-cost integration receipt

## Scope

- Integration ID: `NTC-NATIVE-20260801`.
- User-authorized scope: reconnect the native `Tianzora/codex-token-cost` integration to the ChatGPT-Fix task line as a follow-up enablement item.
- This receipt records delegated evidence; it is not a P1 offline-core acceptance artifact and does not authorize repeating live writes, repacking, restarting ChatGPT, or modifying the official WindowsApps package.
- User/profile-bearing paths are normalized to `%USERPROFILE%` and `%LOCALAPPDATA%`; no thread/task UUID or raw `.codex` session data is recorded.

## Delegated evidence

- Upstream: `Tianzora/codex-token-cost`, latest release reported as `v0.7.9`.
- Native target: `%LOCALAPPDATA%\Programs\Codex-NTFS-Fix\baseline\app\ChatGPT.exe`.
- Backup reported at `%USERPROFILE%\.codex\backups\codex-token-cost-native-20260801-074411`.
- Resource root reported at `%LOCALAPPDATA%\Programs\Codex-NTFS-Fix\baseline\app\resources\native-token-cost\`.
- Repacked `app.asar` SHA-256: `C91E4D7B357C226A807299C40013810BF4BDC646087084F38CD5326D8241C2F3`.
- Native userscript SHA-256: `D285A430CD0DFD00D70C3DC44EB3E2DBCF240524C9804D26197080E9E1DB4FA1`.
- Preload marker: `__codexLiveTokenCostNativeLoaderVersion = "0.1.0"`.
- Reported script version: `0.7.9`; price migration: `official-prices-2026-07-31-v4`.
- Reported helper: Scheduled Task `CodexTokenCostHelper`, loopback `127.0.0.1:17888`, health source `codex-local-usage-helper`, bridge `cc-switch`.
- The external session reported that a ChatGPT launch chain started after the `app.asar` rewrite; P1 did not reread or restart that process state.
- Reported static checks: preload/userscript `node --check` PASS; archive extraction found the marker and `native-token-cost/codex-live-token-cost.js`.
- Renderer/UI proof is explicitly incomplete: CDP/debug access was not stable and no DOM/console overlay proof was obtained.

## ChatGPT-Fix routing

1. P1: record this receipt only. P1 remains Rust standard-library-only, offline, and must not reread live package/process/`.codex` state.
2. P2: add an offline contract/proposal for detecting a launcher-owned native overlay and helper receipt. Do not expand the existing A2-P2 official-package selector or run a live probe as part of P1.
3. P3/P4: treat the overlay as an immutable launcher-owned baseline layer with source/resource manifest, app.asar before/after hashes, backup, generation, rollback, and reapply-on-update behavior. Never patch `WindowsApps`.
4. P8: productize the app.asar preload injection, `native-token-cost` resources, helper task health/reapply check, doctor proposal, installer/repair receipts, and UI canary. Any repeat live write, scheduled-task mutation, or restart needs its own bounded phase authorization.

## Acceptance state

- Current state: `DELEGATED_EVIDENCE_RECORDED`.
- Independent live re-read is intentionally deferred while the P1 offline goal is active.
- The delegated evidence is not proof that the renderer overlay is visible; UI canary remains `PENDING`.
- No P1 release/version/ref/Goal state is changed by this receipt.
