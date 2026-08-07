# ChatGPT-Fix v1.0.0

Windows 本地修复版 ChatGPT 桌面客户端 —— 官方包隔离运行，无内核内存泄漏。

## 核心能力

| 能力 | 说明 |
|---|---|
| **NTFS 泄漏修复** | 官方包完整复制到用户目录 baseline 运行，规避 WindowsApps 沙箱触发的内核非分页池泄漏（修复前 +440MB/min，修复后长时间运行稳定） |
| **一键安装（初版 GUI 安装器）** | `ChatGPT-Fix-Setup.exe`：带引导界面、进度条、默认管理员提权（requireAdministrator）；自动复制官方包到用户目录 baseline、写指针、创建开始菜单快捷方式，全程可视化 |
| **隔离启动** | Launcher 校验 verified baseline 后从用户目录启动，独立 profile（语言/设置/登录数据全量保留），Job Object 归属 + 驻留生命周期 |
| **初版兼容** | Launcher 同时接受新版 `chatgpt_fix.staging.v1` 与初版 `codex.ntfs.setup-state.v1` 两种 baseline 状态文件，旧安装无需重装即可启动 |
| **Token 用量插件** | 注入 Codex++ userscript（本地统计 + CC Switch 同步），输入框上方显示 Token/费用 HUD；安装后由 Manager 自动下载注入 |
| **单实例 + 窗口激活** | 重复双击复用实例并激活窗口，不重复启动 |
| **安全** | 不修改 WindowsApps、不改官方包、全部可回滚（pre-ntc 备份） |

## 发布产物（win-x64）

| 文件 | SHA-256 |
|---|---|
| ChatGPT-Fix-Launcher.exe | `DB666326CC8A74564F40EC01F2340D250BB0F171239F853EB0FECC7CC503D868` |
| ChatGPT-Fix-Manager.exe | `1972D64D30A494B3E96CA0AC72E1C7CCFCBD732B8BCD46F41212065FC9280F7D` |
| ChatGPT-Fix-Packer.exe | `38BC05A95B977200EA45298C5A3FBBDF756CEBA5ECD55A001C7C99896792282E` |
| ChatGPT-Fix-Setup.exe | `E8A9C1C238513F6240C4F90622F4CCECAE67A14002A3BD93DA0A04C9407FB3CD` |

## 安装使用

1. 准备 4 个 EXE 到同一目录（release-packages/ChatGPT-Fix/1.0.0/win-x64/）
2. 双击运行 `ChatGPT-Fix-Setup.exe`（默认管理员提权，自动请求 UAC）
3. 跟随 GUI 引导：选择安装 → 等待进度条完成（复制官方包到用户目录 baseline + 写指针 + 创建开始菜单快捷方式）
4. 双击 **ChatGPT** 或 **ChatGPT-Fix-Launcher** 快捷方式启动（无黑框，启动快）

> 初版安装器基于原始 Codex-NTFS-Fix-Setup 的 WinForms 引导界面（用户授权的最终方案）。

## Token 用量插件

- 数据源：本地捕获 + 可选 CC Switch（helper 127.0.0.1:17888，只读本地 SQLite）
- 显示位置：ChatGPT 输入框上方 HUD（本轮/会话/缓存/费用/今日累计）
- 插件自动获取：优先本机已有副本 → 本地缓存 → GitHub 上游 tag（Tianzora/codex-token-cost v0.7.9）
- 登录官方账号无冲突：插件不碰官方认证/计费，仅本地统计与页面显示
- 若官方 Profile 页显示本地伪装数据，可在插件设置关闭"本地 Profile 解锁"

## 第三方归属

- **Token 用量插件**：来自 [Tianzora/codex-token-cost](https://github.com/Tianzora/codex-token-cost)（v0.7.9），本项目仅注入运行，不宣称所有权
- 注入机制参考 Codex++ 思路，实现为本项目自有代码

## 构建

- Rust `x86_64-pc-windows-msvc`，纯标准库（win32 最小 FFI），无外部 crate 依赖
- 230+ 项测试通过，clippy 零警告
- Setup 为 .NET WinForms（初版安装器原样保留）
- 不包含官方 OpenAI 包内容（安装时从本机已注册包复制）
