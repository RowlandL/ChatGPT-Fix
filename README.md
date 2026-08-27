# ChatGPT-Fix v1.0.5

Windows 本地修复版 ChatGPT 桌面客户端 —— 官方包隔离运行，修复 NTFS 内核非分页池泄漏，集成本地 Token 用量统计。

> ⚠️ **非 OpenAI 官方产品**。本项目与 OpenAI 无关，未获得 OpenAI 任何授权或认可。使用本项目即表示您同意自行承担相关风险，并遵守 [OpenAI 使用条款](https://openai.com/policies/terms-of-use/)。

## 功能

| 能力 | 说明 |
|---|---|
| **NTFS 泄漏修复** | 将官方包完整复制到用户目录 baseline 运行，规避 WindowsApps 沙箱环境触发的内核非分页池泄漏（实测：修复前 +440MB/min，修复后长时间运行稳定） |
| **一键安装** | 将完整 payload 放在 Setup 同目录后运行 `ChatGPT-Fix-Setup.exe install`：复制 baseline、写入指针、创建两个开始菜单快捷方式、记录安装日志 |
| **隔离启动** | Launcher 校验 verified baseline 后从用户目录启动；独立 profile（语言/登录/设置数据保留）；Job Object 归属 + 驻留生命周期 |
| **Token 用量统计** | 输入框上方实时 HUD：本轮/会话 Token、缓存命中、费用、今日累计；本地统计 + 可选 CC Switch 同步；HUD 可见性节流（rAF 批处理 + 500 ms）+ 状态缓存，切换会话不再触发桌面端 ResizeObserver 布局循环导致卡顿 |
| **单实例 + 窗口激活** | 重复双击复用实例并激活窗口；无窗口残留实例自动清理并重新拉起，快捷方式不再"点了没反应" |
| **中文界面持久化** | `ChatGPT-Fix-Locale.exe fix` 将渲染器内 `enable_i18n` 默认值改为开启：中文界面不再依赖从 chatgpt.com 拉取 Statsig 开关，断网/网络受限时保持中文 |
| **安全设计** | 不修改 `WindowsApps`、不随发布分发官方包内容、全部可回滚（注入前自动备份） |

## 安装使用

1. 从 [Releases](https://github.com/RowlandL/ChatGPT-Fix/releases) 下载 v1.0.5 的 zip（推荐，内部保留 `scripts/`）并解压；也可以将五个 EXE 与扁平文件名 `inject-native-token-cost.js` 下载到同一目录
2. 需先安装官方 OpenAI.Codex（ChatGPT 桌面版）
3. 双击运行 `ChatGPT-Fix-Setup.exe`，安装器会从其所在 payload 目录完成安装或升级
4. 双击开始菜单的 **ChatGPT-Fix-Launcher** 或 **ChatGPT** 启动
5. （可选）界面为英文时，退出应用后运行 `ChatGPT-Fix-Locale.exe fix` 固化中文界面；`ChatGPT-Fix-Locale.exe status` 查看注入状态

## 已知限制

- **官方插件商店安装需要 ChatGPT 登录**：桌面应用内置的官方远程插件目录（`Codex official` / `openai-curated-remote`）在读取详情与安装时由应用侧强制要求 ChatGPT 账号认证（`chatgpt authentication required for remote plugin catalog`），API-key 认证模式（`PROXY_MANAGED`）不被支持。这是应用自身的设计限制，本项目不绕过账号体系。
- **NTC（Token 用量插件）当前存在 bug，v1.0.5 暂时不集成/默认禁用**：该用户脚本在切换会话与流式输出时触发 UI 卡顿。实测定位是 `syncHubVisibility` 每次执行都重写 HUD 的 `style.display` 等 DOM，会话切换 DOM 重建时反复触发 26.814+/26.818 桌面端自带的 ResizeObserver 布局循环 → 渲染器主线程饱和。已通过 A/B（关闭 NTC 后切换会话不卡）确认根因，但该插件自身修复尚未完成，故 **v1.0.5 发布不含可用的 NTC**；注入脚本与修复补丁保留在仓库，待上游/后续修复后重新启用。
- **本地市场不受影响**：以 `source_type = "local"` 注册的本地插件市场（如 `marketplaces/<name>` 下的清单与插件副本）完整走本地安装路径，无需账号；此类配置建议写入 cc-switch 接管的通用配置，避免应用侧配置重写时丢失。
- 平台上的「完成 Windows 设置」（Windows Sandbox 沙盒）横幅与权限提示取决于运行环境（缺 Hyper-V/嵌套虚拟化的虚拟机无法完成 elevated 沙盒准备）；Launcher 会将旧版 `windows.sandbox = "low"` 归一为 `elevated` 以对齐最新应用契约，沙盒就绪状态由应用侧判定。

## v1.0.5 发布契约

- **稳定用户 profile**：Launcher 统一使用 `<install-root>/profile/user-data`，不再使用 `<baseline>/profile/user-data`。基线升级不会重置宠物、上下文用量显示、外观、登录态或 Desktop 设置；首次启动会迁移旧基线 profile，并保留失败时的原目录。
- **快捷方式修复**：安装器管理的 `ChatGPT` 与 `ChatGPT-Fix-Launcher` 入口都指向修复版 Launcher；前者保留通用名称以兼容既有打开方式，后者明确标示修复入口。旧版直连基线的快捷方式由 v1.0.5 Setup 迁移；名称已被非本产品快捷方式占用时，安装器保留原文件并使用受管 fallback 名称。
- **官方版本同步边界**：Setup 每次安装/升级都会按注册版本选择最新的官方 OpenAI.Codex 包并生成对应 baseline；运行中的 Launcher 不会自动复制或覆盖 baseline。官方包更新后，请在手动关闭 ChatGPT 后重新运行本目录的 v1.0.5 Setup，以保留稳定 profile 并切换到新版本。
- **NTC 保持禁用**：本版本不重新启用 Token 用量注入，避免已知的会话切换卡顿。

## v1.0.4 发布契约（历史）

- **定版说明**：v1.0.4 曾采用 26.818 基线 + 基线内 profile；该策略会在基线切换时丢失 Desktop 用户状态，已由 v1.0.5 替换。**NTC（Token 用量插件）因当前存在切换会话/流式输出卡顿 bug，v1.0.4 默认不集成、不启用**（见「已知限制」）。
- **整改点 1（profile 权威路径）**：v1.0.4 曾将 `--user-data-dir` 指向 baseline-profile-user-data；v1.0.5 恢复安装根稳定 profile。
- **NTC 修复与打包改进（保留但默认不启用）**：inject-native-token-cost.js 新增 HUD 可见性状态缓存（syncHubVisibility 在可见性未变化时跳过全部 DOM 写，切断与 26.814+/26.818 桌面端 ResizeObserver 布局循环的自激回路）；打包第 4 步改用 createPackageWithOptions(work, outAsar, {})（无 unpack glob，native 留在 asar body，避免宽泛 glob 解出 tslib 等纯 JS 依赖导致加载失败）；unpacked 目录按固定名 app.asar.unpacked 复制，保证原生模块解析正确；注入幂等（重复 ntc-reapply 不叠加 loader）。这些改动为后续重新启用 NTC 预留，当前 v1.0.4 不启用。
- **保留 v1.0.3 发布契约**（快捷方式、install 支持 scripts/inject-native-token-cost.js、hash/SBOM/receipt 仅就实际构建产物生成）。

## v1.0.3 发布契约

- 保留 v1.0.2 对 GitHub issue #2（全新安装缺少自有 injector）与 issue #3（缺少 `ChatGPT` 开始菜单入口）的修复。
- 启动前清洗保留的 `openai-bundled` 本地 marketplace 配置，并把旧版 `windows.sandbox = "low"` 归一为 `elevated`，避免跨机器配置污染导致降权或 app-server 重试。
- 新增 `ChatGPT-Fix-Locale.exe`（专用中文修复工具）：离线固化渲染器 i18n 开关，配合 `scripts/inject-locale-i18n.js` 使用，幂等、失败关闭、注入前备份 `app.asar.pre-locale`。
- 发布校验从单一扫描 Manager 改为扫描全部发布 EXE，并仅依据当前构建环境推导禁止的构建路径标记，不再写死某台机器的 `D:\project` / `C:\Users`。
- 发布 payload 共 6 项：5 个 ChatGPT-Fix EXE（Launcher/Manager/Packer/Locale/Setup），外加本项目自有的 injector；injector 是发布物的一部分，供 Manager 使用。
- 安装器同时接受 zip 解压后的 `scripts/inject-native-token-cost.js` 和五项扁平下载目录中的 `inject-native-token-cost.js`；不会从源码树依赖构建机路径。
- 第三方 Token 用量 userscript 不随包分发；仍按其上游来源与许可单独获取。
- 安装完成必须同时提供 `ChatGPT-Fix-Launcher` 和 `ChatGPT` 两个开始菜单快捷方式。
- v1.0.3 仍为未签名（unsigned）发布。构建后才可生成并复核对应的 hash、SBOM、测试结果和 receipt。
- v1.0.1 的发布资产、checksums、receipt 与其他历史记录不得改写。

## 第三方组件归属与声明

本项目**不宣称**对以下第三方内容的任何所有权，并在分发中**不包含**以下第三方代码（安装/运行时从各自来源获取）：

| 组件 | 来源 | 许可证 | 获取方式 |
|---|---|---|---|
| **Token 用量插件**（userscript） | [Tianzora/codex-token-cost](https://github.com/Tianzora/codex-token-cost)（v0.7.9） | 上游仓库未提供显式 LICENSE（默认版权保留） | `ntc-reapply` 自动获取：优先本机已有副本，否则从上游 GitHub tag 下载；**不随本发布分发** |
| **注入机制思路参考** | [BigPizzaV3/CodexPlusPlus](https://github.com/BigPizzaV3/CodexPlusPlus) | AGPL-3.0 | 仅参考其 userscript 注入思路；本项目 loader 为**独立自研实现**，不包含其代码 |
| **官方 ChatGPT** | OpenAI | 受 [OpenAI 使用条款](https://openai.com/policies/terms-of-use/) 约束 | 用户从官方渠道安装；本项目仅在本机复制运行，不包含/分发官方包内容 |

**免责声明**：
- 本项目与 OpenAI 无任何关联，非官方发布
- 复制并在本地修改运行官方应用可能违反 OpenAI 使用条款（如 "modify/copy" 条款），请自行评估风险；本项目使用者自行承担相应后果
- Token 用量插件来自第三方开源社区，其行为与许可以其上游项目为准；本项目不对其负责
- 本项目仅用于个人本地使用与研究目的

## 构建

- Rust `x86_64-pc-windows-msvc`，纯标准库（最小 Win32 FFI），无外部 crate 依赖
- 发布前运行测试与 clippy；仅在实际构建后发布对应的校验和、SBOM 与 receipt
- 构建产物哈希见各 Release 的 `SHA256SUMS.txt`
- 下载全部 Release 资产后可运行 `scripts/verify-downloaded-release.ps1 -DownloadDirectory <目录> -ExpectedVersion 1.0.5 -ExpectedSourceCommit <tag commit>` 独立复核资产、zip、receipt 与 SBOM

## 许可证

本项目自研代码（Rust 二进制、安装/注入脚本、文档）以 **MIT License** 发布，见 [LICENSE](LICENSE)。

第三方组件（如 Token 用量插件 userscript）**不适用本许可证**，其权利归各自上游作者所有。
