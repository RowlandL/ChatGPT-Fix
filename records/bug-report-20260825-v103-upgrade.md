# ChatGPT-Fix v1.0.3 本机升级 Bug 报告

- **报告日期**：2026-08-25
- **升级路径**：v1.0.1 → v1.0.3（tag `v1.0.3`）
- **权威仓库**：`\\TRUENAS\Omina\gitmirror\ChatGPT-Fix`（TrueNAS 共享 / O 盘；对应远端 `origin = https://github.com/RowlandL/ChatGPT-Fix.git`）
- **GitHub Issue**：[#6](https://github.com/RowlandL/ChatGPT-Fix/issues/6)
- **环境**：Windows 11（10.0.26200.8875）/ x64；官方包 OpenAI.Codex **26.814.5517.0**（含 371 字符超长路径）；Rust 1.97.1（项目锁定）；node v22.22.0
- **范围**：发布工件可运行性、Setup 基线暂存、NTC（Token 用量）注入链路
- **结论**：v1.0.3 存在 **3 个源码缺陷** + **1 个发布工件缺陷** + **1 个流程产物断层**；本机已全部绕过/修复并验证通过，源码修复已提交权威仓库（见「三、源码修复清单」）

---

## 一、问题清单总览

| # | 严重度 | 问题 | 状态 |
|---|--------|------|------|
| BUG-1 | P1 | 发布包 `.NET Setup.exe` 在所有版本上无法启动（PE 加载层拒绝） | 绕过：改用 Rust 版 Setup；建议上游调查构建产物 |
| BUG-2 | **P0** | Rust Setup 递归复制缺失 `create_dir_all`（1.0.1+ 回归）→ 任何含子目录的包 staging 必失败 | 已修复（权威仓库 main） |
| BUG-3 | **P0** | Rust std 不自动加 `\\?\` 前缀，>260 字符路径复制/校验失败（新官方包含 371 字符路径） | 已修复（权威仓库 main） |
| BUG-4 | P1 | staging manifest 相对路径锚定当前递归层 → 子目录条目被压平成顶层名（修复 BUG-2 后暴露） | 已修复（权威仓库 main） |
| BUG-5 | P2 | NTC 注入链路断层：fixture 标记 / injector 脚本 / asar 运行时均依赖 .NET Setup 部署，本机缺失 | 已补齐（手工按官方行为等价补写） |

---

## 二、逐项详情

### BUG-1：发布包 .NET Setup.exe 无法启动（P1，发布工件缺陷）

**现象**
`ChatGPT-Fix-Setup.exe`（发布包内 61,440 字节，.NET 程序集）通过 `CreateProcess` 启动即失败，系统报「指定的程序需要更新的 Windows 版本」（ERROR_OLD_WIN_VERSION）。v1.0.0 / v1.0.1 / v1.0.3 三个版本的该工件行为一致。

**排查证据**
- `file` 识别为 `PE32 executable for MS Windows 4.00 (GUI), Mono/.Net assembly`；
- 手工解析 PE：CLR 头 `MajorRuntimeVersion=5, MinorRuntimeVersion=2`（异常，标准 CLR 为 2.0/4.0），`runtime_ver_rva` 指向镜像外（0x50002，超出所有 section）；
- 本机 .NET Framework 4.8.1（Release 533509）与 .NET 8/6 运行时均在，排除运行时缺失；
- 排障试验：1.0.0/1.0.1/1.0.3 三个 Setup 全部同样失败 → 不是版本回归，是该工件（或其构建方式）与本机 PE 加载器不兼容。

**影响**
官方安装/升级通道在本机不可用。`logs/setup.jsonl` 显示历史成功安装（SetupVersion 字段仅 Rust 版写入）实际均由 Rust 版 Setup 完成——.NET Setup 从未在本机成功执行。

**处置（本机）**
用 v1.0.3 标签源码构建 Rust 版 Setup 完成安装（CLI 契约一致：`install --source <dir>`，日志 schema 一致）。

**建议（上游）**
- 核对 .NET Setup 的构建/发布流程（CLR 头版本异常疑似构建工具链或发布工序问题）；
- 或明确 Rust 版 Setup 为官方安装通道，发布包同步包含。

---

### BUG-2：Setup 递归复制缺失 `create_dir_all`（P0，回归缺陷）

**现象**
`install --source <dir>` 的 baseline staging 在复制 ~19 个顶层文件后失败，`setup_warn: cannot copy official app to baseline`；`baselines/<pkg>/app.tmp` 残留半成品。

**根因**
`src/chatgpt-fix-setup/src/main.rs` 的 `copy_dir_manifest_inner` 在 1.0.1 重构后**递归进入子目录前不创建目标目录**：

```rust
if md.is_dir() {
    copy_dir_manifest_inner(&from, &to, ...)?;   // 1.0.1+ 缺失 fs::create_dir_all(&to)
}
```

`fs::copy` 的目标父目录不存在 → 第一个子目录内的首个文件即失败。对比：v1.0.0 的同名逻辑存在 `fs::create_dir_all(dst)`；core 的 `staging.rs::copy_file`（Manager 路径）也正确创建父目录（第 200-208 行）——仅 Setup 自研复制路径回归。

**为什么此前未暴露**：1.0.1/1.0.2 的 staging 从未在含子目录的真实包上成功执行过（或从未执行到递归）。

**影响**：v1.0.1+ 的 Setup 无法暂存任何官方包（官方包必有子目录）。本机 2026-08-07 的 1.0.1 安装记录只有 `START` 无 `PASS`，与之一致。

**修复（已提交权威仓库）**：递归前补 `fs::create_dir_all(chatgpt_fix_core::long_path(&to)).ok()?;`

---

### BUG-3：Rust std 长路径（>260 字符）不自动加 `\\?\` 前缀（P0）

**现象**
修复 BUG-2 后 staging 仍在复制中途失败；Python（自动 `\\?\` 前缀）可完整复制 4,876 个文件，Rust 失败。

**根因**
官方包 26.814.5517.0 新增深度目录：

```
resources\cua_node\bin\node_modules\@oai\sky\dist\js-dependency-cache\shared-v1\
applied-bk-agent-openai-js\pnpm-store\v11\links\@rollup\plugin-typescript\12.1.2\
a522fe…\node_modules\tslib\tslib.es6.js
```

源路径 336 字符 / 目标路径 371 字符 > MAX_PATH(260)。Rust `std::fs`（`fs::copy`/`File::open`/`read_dir`）不做自动前缀，直接报错（ERROR_FILENAME_EXCED_RANGE 类）。受影响面：Setup 复制/哈希、core `sha256_file`（全量校验）、core `copy_file`（Manager activate）。

**修复（已提交权威仓库）**
- 新增 `chatgpt_fix_core::path::long_path()`：绝对路径加 `\\?\` 前缀（含 UNC → `\\?\UNC\...` 处理，相对路径/已前缀路径原样返回）；
- 接入点：Setup `copy_dir_manifest_inner` / `dir_total_bytes`；core `sha256_file`、`staging::copy_file`。

---

### BUG-4：staging manifest 相对路径锚定错误（P1，修复 BUG-2 后暴露）

**现象**
staging 完成后（4,876 文件全部落盘），按 `state.json` 全量 SHA-256 校验：4,845 个条目报 MISSING、2 个 MISMATCH。`app/locales/af.pak` 在 manifest 中被记录为 `af.pak`（丢 `locales/` 前缀）；`IwaKeyDistribution/` 内文件被压平成顶层名与真实顶层文件（大小写不同）碰撞 → MISMATCH。

**根因**
`copy_dir_manifest_inner` 中 `rel = from.strip_prefix(src)` 的 `src` 是**当前递归层**，不是最顶层源根：

```rust
let rel = from.strip_prefix(src)   // src 递归后 = 当前层目录
```

1.0.1+ 该缺陷一直存在，但 BUG-2 使递归从未成功执行，缺陷被掩盖；本机修复 BUG-2 后立即触发。

**影响**
- launcher 快速校验（`require_verified_baseline_fast` 子串检查）可通过 → 应用可启动，文件实际完整；
- 但**全量校验（`verify_staging` / Manager doctor）必失败并会将 baseline 置为 `quarantined`**（fail-closed），属潜在数据完整性地雷；
- manifest 为 4,845 个文件给出错误路径 + 2 处哈希错配，违反 `files_staged == manifest 长度` 之外的唯一性/可验证性契约。

**修复（已提交权威仓库）**
增加 `root_src` 参数贯穿递归，`rel` 一律相对最顶层源根：

```rust
let rel = from.strip_prefix(root_src)...
```

修复后全量校验：`checked=4876 missing=0 mismatch=0`，无重复路径（58s）。

---

### BUG-5：NTC（Token 用量）注入链路断层（P2，流程产物缺失）

**现象**
升级后 HUD 不显示；`Manager ntc-health` 显示 helper 健康（`127.0.0.1:17888` reachable）、`reapply_state: clean`，但 baseline 的 `app.asar` 无注入痕迹（时间戳为原包时间，无 `app.asar.pre-ntc`、无 `ntc-commit.json`）。

**根因**
v1.0.3 的 NTC 注入链路依赖 .NET Setup 部署的 4 项产物（BUG-1 导致本机全部缺失）：

| 缺失项 | 官方职责（.NET Setup） | 本机补齐方式 |
|---|---|---|
| `app/resources/chatgpt-fix.fixture`（内容 `launcher-owned-v1`） | `setup-winforms/SetupForm.cs:492` 写入 | 手工按同内容补写 |
| `scripts/inject-native-token-cost.js`（+ `inject-locale-i18n.js`） | `DeployPayloadTransactional` 部署 | 从发布包复制 |
| `ntc/node_modules/@electron/asar` 运行时 | 安装目录本地运行时 | `npm install --prefix <root>/ntc @electron/asar` |
| `app.asar` 注入（备份 + 原子替换） | 安装时注入 | `Manager ntc-ensure --fixture-root <baseline>/app/resources`（NODE_PATH 指向 ntc 运行时） |

**附加约束**
- `app.asar`（~287MB）被运行中的 Electron 进程锁定，注入的原子替换阶段报 `拒绝访问 (os error 5)` → **必须先关闭应用再注入，注入完成后再拉起**；
- userscript 优先使用本机已有副本 `%USERPROFILE%\.codex\tools\codex-token-cost\scripts\codex-live-token-cost.js`（未触发上游下载）。

**验证**
`ntc_manifest.v1`：`before a872ead5… → after f0d8e676…`，`backup_ref: app.asar.pre-ntc`，`reapply_state: clean`；重启后 HUD 正常显示。

---

## 三、源码修复清单（已提交权威仓库 `\\TRUENAS\Omina\gitmirror\ChatGPT-Fix`，main 分支）

| 文件 | 修改 |
|---|---|
| `src/chatgpt-fix-core/src/path.rs` | 新增 `long_path()`（`\\?\` 前缀，UNC/相对路径处理） |
| `src/chatgpt-fix-core/src/sha256.rs` | `sha256_file` 使用 `long_path` |
| `src/chatgpt-fix-core/src/staging.rs` | `copy_file` 创建目录与复制使用 `long_path` |
| `src/chatgpt-fix-core/src/lib.rs` | 导出 `long_path` |
| `src/chatgpt-fix-setup/src/main.rs` | 递归补 `create_dir_all`；复制/读/遍历走 `long_path`；manifest 相对路径锚定 `root_src` |

建议：以上修复合入上游（v1.0.3 发布分支或 `fix/portable-runtime-paths-v1.0.3`），并补充「含子目录 + 超长路径」的 staging 集成测试（现有 225 项测试未覆盖递归实际执行）。

---

## 四、本机最终状态

- ChatGPT-Fix 1.0.3：Launcher / Manager / Packer / Setup 已部署 `AppData\Local\Programs\ChatGPT-Fix\bin\`（旧版备份 `backups\1787670499\`）
- baseline：`OpenAI.Codex_26.814.5517.0_x64__2p2nqsd0c76g0`，4,876 文件全量 SHA-256 校验 PASS，`state=verified`
- NTC：已注入（`app.asar` 300,415,179 字节，`app.asar.pre-ntc` 备份保留），HUD 显示正常
- 回滚：旧 baseline `26.803.5235.0` 完整保留；`app.asar.pre-ntc` 为原包

## 五、遗留事项

1. 源码修复已提交权威仓库 main；是否推送到 GitHub 远端（origin）待决策
2. `ChatGPT (2).lnk`（用户手动建）仍指向旧 baseline 26.803，可手动清理
3. 若后续官方包再次引入 >260 字符路径或 .NET Setup 仍无法运行，本报告流程可直接复用
