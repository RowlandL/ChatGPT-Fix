# ChatGPT-Fix v1.0.1

Windows 本地修复版 ChatGPT 桌面客户端 —— 官方包隔离运行，无内核内存泄漏。**4 个 exe 同目录，双击 Setup 一键安装**。

## 核心能力

| 能力 | 说明 |
|---|---|
| **NTFS 泄漏修复** | 官方包完整复制到用户目录 baseline 运行，规避 WindowsApps 沙箱触发的内核非分页池泄漏（修复前 +440MB/min，修复后长时间运行稳定） |
| **一键安装** | 将 `ChatGPT-Fix-Launcher/Manager/Packer/Setup.exe` 4 个 exe 置于同一目录，双击 `ChatGPT-Fix-Setup.exe`（.NET WinForms，.NET Framework 4.x 系统预装、无需 SDK）：自动探测官方包 → 复制到用户目录 → 写指针 → 创建快捷方式 → 注入 Token 插件 → 启动，全程无黑框、启动快 |
| **卸载（保留数据）** | GUI「卸载」按钮或注册表卸载项（设置-应用可卸载）；移除 bin/ 快捷方式/注册表项，**保留 baselines/backups/logs**（登录信息、设置、已修复基线可续用） |
| **注册表卸载项** | 安装后写入 `HKCU\...\Uninstall\ChatGPT-Fix`（DisplayName=ChatGPT-Fix, v1.0.1），支持「设置-应用」管理 |
| **Token 用量插件** | 安装完成后自动注入 Codex++ userscript（本地统计 + CC Switch 同步），输入框上方显示 Token/费用 HUD；幂等（已注入则跳过） |
| **隔离启动** | Launcher 校验 verified baseline 后从用户目录启动，独立 profile（语言/设置/登录数据全量保留），Job Object 归属 + 驻留生命周期 |
| **初版兼容** | Launcher 同时接受新版 `chatgpt_fix.staging.v1` 与初版 `codex.ntfs.setup-state.v1` 两种 baseline 状态文件，旧安装无需重装即可启动 |
| **单实例 + 窗口激活** | 重复双击复用实例并激活窗口，不重复启动 |
| **安全** | 不修改 WindowsApps、不改官方包、全部可回滚（pre-ntc 备份） |

## v1.0.1 变更

- **PowerShell 自动检测**：优先使用 `pwsh.exe`（PowerShell 7），找不到时回退到 `powershell.exe`，兼容 Windows 25H2 及后续版本
- **UAC 取消处理**：用户取消 UAC 提升提示后不再静默退出，继续显示 GUI 窗口
- **Manager 修复**：`ntc-ensure` 和 `userscript` 下载同样使用 PowerShell 自动检测
- **Setup 换用 .NET WinForms**：替代 Rust 原生 GUI（源码 `setup-winforms/SetupForm.cs`，csc .NET Framework 4.0.30319 x64 编译），同步修复：NODE_PATH 转发（FindAsarNodePath 候选链）、NeedsCopy 修正（`resources\app.asar`）、robocopy 工作目录硬化（规避 TMP 8.3 短名分号陷阱）+ ROBEXIT 日志校验
- 全量 131 项测试通过，clippy 零警告

## 发布产物（4 exe 同目录）

| 文件 | SHA-256 |
|---|---|
| ChatGPT-Fix-Launcher.exe | `98054a54d6acbf825c1ce14d5ebb6c62f564b9f59e72b80ea3f1ee2024bba4ce` |
| ChatGPT-Fix-Manager.exe | `438fc637bfbd9cbfc983426e486016432cf1a9b2e31d7da3e5dda86fac48618e` |
| ChatGPT-Fix-Packer.exe | `1d44d7edad9e430887c13bd6eced3e329303c2e4bf6deb406065acc6e8a56fce` |
| ChatGPT-Fix-Setup.exe | `de14cac62309c9db15d1b8325cb3f59a9701e65ca7e84cbd000f44e762356c3d`（.NET WinForms 版） |

## 安装使用

1. 下载 4 个 exe 至同一目录（`ChatGPT-Fix-Launcher/Manager/Packer/Setup.exe`）
2. 双击 `ChatGPT-Fix-Setup.exe` 运行（默认管理员提权，自动请求 UAC）
3. 跟随 GUI 引导：校验官方包 → 复制到用户目录 baseline → 创建快捷方式与配置 → 启动 ChatGPT
4. 安装完成自动注入 Token 插件（幂等，已注入则跳过）并启动 ChatGPT

> 前提：本机需已安装官方 ChatGPT 桌面版（OpenAI.Codex MSIX 包），Setup 从本机已注册包复制，不包含官方包内容。

## 卸载（保留数据）

- 方式一：「设置」→「应用」→ ChatGPT-Fix → 卸载
- 方式二：`ChatGPT-Fix-Setup.exe uninstall`（命令行）
- 方式三：运行 Setup GUI → 点「卸载」
- 效果：移除 bin/（Launcher/Manager/Packer/Setup）、开始菜单快捷方式、注册表卸载项；**保留 baselines/backups/logs**（登录信息、设置、已修复基线可续用，重新安装即可恢复）

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

- Rust `x86_64-pc-windows-msvc`，纯标准库（win32 最小 FFI），无外部 crate 依赖（Launcher/Manager/Packer）
- 230+ 项测试通过，clippy 零警告
- Setup 为 .NET WinForms（`setup-winforms/SetupForm.cs`，csc .NET Framework 4.0.30319 x64，.NET Framework 4.x 系统预装、无需 SDK）；Rust Setup 源码保留于 `src/chatgpt-fix-setup/` 但不再是发布产物
- 不包含官方 OpenAI 包内容（安装时从本机已注册包复制）

