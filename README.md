# ChatGPT-Fix v1.0.2

Windows 本地修复版 ChatGPT 桌面客户端 —— 官方包隔离运行，修复 NTFS 内核非分页池泄漏，集成本地 Token 用量统计。

> ⚠️ **非 OpenAI 官方产品**。本项目与 OpenAI 无关，未获得 OpenAI 任何授权或认可。使用本项目即表示您同意自行承担相关风险，并遵守 [OpenAI 使用条款](https://openai.com/policies/terms-of-use/)。

## 功能

| 能力 | 说明 |
|---|---|
| **NTFS 泄漏修复** | 将官方包完整复制到用户目录 baseline 运行，规避 WindowsApps 沙箱环境触发的内核非分页池泄漏（实测：修复前 +440MB/min，修复后长时间运行稳定） |
| **一键安装** | 将完整 payload 放在 Setup 同目录后运行 `ChatGPT-Fix-Setup.exe install`：复制 baseline、写入指针、创建两个开始菜单快捷方式、记录安装日志 |
| **隔离启动** | Launcher 校验 verified baseline 后从用户目录启动；独立 profile（语言/登录/设置数据保留）；Job Object 归属 + 驻留生命周期 |
| **Token 用量统计** | 输入框上方实时 HUD：本轮/会话 Token、缓存命中、费用、今日累计；本地统计 + 可选 CC Switch 同步 |
| **单实例 + 窗口激活** | 重复双击复用实例并激活窗口 |
| **安全设计** | 不修改 `WindowsApps`、不随发布分发官方包内容、全部可回滚（注入前自动备份） |

## 安装使用

1. 从 [Releases](https://github.com/RowlandL/ChatGPT-Fix/releases) 下载 v1.0.2 的 zip（推荐，内部保留 `scripts/`）并解压；也可以将四个 EXE 与扁平文件名 `inject-native-token-cost.js` 下载到同一目录
2. 需先安装官方 OpenAI.Codex（ChatGPT 桌面版）
3. 双击运行 `ChatGPT-Fix-Setup.exe`，安装器会从其所在 payload 目录完成安装或升级
4. 双击开始菜单的 **ChatGPT-Fix-Launcher** 或 **ChatGPT** 启动

## v1.0.2 发布契约

- 修复 GitHub issue #2（全新安装缺少自有 injector）与 issue #3（缺少 `ChatGPT` 开始菜单入口）。
- 发布 payload 共 5 项：4 个 ChatGPT-Fix EXE，外加本项目自有的 injector；injector 是发布物的一部分，供 Manager 使用。
- 安装器同时接受 zip 解压后的 `scripts/inject-native-token-cost.js` 和五项扁平下载目录中的 `inject-native-token-cost.js`；不会从源码树依赖构建机路径。
- 第三方 Token 用量 userscript 不随包分发；仍按其上游来源与许可单独获取。
- 安装完成必须同时提供 `ChatGPT-Fix-Launcher` 和 `ChatGPT` 两个开始菜单快捷方式。
- v1.0.2 仍为未签名（unsigned）发布。构建后才可生成并复核对应的 hash、SBOM、测试结果和 receipt。
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
- 下载全部 Release 资产后可运行 `scripts/verify-downloaded-release.ps1 -DownloadDirectory <目录> -ExpectedVersion 1.0.2 -ExpectedSourceCommit <tag commit>` 独立复核资产、zip、receipt 与 SBOM

## 许可证

本项目自研代码（Rust 二进制、安装/注入脚本、文档）以 **MIT License** 发布，见 [LICENSE](LICENSE)。

第三方组件（如 Token 用量插件 userscript）**不适用本许可证**，其权利归各自上游作者所有。
