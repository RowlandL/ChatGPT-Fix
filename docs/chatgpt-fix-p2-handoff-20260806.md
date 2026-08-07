# ChatGPT-Fix P2 Handoff — 2026-08-06

## 项目概述

ChatGPT-Fix 是一个 Rust 项目，用于管理 OpenAI Codex 的 Windows AppX 包生命周期。当前在 `release/0.4.0` 分支上，工作树路径：

```
d:\project\worktrees\ChatGPT-Fix\chatgpt-fix-p2-20260806
```

## 当前状态

| 阶段 | 状态 |
|------|------|
| P2 离线门禁 | **已完成** — cargo metadata, fmt, clippy, test(125), build, git diff --check 全部通过 |
| P2 Live Acceptance | **已完成** — Packer inspect/plan + Manager review 全部通过 |
| P3 详细执行卡 | **已追加** — 待 A3 授权后执行 |

## 工作树信息

- **分支**: `work/p2-live-inspection`
- **工作树路径**: `d:\project\worktrees\ChatGPT-Fix\chatgpt-fix-p2-20260806`
- **子仓库**: `chatgpt-fix`（在 `d:\project\worktrees\ChatGPT-Fix\chatgpt-fix-p2-20260806` 中是独立 Git 仓库）
- **未提交修改**: 12 个源文件已修改（见下方文件清单）

## 本次会话完成的修改

### 修复 1: PowerShell 7 `-Command -` stdin 兼容性问题

**文件**: `src/chatgpt-fix-packer/src/main.rs`

将 `run_live_probe()` 从 stdin 管道模式（`-Command -` + `Stdio::piped()`）改为 `-Command <script-string>` 参数模式，因为 PowerShell 7 的 `-Command -` stdin 模式对多行脚本（含函数定义）支持不佳。

### 修复 2: PowerShell 7 `$PSStyle` ANSI 输出

**文件**: `scripts/chatgpt-fix-p2-probe.ps1`

添加 `$PSStyle.OutputRendering = 'PlainText'` 禁用 PowerShell 7 默认的 ANSI 转义序列输出。

### 修复 3: `SelectSingleNode`/`SelectNodes` 命名空间哈希表兼容性

**文件**: `scripts/chatgpt-fix-p2-probe.ps1`

PowerShell 7 不支持 `XmlDocument.SelectSingleNode(xpath, hashtable)` 的重载。改用 `Select-Xml` cmdlet：
- `$manifestXml.SelectSingleNode(xpath, $ns)` → `Select-Xml -Xml $manifestXml -XPath ... -Namespace $ns`
- `$manifestXml.SelectNodes(xpath, $ns)` → `Select-Xml -Xml $manifestXml -XPath ... -Namespace $ns`

### 修复 4: `ComputeHash` 重载歧义

**文件**: `scripts/chatgpt-fix-p2-probe.ps1`

`$sha256.ComputeHash($Bytes)` 在 PS7 中有多个重载导致歧义。添加显式类型转换：`$sha256.ComputeHash([byte[]]$Bytes)`。

### 修复 5: XPath 属性 vs 元素错误

**文件**: `scripts/chatgpt-fix-p2-probe.ps1`

原 XPath `//m:Application/m:Executable` 查找子元素，但 `Executable` 是 `Application` 的属性。改为 `//m:Application` + `GetAttribute('Executable')`。

### 修复 6: 反引号转义语法

**文件**: `scripts/chatgpt-fix-p2-probe.ps1`

`` $depName`_$depMinVer`_x64__$depPublisher `` 在 PS7 中解析错误（`` `_ `` 不是有效转义序列）。改为 `${depName}_${depMinVer}_x64__${depPublisher}`。

### 修复 7: 入口点逻辑简化

**文件**: `scripts/chatgpt-fix-p2-probe.ps1`

移除基于 `$MyInvocation.CommandOrigin` 的条件分支（`-Command` 模式下此变量为空），改为直接执行 `Invoke-ChatGptFixP2Probe` + `ConvertTo-Json`。

### 修复 8: LIVE_INVOCATION 加入 JSON 转换

**文件**: `src/chatgpt-fix-packer/src/main.rs`

`LIVE_INVOCATION` 从 `Invoke-ChatGptFixP2Probe -Mode Live` 改为 `Invoke-ChatGptFixP2Probe -Mode Live | ConvertTo-Json -Compress -Depth 10`。

### 新增: probe.rs 中的 live 路径函数

**文件**: `src/chatgpt-fix-core/src/probe.rs`

- 添加 `A2_P2_ENV` 常量
- 重构 `inspect_probe_json` 提取 `inspect_from_bytes` 内部函数
- 添加 `inspect_live_json(bytes)` 和 `plan_live_json(bytes)` 函数

### 新增: lib.rs 导出

**文件**: `src/chatgpt-fix-core/src/lib.rs`

导出 `inspect_live_json`、`plan_live_json`、`A2_P2_ENV`。

## 修改文件清单

```
 M scripts/chatgpt-fix-p2-probe.ps1
 M src/chatgpt-fix-core/src/doctor.rs
 M src/chatgpt-fix-core/src/fixture.rs
 M src/chatgpt-fix-core/src/json.rs
 M src/chatgpt-fix-core/src/lib.rs
 M src/chatgpt-fix-core/src/probe.rs
 M src/chatgpt-fix-core/src/schema.rs
 M src/chatgpt-fix-core/tests/live_fixtures.rs
 M src/chatgpt-fix-core/tests/schema_contracts.rs
 M src/chatgpt-fix-manager/tests/review.rs
 M src/chatgpt-fix-packer/src/main.rs
 M src/chatgpt-fix-packer/tests/plan.rs
```

## 测试结果

全部 125 个测试通过，无回归。

## Live Acceptance 验证结果

| 步骤 | 退出码 | 结果 |
|------|--------|------|
| `Packer inspect --live-readonly` | 0 | 规范 JSON，schema/version/architecture/publisher_id 等字段验证通过 |
| `Packer plan --live-readonly` | 0 | plan JSON（rejected/unknown_publisher，预期行为） |
| `Manager review --plan-stdin` | 0 | 143 bytes，100% 字节一致 |

## 环境变量

- `CHATGPT_FIX_A2_P2_READONLY=1` — A2-P2 授权环境变量，用于启用 live-readonly 模式

## 剩余任务 (P3 阶段)

```json
{
  "p2c": "追加 P3 详细执行卡并审查 — 已完成",
  "p2d": "获取 A3 授权 — 待处理",
  "p2e": "运行接受事务 release/0.4.0 — 待处理"
}
```

P3 阶段需要用户授权（A3 级别）后，才能执行 release/0.4.0 接受事务，包括：
- 构建 release 二进制
- 复制到 `out/0.4.0/win-x64/`
- 创建构建 receipt 和 checksum
- 运行完整的离线 + live 接受验证

## 建议技能 (Suggested Skills)

接收此 handoff 的智能体应调用以下技能：

1. **`decretum-release-fastpath`** — 用于 release/0.4.0 的打包和发布流程
2. **`decretum-matrix`** — 用于三省六部授权流程（A3 授权）
3. **`handoff`** — 如果在后续工作中需要再次交接

## 注意事项

- 所有代码修改在 `work/p2-live-inspection` 分支上，尚未提交
- 根工作树目录 `d:\project` 中可能有 workspace.yaml 需要同步更新
- 工作树不与根 Git 仓库共享 git 配置 — 这是一个独立的子仓库
- 发布二进制在 `target/x86_64-pc-windows-msvc/release/` 下
- 离线构建证据在 `tests/build_evidence.rs` 中