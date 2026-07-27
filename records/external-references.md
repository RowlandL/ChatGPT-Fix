# External Reference Snapshot

Generated at: `2026-07-27T22:33:05+08:00`

## GitHub Issues

| Issue | State | Category | Updated | Title |
|---|---|---|---|---|
| [#12491](https://github.com/openai/codex/issues/12491) | open | process-lifecycle | 2026-07-24T15:46:00Z | Codex.app GUI: MCP child processes not reaped after task completion — 1300+ zombies, 37GB memory leak |
| [#15379](https://github.com/openai/codex/issues/15379) | open | process-lifecycle | 2026-05-03T16:06:55Z | Orphaned child processes persist after Codex parent exits |
| [#20980](https://github.com/openai/codex/issues/20980) | closed | process-lifecycle | 2026-05-04T16:10:53Z | Codex App starts a full local MCP process stack per opened chat/thread, causing memory and process growth |
| [#25935](https://github.com/openai/codex/issues/25935) | open | process-lifecycle | 2026-07-22T02:06:12Z | Codex appears to leave orphaned git.exe processes on Windows. |
| [#33913](https://github.com/openai/codex/issues/33913) | open | process-lifecycle | 2026-07-17T22:23:02Z | Windows command execution can hang indefinitely when descendant processes inherit stdout/stderr handles, causing agent turns to become stuck forever |
| [#24948](https://github.com/openai/codex/issues/24948) | open | history-and-image-payload | 2026-07-27T01:45:46Z | Codex session logs grow to 700MB-2GB from repeated compaction history and raw tool output |
| [#23257](https://github.com/openai/codex/issues/23257) | open | history-and-image-payload | 2026-07-26T01:00:41Z | Desktop compaction repeatedly embeds full image base64 in compacted checkpoints |
| [#28531](https://github.com/openai/codex/issues/28531) | open | history-and-image-payload | 2026-07-15T01:08:24Z | Codex Desktop can crash or freeze when opening image-heavy sessions because session JSONL embeds base64 image payloads |
| [#34863](https://github.com/openai/codex/issues/34863) | open | history-and-image-payload | 2026-07-23T02:39:24Z | Codex app-server reaches 27 GB footprint and 36 GB swap after compacted records grow one rollout JSONL to 10.2 GB with repeated inline PNG data URLs |
| [#35458](https://github.com/openai/codex/issues/35458) | open | history-and-image-payload | 2026-07-26T10:07:16Z | Codex Desktop: screenshots re-persisted in full on every compaction and inherited by subagent forks - ~/.codex/sessions reached ~165 GiB (95% base64 images) |

## Official Documentation Notes

- Official Codex advanced configuration page opened via browser: https://learn.chatgpt.com/docs/config-file/config-advanced
- Relevant observed lines: dot-notation config override such as `mcp_servers.context7.enabled=false`; local state under `CODEX_HOME` defaulting to `~/.codex`; history persistence can be disabled or capped with `history.max_bytes`.
- Official Codex changelog opened via browser: https://learn.chatgpt.com/docs/changelog
- Relevant observed lines include recent MCP/runtime/change-log items, remote compaction history optimization, Windows app availability, Windows updater/MSIX support, and app-server/thread-history related work.
