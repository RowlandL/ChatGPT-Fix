# ChatGPT-Fix 两个 Issue 解决计划书

状态：**已实施，纳入 v1.0.2 发布**
审查日期：2026-08-23
仓库：`RowlandL/ChatGPT-Fix`
审查基线：`release/1.0.1` / `77919fe58fa7a3c4204d69d16cbeeb042aa5762f`
Issue：[#2](https://github.com/RowlandL/ChatGPT-Fix/issues/2)、[#3](https://github.com/RowlandL/ChatGPT-Fix/issues/3)

## 1. 本轮范围与结果

- 已通过 GitHub 插件确认仓库为 `RowlandL/ChatGPT-Fix`，并克隆到当前工作区的 `ChatGPT-Fix` 目录。
- 已审查两个当前未关闭的 issue。Issue #1 已关闭，不纳入本计划。
- 已对照 `origin/release/1.0.1` 的源码、发布记录、安装器实现和现有测试。
- 当前工作树保持干净，未修改源码，未构建、安装、修复、提交、推送，也未改变 GitHub issue 状态。
- `main` 是证据/交接线；产品实现和 v1.0.1 发布线在 `release/1.0.1`。后续实现必须从 `release/1.0.1` 开修复分支，不能从 `main` 单独开发。

## 2. Issue #2：Manager 固化构建机路径，NTC 注入不可用

### 结论与级别

确认是 **P1 发布阻断缺陷**。v1.0.1 的 `ntc-ensure` 在全新安装上会因找不到自有注入器而返回退出码 4，Token 统计/HUD 不可用；官方 WindowsApps 包不会因此被修改。

### 证据与根因链

1. `src/chatgpt-fix-manager/src/main.rs:466-488` 的 `ntc-ensure` 在新基线没有 `app.asar.pre-ntc` 时进入 `run_ntc_reapply`。
2. `src/chatgpt-fix-manager/src/main.rs:596-600` 使用 `env!("CARGO_MANIFEST_DIR")` 定位 `scripts/inject-native-token-cost.js`。该宏在编译时展开，导致发布二进制嵌入构建机源码目录，例如 `D:\project\ChatGPT-Fix\...`。
3. `src/chatgpt-fix-manager/src/main.rs:626-632` 在脚本不存在时输出 `inject script missing at ...` 并返回 4；这与 issue 报告中的发布二进制行为一致。
4. `setup-winforms/SetupForm.cs:280-310` 只部署四个 EXE，没有部署上述项目自有脚本，也没有给 Manager 提供安装根解析路径。
5. `setup-winforms/SetupForm.cs:312-321` 在部署后自动调用 `EnsureNtc`；`SetupForm.cs:483-509` 又未传播 Manager 退出码和详细诊断。
6. 现有 `src/chatgpt-fix-manager/tests/ntc.rs:24-150` 覆盖若干 fixture 失败/幂等场景，但没有覆盖“发布布局中的 Manager 如何找到注入器”，也没有阻止构建机绝对路径进入二进制。

### 推荐方案

采用“自有注入器作为受管发布资源 + Manager 按安装布局运行时定位”的方案：

- 发布/安装布局增加：`<installRoot>\\scripts\\inject-native-token-cost.js`。
- Manager 默认从 `current_exe()` 推导 `<installRoot>\\bin\\ChatGPT-Fix-Manager.exe` 的父级安装根，再拼接 `scripts\\inject-native-token-cost.js`；删除生产路径中的 `CARGO_MANIFEST_DIR`。
- 可提供仅供测试/运维使用的显式路径覆盖，但仅接受存在且为常规文件的路径。
- Setup 的受管 payload 从四个 EXE 扩展为四个 EXE 加该脚本，复制前后均校验来源/目标；升级覆盖旧脚本，卸载删除受管 `scripts` 目录但保留 baseline、backups、logs 等用户数据。
- `EnsureNtc` 读取并记录 Manager 的退出码、超时和 stdout/stderr，区分“注入器缺失、Node/asar 依赖缺失、userscript 获取失败、注入失败”。NTC 失败不应回滚已完成的 baseline 安装，但必须给出可行动诊断。
- 保持第三方 token-cost userscript 的既有运行时缓存/固定 tag 下载策略，不把第三方代码混入发布包。
- 更新 README、release notes、payload manifest 和 receipt，明确自有脚本随安装器部署、第三方 userscript 不随发布分发。

备选方案是把自有脚本用 `include_bytes!` 内嵌到 Manager，并在安装根原子释放后执行；它减少 payload 遗漏风险，但增加 EXE 体积、提取完整性和安全软件风险，暂不推荐。仅把路径改成安装根而不部署脚本是无效方案。

## 3. Issue #3：WinForms Setup 漏建 `ChatGPT.lnk`

### 结论与级别

确认是 **P1 发布回归**。v1.0.1 实际发布的 WinForms Setup 只创建 `ChatGPT-Fix-Launcher.lnk`，没有创建文档和诊断工具约定的官方 `ChatGPT.lnk`，导致开始菜单入口契约不一致；现有安装、重复安装和 `--repair` 都无法自行补齐该入口。

### 证据与根因链

1. `setup-winforms/SetupForm.cs:220-271` 的安装流程调用 `CreateShortcuts`。
2. `setup-winforms/SetupForm.cs:854-868` 只创建指向 `<installRoot>\\bin\\ChatGPT-Fix-Launcher.exe` 的 `ChatGPT-Fix-Launcher.lnk`，没有 AUMID、官方入口或第二个快捷方式。
3. `SetupForm.cs:358-368` 的 `--repair` 复用同一单快捷方式函数，无法修复缺失的 `ChatGPT.lnk`。
4. `SetupForm.cs:426-429` 卸载只删除 `ChatGPT-Fix-Launcher.lnk`；补建官方入口后必须同步处理所有权和卸载。
5. `README.md:12,20-23`、`scripts/chatgpt-fix-doctor.ps1:10,153-173,581-595` 以及 `scripts/chatgpt-fix-p2-probe.ps1:84-112` 都把 `ChatGPT.lnk` 当作规范入口。
6. 保留的 Rust 实现 `src/chatgpt-fix-setup/src/main.rs:431-435,507-562` 已定义双入口设计：官方 `ChatGPT.lnk` 使用稳定 AUMID `shell:AppsFolder\\OpenAI.Codex_2p2nqsd0c76g0!App`，wrapper 单独使用 `ChatGPT-Fix-Launcher.lnk`。WinForms 迁移时未移植这项契约。

### 推荐方案

恢复既有“双入口、不接管官方启动”的设计：

- `ChatGPT.lnk`：目标为系统 `explorer.exe`，参数为稳定 AUMID，工作目录为系统目录；不指向 baseline 或 wrapper。
- `ChatGPT-Fix-Launcher.lnk`：目标为安装根下的 wrapper，工作目录为 `bin`，明确标注为 wrapper。
- 将快捷方式创建安排在 launcher 已部署并验证存在之后，或至少在保存后验证 wrapper 目标已存在；保留目录创建、一次重试和“两者都存在”的成功条件。
- `--repair` 使用同一经过验证的双快捷方式函数，只修快捷方式和卸载注册，不重拷贝 baseline、不改 `current.json/state.json`。
- 卸载对产品专属的 `ChatGPT-Fix-Launcher.lnk` 可直接删除；对通用名 `ChatGPT.lnk` 必须先比对目标、参数和描述等产品标记，仅删除安装器拥有的链接，遇到用户既有/不匹配文件则保留并报告。
- 图标优先使用稳定的本地资源或安全 fallback；不要把版本化 WindowsApps 路径作为唯一图标来源。

不推荐把 `ChatGPT.lnk` 改为指向 wrapper：这会修复文件名但违背现有“官方入口保留、wrapper 独立”的产品设计。

## 4. 统一实施顺序（获确认后执行）

1. **确认产品契约**：确认 #2 采用外置受管脚本，#3 采用官方 AUMID + 独立 wrapper；确认 `ChatGPT.lnk` 的所有权删除规则和图标策略。
2. **建立修复分支**：从 `origin/release/1.0.1` 创建两个 issue 分支或一个组合修复分支；不要从 `main` 单独实现。保留完整发布 source commit 和 provenance。
3. **先修 #2 Manager**：抽取安装根/注入器路径解析，删除生产 `CARGO_MANIFEST_DIR` 运行时依赖，补单元和隔离集成测试。
4. **扩展 WinForms payload**：部署、校验、升级和卸载自有注入器；传播 `EnsureNtc` 退出码和诊断；补 `--test-install` 测试。
5. **再修 #3 快捷方式**：实现双入口、AUMID、目标校验、重试、`--repair` 和所有权安全卸载；补安装/升级/卸载集成测试。
6. **文档与证据**：更新 README、release notes、payload manifest、receipt 和版本快照，明确第三方 userscript 的分发边界。
7. **验证与发布**：在无源码的干净 Windows 环境执行安装、重装、repair、启动、卸载；随后构建新的补丁版本（建议 `v1.0.2`），更新 SHA-256 与发布证据。
8. **回填 issue**：只有所有门禁通过并经用户确认后，才在 GitHub issue 中更新复现/修复证据并关闭对应 issue；本轮不执行此步骤。

## 5. 测试与发布门禁

### #2 NTC 门禁

- 路径单元：安装根含空格、Unicode、UNC 和长路径；当前工作目录变化不影响解析。
- Manager 隔离集成：临时布局包含 `bin\\ChatGPT-Fix-Manager.exe` 和 `scripts\\inject-native-token-cost.js`，验证调用脚本来自安装根而不是源码树。
- 负面场景：注入器缺失返回 4 且诊断明确；userscript 缺失保留既有错误类别；Node/asar 缺失、下载失败、asar 锁定、篡改和二次 ensure 均不破坏原文件。
- Setup 集成：四 EXE + 脚本的 staging manifest、复制后 hash、升级覆盖、卸载所有权和用户数据保留。
- Clean-room canary：真实 launcher-owned `app.asar` 完成 backup/atomic swap、overlay/receipt 生成，第二次运行返回 `already_injected`，全程不触碰 WindowsApps。
- CI：对 Manager 二进制扫描构建机路径（包括 `D:\project\\`、`C:\Users\\` 和工作区根）；发布 staging 必须包含五类受管 payload。

### #3 快捷方式门禁

- 隔离安装验证两个 `.lnk` 均存在，且分别满足官方 AUMID 与 wrapper 目标契约。
- 对仅有旧 wrapper 的 v1.0.1 状态运行 `--repair`，验证双入口恢复、baseline/current/state 不变且重复 repair 幂等。
- 覆盖缺少开始菜单目录、路径含空格/撇号、COM 保存失败、文件锁、launcher 缺失等失败场景；不得报告部分成功。
- 卸载删除产品拥有的两个入口，保留 baseline/backups/logs；预先存在且不匹配的 `ChatGPT.lnk` 必须保留。
- 人工在 Windows 10/11/Server 2022 验证两个入口启动行为；官方包升级后再次验证稳定 AUMID；doctor 默认运行不应报告 `shortcut-not-found`。
- `src/chatgpt-fix-setup/tests/version.rs` 不能替代 WinForms 行为测试，必须增加真实/隔离安装验证。

### 发布门禁

- 不改写或替换已发布 v1.0.1 同名资产；新修复发布为 `v1.0.2` 或经确认的下一补丁版本。
- 新构建必须记录完整 source commit、toolchain、artifact SHA-256、payload manifest 和 `SHA256SUMS.txt`，并在下载后的独立目录复核。
- 新 Setup 仍为未签名状态时，发布说明必须明确；若引入签名，另行增加证书/验证计划。

## 6. 风险、回滚与未决决策

- #2 的注入仍只针对 launcher-owned baseline；失败时保留原 `app.asar` 和 `app.asar.pre-ntc`，不得改变官方包。
- #3 的最大风险是误删用户已有 `ChatGPT.lnk`；通过目标/参数/描述匹配和不匹配保留规则规避。
- AUMID 和官方包发布者标识可能变化；安装时继续以 `OpenAI.Codex` 探测为前置，并在真实 OS 验收。
- 回滚仅恢复 launcher-owned `app.asar`，清理临时 NTC 产物和本版本受管链接；不删除用户数据，不改写旧发布资产。

请用户确认以下三点后再进入实现阶段：

1. 是否按推荐方案把 `inject-native-token-cost.js` 作为安装根 `scripts` 资源随 v1.0.2 发布？
2. 是否恢复官方 AUMID `ChatGPT.lnk` + 独立 `ChatGPT-Fix-Launcher.lnk` 双入口设计？
3. 是否以 `release/1.0.1` 为修复基线并发布新的补丁版本，而不改写 v1.0.1 资产？

本文件本身只是计划书，不代表已授权执行上述步骤。
