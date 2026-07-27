# Codex for Windows Main Process Triggers Unbounded NTFS `NtFC` Nonpaged-Pool Growth

## Suggested Issue Title

`[Windows][Critical] Codex desktop main process triggers unbounded NTFS NtFC nonpaged-pool growth and system memory exhaustion`

## One-Sentence Summary

The main `ChatGPT.exe` process shipped in Codex for Windows continuously performs approximately 100,000 to 120,000 Other I/O operations per second while the `ntfs.sys` `NtFC` pool tag grows at roughly 9 to 12 MB/s; suspending only that main process stops the growth immediately, and resuming it restarts the growth, eventually pushing the nonpaged pool above 26 GB on a 32 GB system.

## Report Metadata

| Field | Value |
|---|---|
| Report date | 2026-07-13 (Asia/Shanghai) |
| Severity | Critical: may exhaust system memory, hang applications, or crash Windows |
| Affected component | Codex for Windows desktop main/browser process |
| Desktop application version | `OpenAI.Codex_26.707.8168.0_x64` |
| Main process image | `ChatGPT.exe`, PID `16676` |
| Codex backend image | `codex.exe`, PID `20980` |
| Windows build | `26200.8655`, x64 |
| Physical memory | 31.9 GB |
| Reproduction scope | Persistent and repeatable within one Windows boot session; cross-reboot reproduction has not yet been verified |

> Note: `ChatGPT.exe` is the filename of the main desktop process inside this Codex for Windows package. Every reference to this image in this report refers to the Codex desktop component.

## User Impact

- Physical-memory usage reached approximately 89% and continued to increase.
- Available physical memory fell as low as approximately 0.35 GB.
- The nonpaged pool reached 26.89 GB and was still growing.
- Windows aggressively compressed or trimmed normal process working sets but could not reclaim the leaking kernel pool.
- Without exiting the triggering process or restarting Windows, the machine was at risk of becoming unresponsive, failing allocations, and losing unsaved work.

## Expected Behavior

Kernel-pool usage and process I/O should remain bounded while Codex runs local tools, waits for tool results, or renders streaming output. File-system resources associated with a tool call should be released after completion, interruption, or idle transition. Long-running use must not exhaust system memory.

## Actual Behavior

The Codex desktop main process continuously generated approximately 104,000 to 119,000 Other I/O operations per second while tool child processes were nearly idle. At the same time, the NTFS `NtFC` nonpaged-pool tag increased continuously while its free counter barely changed.

The pressure was not caused by ordinary process private memory, the page file, or the standby cache. The dominant anomaly was in the kernel nonpaged pool.

## Environment and Initial Memory Snapshot

Snapshot time: 2026-07-13 17:25:11 +08:00.

| Metric | Observed value |
|---|---:|
| Total physical memory | 31.90 GB |
| Used physical memory | 28.41 GB |
| Physical-memory usage | 89.1% |
| Available physical memory | 3.50 GB |
| Committed memory | 33.56 GB |
| Commit limit | 91.90 GB |
| Commit usage | 36% |
| Current page-file usage | 0.28 GB |
| Standby cache | 3.46 GB |
| Paged pool | 0.56 GB |
| Nonpaged pool | 18.65 GB |

All ordinary processes together had approximately 10.93 GB of private memory, far below the abnormal nonpaged-pool value. Sorting only by process working set therefore hides the actual problem.

## Kernel Pool-Tag Evidence

Pool tags were enumerated through the read-only `NtQuerySystemInformation(SystemPoolTagInformation)` API. The first material snapshot was:

| Tag | Nonpaged usage | Outstanding allocations | Notes |
|---|---:|---:|---|
| `NtFC` | 18,711.28 MB | 87,590,160 | Dominant entry |
| `EtwB` | 275.83 MB | 1,256 | Distant second |
| `HalD` | 168.56 MB | 229 | Distant second |
| `Cont` | 151.18 MB | 9,837 | Distant second |

In a later snapshot, `NtFC` reached 22,388.98 MB with 104,806,001 outstanding allocations. Each newly outstanding allocation was approximately 224 bytes.

A local binary string search mapped `NtFC` to:

```text
C:\Windows\System32\drivers\ntfs.sys
```

This confirms that the leaking objects are accounted under an NTFS pool tag. It does not by itself prove that the defect exists only in `ntfs.sys`: an invalid or unbounded user-mode access pattern, or an interaction with a file-system filter, may trigger unreclaimed kernel allocations.

## Timeline and Growth Rate

### System Pool Trend

| Time | Nonpaged pool | Available memory |
|---|---:|---:|
| 17:25:11 | 18.65 GB | 3.50 GB |
| 17:25:57 | 19.04 GB | 3.31 GB |
| 17:26:07 | 19.12 GB | 3.27 GB |
| 17:37:36 | 24.89 GB | 0.78 GB |
| 17:37:56 | 25.07 GB | 0.66 GB |
| Final observation | 26.89 GB | 0.35 GB |

### 15-Second `NtFC` Differential Sample

| Time | `NtFC` usage | Outstanding allocations |
|---|---:|---:|
| 17:37:36 | 23,070.10 MB | 107,994,468 |
| 17:37:56 | 23,243.50 MB | 108,806,269 |

Calculated over the 15-second effective interval:

- `NtFC` growth: 173.4 MB.
- Outstanding-allocation growth: 811,801.
- Average size per item: approximately 224 bytes.
- Equivalent growth rate: approximately 11.6 MB/s.

## Process Comparison

During the same sampling window:

| Process | Role | CPU counter | Other I/O/s | File read/write operations/s | Assessment |
|---|---|---:|---:|---:|---|
| `ChatGPT.exe` PID 16676 | Codex desktop main/browser process | approximately 96% to 145% | 104,480 to 119,512 | usually below a few hundred | Only persistent high-rate initiator |
| `codex.exe` PID 20980 | Codex backend | approximately 0% to 10% | 0 to 325 | low | Does not match the leak magnitude |
| `node.exe` MCP children | MCP services | 0% | 0 | 0 | Idle |
| `node_repl.exe` | Tool runtime | 0% | 0 | 0 | Idle |
| `pwsh.exe` | Tool commands | near 0% during comparison | near 0 | near 0 | Does not match persistent growth |

The `ChatGPT.exe` Other I/O rate was hundreds to thousands of times higher than the backend and tool children. This supports a tight polling or file/directory-operation loop in the desktop main process rather than continuous scanning by an invoked tool process.

## Controlled Causality Test

To distinguish causation from correlation, a controlled three-second suspend/resume test was performed:

1. Query current `NtFC` bytes and allocation counters.
2. Keep PID 16676 running for three seconds and query again.
3. Call `NtSuspendProcess` only for PID 16676.
4. Keep it suspended for three seconds and query `NtFC`.
5. Call `NtResumeProcess` in a `finally` block; the resume status was `0x00000000`.
6. Wait approximately 0.75 seconds after resume and query again.

Results:

| Phase | Duration | `NtFC` growth | New allocations | Frees |
|---|---:|---:|---:|---:|
| PID 16676 running | 3 seconds | 26.09 MB | 122,150 | 20 |
| PID 16676 suspended | 3 seconds | 0.00 MB | 0 | 0 |
| After PID 16676 resumed | approximately 0.75 seconds | 5.86 MB | 27,448 | 13 |

Growth stopped exactly during the suspend window and resumed immediately afterward. This is the strongest causal evidence in this report: the triggering activity originates from the Codex desktop main process, PID 16676.

## Root-Cause Assessment and Confidence

### Confirmed

- System memory pressure was dominated by the nonpaged pool, not ordinary process private memory.
- `NtFC` was by far the largest and continuously growing pool tag.
- `NtFC` was found in the local `ntfs.sys` image.
- The Codex desktop main process continuously generated an abnormal Other I/O rate.
- Suspending that main process stopped `NtFC` growth immediately; resuming it restarted the growth.
- `codex.exe`, MCP Node processes, Node REPLs, and PowerShell tool processes had no activity at a comparable scale in the comparison window.

### High-Confidence Inference

The Codex desktop main process likely enters a no-backoff I/O polling loop in a tool-call, task-state synchronization, session-history monitoring, or directory-watching path. The loop repeatedly triggers allocations of NTFS file-control structures that are not reclaimed promptly.

### Not Yet Proven

- The exact user-mode function, thread, and call stack.
- The exact file or directory path being accessed repeatedly.
- Whether this is a standalone Codex desktop defect or requires an interaction with this Windows build, NTFS, or a file-system filter driver.
- Whether every cold start reproduces the issue.

An attempt was made to capture kernel stacks with the Windows Performance Recorder `Pool` profile, but the current non-elevated session returned:

```text
Failed to enable the policy to profile system performance.
Profile Id: Pool.Verbose.File
Error code: 0xc5585011
```

No call stack has been invented or inferred beyond the available evidence.

## Low-Risk Mitigation Already Attempted

`EmptyWorkingSet` was called without terminating processes, closing windows, or discarding conversation state:

| Target | Reported release |
|---|---:|
| Explicitly selected non-critical background applications | 85.3 MB |
| Codex process group | 612.4 MB |
| Total | approximately 697.7 MB |

Available memory temporarily increased from 0.42 GB to 1.86 GB, confirming that working-set trimming succeeded. However, `NtFC` continued to grow and available memory later fell to 0.35 GB. This action only created a short buffer; it did not fix or reclaim the nonpaged-pool leak.

## Current Workaround

1. Exit Codex desktop gracefully and reopen it.
2. Wait 10 to 30 seconds after exit and verify that the nonpaged pool drops materially.
3. If the kernel pool does not release after the application exits, restart Windows.
4. Until fixed, prefer the standalone Codex CLI for long-running local tool workloads, avoiding long periods in which the desktop shell waits for tools or processes streaming tool output.
5. Keep independent CLI sessions such as cc-connect running; the evidence collected here does not identify them as high-rate triggers.

## Recommended Engineering Investigation

1. Record WPR `CPU`, `FileIO`, and `Pool` profiles together from an elevated session, preserving user-mode and kernel-mode stacks.
2. Aggregate the main `ChatGPT.exe` process's approximately 100,000 Other I/O operations per second by thread, operation type, and target path.
3. Inspect desktop-shell code for tool-state polling, session JSONL monitoring, directory watchers, child-process completion checks, and IPC read loops.
4. Verify that watchers, file and directory handles, timers, and completion callbacks are cancelled after normal completion, interruption, and abnormal child exit.
5. Check EOF and error paths for missing backoff, blocking waits, or deduplication.
6. Compare the same Codex build on this Windows build and on a stable Windows release to isolate an operating-system interaction.
7. Reproduce in controlled environments with and without third-party file-system filters; do not blindly remove drivers on an affected production machine.

## Suggested Regression Acceptance Criteria

Run at least a 60-minute local-tool stress test on a 32 GB Windows x64 machine:

- `ChatGPT.exe` Other I/O must not remain continuously in the tens-of-thousands-per-second range while idle or waiting for a tool.
- `NtFC` nonpaged usage must stabilize after warm-up and must not grow linearly without a bound.
- Tool-related resources must return to a stable baseline within 60 seconds after completion or interruption.
- After at least 100 consecutive tool calls, net nonpaged-pool growth should be near zero or remain under a documented, explainable fixed ceiling.
- Cover normal completion, user cancellation, timeout, abnormal child exit, and switching between tasks.

## Recommended Attachments

- WPR/WPA CPU, FileIO, and Pool ETL traces.
- Hot-thread stacks for PID 16676.
- File-I/O summary grouped by operation type and target path.
- Before-and-after 60-minute charts for `NtFC`, total nonpaged pool, available memory, and Other I/O rate.

This report contains no prompts, conversation text, usernames, credentials, tokens, cookies, WeChat identifiers, or private file contents.

