# ChatGPT-Fix

Local evidence and handoff repository for the Codex Desktop / ChatGPT Desktop mitigation review.

This repository captures the July 2026 local audit of three related failure surfaces:

- Codex Desktop / ChatGPT Desktop child processes, especially MCP/`uv`/Node/Python process stacks, remaining after task activity.
- Codex session and log growth caused by repeated history compaction and inline image/base64 payloads.
- A wrapper-based mitigation strategy that preserves the official OpenAI package while keeping a local patch effective after official updates.

## Repository Map

| Path | Purpose |
|---|---|
| `docs/审查文档.md` | Main review document with findings, evidence, and recommended architecture. |
| `docs/HANDOFF.md` | Current handoff: where the work stopped and what the next session should do. |
| `records/conversation-visible.md` | Redacted visible user/assistant conversation export from the local Codex session. |
| `records/local-audit-snapshot.md` | Local host/process/version/storage snapshot gathered during review. |
| `records/external-references.md` | Public issue and official documentation reference snapshot. |
| `records/external-reference-snapshot.json` | Machine-readable GitHub issue metadata snapshot. |
| `materials/reports/` | Previous bilingual NTFS/nonpaged-pool reports and PDFs. |
| `materials/screenshots/` | Screenshot supplied by the user for the image/base64 storage issue. |
| `materials/installers/` | Local mitigation installer artifact, stored for traceability only. |
| `checksums/SHA256SUMS.txt` | SHA-256 hashes for committed materials. |

## Current Status

The review phase is complete. No installer was executed, no active process was terminated, no logs were cleaned, and no official package files were modified during this archiving pass.

The next useful step is an implementation design/prototype for a wrapper that:

- discovers the latest registered official `OpenAI.Codex` package on each launch;
- prepares an isolated side-by-side runtime copy when the official version changes;
- applies local mitigations only to the isolated runtime or launcher environment;
- adds a child-process watchdog for MCP/`uv`/Node/Python/PowerShell descendants;
- caps or externalizes image-heavy history/log payloads without corrupting active sessions.

