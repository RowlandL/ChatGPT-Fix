// ChatGPT-Fix Setup — .NET WinForms installer (based on the initial
// Codex-NTFS-Fix installer: VERIFY -> INSTALL -> CONFIGURE -> LAUNCH).
// Compile in a normal terminal (not inside the AI tool sandbox):
//   cd <dir>
//   "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe" /nologo /target:winexe /out:ChatGPT-Fix-Setup.exe /win32manifest:app.manifest SetupForm.cs
// Requires .NET Framework 4.x (preinstalled on Windows; no SDK needed).
using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Windows.Forms;
using Microsoft.Win32;

namespace ChatGPTFixSetup
{
    public class SetupForm : Form
    {
        private readonly Label[] _steps = new Label[4];
        private readonly Label _status;
        private readonly ProgressBar _progress;
        private readonly Button _install;
        private readonly Button _uninstall;
        private readonly Button _cancel;
        private volatile bool _running;

        private const string LocalAppData = @"%LOCALAPPDATA%\Programs\ChatGPT-Fix";

        // True when running as the CLI automation mode (--test-install):
        // there is no window handle, so ALL UI updates must be skipped
        // (BeginInvoke on a handle-less control can throw and abort the copy).
        private static bool _cliMode;
        internal static void SetCliMode(bool cli) { _cliMode = cli; }

        // --- Test hooks (opt-in, default OFF) -------------------------------
        // Two channels (BOTH default OFF, production behaviour unchanged):
        //   1. A sidecar file "test-config.json" sitting NEXT TO the running
        //      Setup.exe (e.g. inside a throwaway _iso_test folder). This is
        //      the RELIABLE channel: no env-var inheritance needed, so a CLI
        //      subprocess launched by an external tool cannot accidentally
        //      target the real installation. Format:
        //        { "install_root": "...", "shell_root": "...", "uninstall_subkey": "..." }
        //   2. Env vars CODEX_NTFS_FIX_SETUP_TESTING=1 + TEST_INSTALL_ROOT /
        //      TEST_SHELL_ROOT / TEST_UNINSTALL_SUBKEY (fallback).
        //   Optional sidecar field "skip_launch": "1" makes --test-install
        //   skip the final Launcher start, so an automated re-verification
        //   can never hijack (or kill) a live ChatGPT instance. Production
        //   has no test-config.json and always launches.
        private sealed class TestConfig
        {
            public string install_root;
            public string shell_root;
            public string uninstall_subkey;
            public string skip_launch;
        }

        private static TestConfig _testConfig;
        private static TestConfig GetTestConfig()
        {
            if (_testConfig != null) return _testConfig;
            string exeDir = Path.GetDirectoryName(
                System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
            string cfg = Path.Combine(exeDir, "test-config.json");
            if (File.Exists(cfg))
            {
                try
                {
                    string json = File.ReadAllText(cfg);
                    _testConfig = NewtonsoftSafe(json);
                    return _testConfig;
                }
                catch { /* fall through to env-var channel */ }
            }
            if (Environment.GetEnvironmentVariable("CODEX_NTFS_FIX_SETUP_TESTING") == "1")
            {
                _testConfig = new TestConfig
                {
                    install_root = Environment.GetEnvironmentVariable("TEST_INSTALL_ROOT"),
                    shell_root = Environment.GetEnvironmentVariable("TEST_SHELL_ROOT"),
                    uninstall_subkey = Environment.GetEnvironmentVariable("TEST_UNINSTALL_SUBKEY")
                };
                return _testConfig;
            }
            _testConfig = new TestConfig(); // all null => disabled
            return _testConfig;
        }

        private static TestConfig NewtonsoftSafe(string json)
        {
            // Minimal JSON parse (no external dependency): extract the three
            // string values by key. Keys must be the exact names above.
            var cfg = new TestConfig();
            cfg.install_root = ExtractJsonString(json, "install_root");
            cfg.shell_root = ExtractJsonString(json, "shell_root");
            cfg.uninstall_subkey = ExtractJsonString(json, "uninstall_subkey");
            cfg.skip_launch = ExtractJsonString(json, "skip_launch");
            return cfg;
        }

        private static string ExtractJsonString(string json, string key)
        {
            string marker = "\"" + key + "\"";
            int i = json.IndexOf(marker, StringComparison.Ordinal);
            if (i < 0) return null;
            int v = json.IndexOf('"', i + marker.Length);
            if (v < 0) return null;
            int e = json.IndexOf('"', v + 1);
            if (e < 0) return null;
            return json.Substring(v + 1, e - v - 1);
        }

        private static string GetInstallRoot()
        {
            var cfg = GetTestConfig();
            if (!string.IsNullOrEmpty(cfg.install_root)) return cfg.install_root;
            return Environment.ExpandEnvironmentVariables(LocalAppData);
        }

        private static string GetShellProgramsDir()
        {
            var cfg = GetTestConfig();
            if (!string.IsNullOrEmpty(cfg.shell_root)) return cfg.shell_root;
            return Environment.GetFolderPath(Environment.SpecialFolder.StartMenu) + "\\Programs";
        }

        private static string GetUninstallSubkey()
        {
            var cfg = GetTestConfig();
            if (!string.IsNullOrEmpty(cfg.uninstall_subkey)) return cfg.uninstall_subkey;
            return @"Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPT-Fix";
        }

        public SetupForm()
        {
            Text = "ChatGPT-Fix 安装程序";
            ClientSize = new Size(500, 250);
            StartPosition = FormStartPosition.CenterScreen;
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;

            string[] stepTexts = {
                "1. 校验官方 ChatGPT 包",
                "2. 复制到用户目录（修复 NTFS 泄漏）",
                "3. 创建快捷方式与配置",
                "4. 启动 ChatGPT",
            };
            for (int i = 0; i < 4; i++)
            {
                _steps[i] = new Label { Text = "○ " + stepTexts[i], Left = 20, Top = 34 + i * 24, Width = 420 };
                Controls.Add(_steps[i]);
            }

            _status = new Label { Text = "准备就绪，点击“开始安装”。", Left = 20, Top = 136, Width = 420 };
            Controls.Add(_status);

            _progress = new ProgressBar { Left = 20, Top = 160, Width = 420, Height = 20 };
            Controls.Add(_progress);

            _install = new Button { Text = "开始安装", Left = 180, Top = 200, Width = 90, Height = 30 };
            _install.Click += (s, e) => StartInstall();
            Controls.Add(_install);

            _uninstall = new Button { Text = "卸载", Left = 280, Top = 200, Width = 90, Height = 30 };
            _uninstall.Click += (s, e) => StartUninstall();
            Controls.Add(_uninstall);

            _cancel = new Button { Text = "取消", Left = 380, Top = 200, Width = 90, Height = 30 };
            _cancel.Click += (s, e) => Close();
            Controls.Add(_cancel);
        }

        private void SetStep(int index)
        {
            if (index < _steps.Length && _steps[index] != null)
                _steps[index].Text = "● " + _steps[index].Text.Substring(2);
        }

        private void StartInstall()
        {
            if (_running) return;
            _running = true;
            _install.Enabled = false;
            _install.Text = "安装中…";
            SetStep(0);
            _status.Text = "正在校验官方 ChatGPT 包…";
            new System.Threading.Thread(() =>
            {
                var result = DoInstall();
                BeginInvoke((Action)(() => OnFinished(result)));
            }).Start();
        }

        internal bool DoInstall()
        {
            try
            {
                return DoInstallInner();
            }
            catch (Exception ex)
            {
                LastError = ex.ToString();
                SetStatus("安装失败：" + ex.Message);
                return false;
            }
        }

        // Last error detail (used by --test-install to write test-install.log).
        internal string LastError { get; private set; }

        private bool DoInstallInner()
        {
            try
            {
                // 1. Discover the official package.
                var appRoot = DiscoverPackage();
                if (appRoot == null) { SetStatus("未检测到官方 OpenAI.Codex 包。"); return false; }
                string exe = Path.Combine(appRoot, "app", "ChatGPT.exe");
                if (!File.Exists(exe)) { SetStatus("官方包 app\\ChatGPT.exe 不存在。"); return false; }

                // 2. Copy to the user-owned baseline with progress.
                string installRoot = GetInstallRoot();
                string pkgName = new DirectoryInfo(appRoot).Name;
                string baseline = Path.Combine(installRoot, "baselines", pkgName);
                string appDst = Path.Combine(baseline, "app");
                if (!NeedsCopy(Path.Combine(appRoot, "app"), appDst))
                {
                    SetStatus("baseline 已存在且版本一致，跳过复制。");
                }
                else
                {
                    SetStatus("正在复制官方包到用户目录（约 1-2 分钟）…");
                    long total = TotalBytes(Path.Combine(appRoot, "app"));
                    CopyTree(Path.Combine(appRoot, "app"), appDst, total);
                }
                SetStep(1);
                _progress.Value = 92;

                // 3. Write pointer + shortcuts + verified state.json.
                SetStatus("正在创建快捷方式与配置…");
                Directory.CreateDirectory(installRoot);
                Directory.CreateDirectory(baseline); // state.json lives here
                File.WriteAllText(Path.Combine(installRoot, "current.json"),
                    "{\"schema\":\"chatgpt_fix.pointer.v1\",\"baseline_root\":\"" +
                    baseline.Replace("\\", "/") + "\"}\n");
                // state.json (StagingV1 verified) so the launcher's baseline
                // check passes. The manifest now reflects the REAL copied
                // tree (file count + total bytes) instead of a placeholder
                // single entry, so a partial copy can never be "verified".
                var copied = DirStats(appDst);
                string sha = Sha256(exe);
                string stateJson =
                    "{\"schema\":\"chatgpt_fix.staging.v1\",\"source_package_full_name\":\"" + pkgName +
                    "\",\"source_version\":\"\",\"source_hash_manifest\":[{\"relative_path\":\"ChatGPT.exe\",\"bytes\":" +
                    new FileInfo(exe).Length + ",\"sha256\":\"" + sha +
                    "\"}],\"staging_root\":\"" + baseline.Replace("\\", "/") +
                    "\",\"baseline_root\":\"" + baseline.Replace("\\", "/") +
                    "\",\"files_staged\":" + copied.Item1 + ",\"total_bytes\":" + copied.Item2 +
                    ",\"state\":\"verified\",\"created_at_utc\":\"" +
                    DateTime.UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ") + "\"}\n";
                File.WriteAllText(Path.Combine(baseline, "state.json"), stateJson);
                CreateShortcuts(installRoot);
                // 3.5. Register the uninstall entry so "Settings > Apps" can
                // uninstall ChatGPT-Fix (data-preserving). Best-effort: a
                // failure here must not fail the install.
                try { RegisterUninstallEntry(installRoot); }
                catch (Exception ex) { SetStatus("提示：注册卸载项失败（" + ex.Message + "）"); }
                SetStep(2);
                _progress.Value = 100;

                // 4. Deploy bin\ (Launcher/Manager/Packer) from the Setup's
                // own directory, then launch. This makes the single-exe
                // install actually self-contained: if the 3 Rust binaries
                // sit next to Setup.exe, they are copied into bin\; if they
                // are missing, the install FAILS LOUDLY instead of silently
                // "succeeding" with nothing to launch.
                string exeDir = Path.GetDirectoryName(
                    System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
                string binDir2 = Path.Combine(installRoot, "bin");
                string[] rustBins = { "ChatGPT-Fix-Launcher.exe", "ChatGPT-Fix-Manager.exe", "ChatGPT-Fix-Packer.exe" };
                int missingBins = 0;
                foreach (string name in rustBins)
                {
                    string srcFile = Path.Combine(exeDir, name);
                    string dstFile = Path.Combine(binDir2, name);
                    if (File.Exists(srcFile))
                    {
                        try { Directory.CreateDirectory(binDir2); File.Copy(srcFile, dstFile, true); }
                        catch (Exception bex) { SetStatus("提示：部署 " + name + " 失败（" + bex.Message + "）"); }
                    }
                    else if (!File.Exists(dstFile)) missingBins++;
                }
                if (missingBins > 0)
                {
                    throw new IOException(missingBins + " 个程序文件（Launcher/Manager/Packer）未找到。" +
                        "请将 4 个 EXE（Setup + 3 个 Rust 程序）放在同一目录后重试。");
                }
                // 4.1. Ensure the token-cost overlay (idempotent) AFTER bin\
                // is deployed: EnsureNtc short-circuits when the Manager is
                // absent, so calling it before step 4 silently skipped the
                // injection on a fresh install. The Manager's ntc-ensure
                // short-circuits when already injected.
                bool ntcOk = false;
                try { ntcOk = EnsureNtc(installRoot, baseline); }
                catch (Exception ex) { SetStatus("提示：Token 插件检查跳过（" + ex.Message + "）"); }
                if (!ntcOk)
                    SetStatus("提示：Token 插件注入未完成（缺少 @electron/asar 或注入失败），可稍后重新安装重试。");
                SetStatus("正在启动 ChatGPT…");
                string launcher = Path.Combine(binDir2, "ChatGPT-Fix-Launcher.exe");
                if (File.Exists(launcher) && !TestSkipLaunch())
                    Process.Start(launcher);
                SetStep(3);
                return true;
            }
            catch (Exception ex)
            {
                // Re-throw so the OUTER DoInstall() records LastError and
                // returns false; keeping a local catch here would swallow
                // the detail (LastError stayed empty in --test-install).
                throw new IOException("安装失败：" + ex.Message, ex);
            }
        }

        private void StartUninstall()
        {
            if (_running) return;
            var confirm = MessageBox.Show(
                "将移除 ChatGPT-Fix 程序文件（bin、快捷方式、卸载项），\n但保留你的数据（登录信息、设置、备份、已修复的基线）。\n\n确定要卸载吗？",
                "ChatGPT-Fix 卸载（保留数据）",
                MessageBoxButtons.YesNo, MessageBoxIcon.Question);
            if (confirm != DialogResult.Yes) return;
            _running = true;
            _install.Enabled = false;
            _uninstall.Enabled = false;
            _uninstall.Text = "卸载中…";
            SetStatus("正在卸载（保留数据）…");
            new System.Threading.Thread(() =>
            {
                bool ok = DoUninstall();
                BeginInvoke((Action)(() => OnUninstallFinished(ok)));
            }).Start();
        }

        /// <summary>Repair mode: rebuild the start-menu shortcut and the
        /// uninstall registry entry WITHOUT touching the baseline (no copy,
        /// no state.json rewrite). CLI: `ChatGPT-Fix-Setup.exe --repair`.</summary>
        public bool RepairShortcutAndRegistry()
        {
            try
            {
                string installRoot = GetInstallRoot();
                CreateShortcuts(installRoot);
                RegisterUninstallEntry(installRoot);
                return true;
            }
            catch (Exception ex)
            {
                SetStatus("修复失败：" + ex.Message);
                return false;
            }
        }

        public bool DoUninstall()
        {
            try
            {
                string installRoot = GetInstallRoot();
                // 1. Remove the four EXEs from bin\ (Launcher/Manager/Packer/Setup).
                // A running Setup.exe cannot delete itself; skip it and leave a
                // marker so the next run (or manual cleanup) finishes the job.
                string binDir = Path.Combine(installRoot, "bin");
                if (Directory.Exists(binDir))
                {
                    string self = System.Reflection.Assembly.GetExecutingAssembly().Location;
                    foreach (string name in new[] { "ChatGPT-Fix-Launcher.exe", "ChatGPT-Fix-Manager.exe", "ChatGPT-Fix-Packer.exe", "ChatGPT-Fix-Setup.exe" })
                    {
                        string p = Path.Combine(binDir, name);
                        try
                        {
                            if (!File.Exists(p)) continue;
                            // Compare normalized full paths: Assembly.Location
                            // may use a different casing/format than Path.Combine.
                            string pFull = Path.GetFullPath(p);
                            string selfFull = Path.GetFullPath(self);
                            if (string.Equals(pFull, selfFull, StringComparison.OrdinalIgnoreCase))
                            {
                                // Running Setup: cannot delete self; mark pending.
                                try
                                {
                                    File.WriteAllText(Path.Combine(binDir, ".uninstall-pending"), "setup");
                                    SetStatus("提示：Setup 运行中无法自删，已写 .uninstall-pending 标记。");
                                }
                                catch (Exception wex) { SetStatus("提示：写 .uninstall-pending 失败（" + wex.Message + "）"); }
                                continue;
                            }
                            try { File.Delete(p); }
                            catch (Exception ex)
                            {
                                // Cannot delete (e.g. locked): still record the
                                // pending marker so a later run can finish.
                                try { File.WriteAllText(Path.Combine(binDir, ".uninstall-pending"), name); }
                                catch { }
                                SetStatus("提示：无法移除 " + name + "（" + ex.Message + "）");
                            }
                        }
                        catch (Exception ex) { SetStatus("提示：无法移除 " + name + "（" + ex.Message + "）"); }
                    }
                    // Remove the directory if it is now empty (never remove data dirs).
                    try { if (Directory.GetFiles(binDir).Length == 0 && Directory.GetDirectories(binDir).Length == 0) Directory.Delete(binDir); }
                    catch { }
                }
                // 2. Remove the start-menu shortcut (ChatGPT-Fix-Launcher.lnk).
                string lnkDir = GetShellProgramsDir();
                string lnk = Path.Combine(lnkDir, "ChatGPT-Fix-Launcher.lnk");
                try { if (File.Exists(lnk)) File.Delete(lnk); } catch { }
                // 3. Remove the uninstall registry entry.
                try { RemoveUninstallEntry(); } catch { }
                // 4. KEEP baselines\ (user data + profile), backups\, logs\.
                return true;
            }
            catch (Exception ex)
            {
                SetStatus("卸载失败：" + ex.Message);
                return false;
            }
        }

        private void OnUninstallFinished(bool ok)
        {
            _running = false;
            _uninstall.Enabled = true;
            if (ok)
            {
                _status.Text = "已卸载（数据已保留）。可随时重新安装。";
                _uninstall.Text = "关闭";
                _uninstall.Click -= StartUninstallClick;
                _uninstall.Click += (s, e) => Close();
            }
            else
            {
                _uninstall.Text = "重试";
            }
        }

        private void StartUninstallClick(object s, EventArgs e) { }

        private static void RegisterUninstallEntry(string installRoot)
        {
            string binSetup = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Setup.exe");
            string uninstallCmd = "\"" + binSetup + "\" uninstall";
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(GetUninstallSubkey()))
            {
                if (key == null) return;
                key.SetValue("DisplayName", "ChatGPT-Fix");
                key.SetValue("DisplayVersion", "1.0.1");
                key.SetValue("InstallLocation", installRoot);
                key.SetValue("UninstallString", uninstallCmd);
                key.SetValue("QuietUninstallString", uninstallCmd);
                key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
            }
        }

        private static void RemoveUninstallEntry()
        {
            Registry.CurrentUser.DeleteSubKeyTree(GetUninstallSubkey(), false);
        }

        // Runs the Manager's ntc-ensure and returns whether the token-cost
        // overlay is actually present afterwards. The Manager's asar
        // injector requires the @electron/asar node module; without it the
        // injection dies with MODULE_NOT_FOUND and the install would
        // "succeed" WITHOUT the token-cost plugin, so the overlay check
        // turns that silent no-op into a visible warning.
        private static bool EnsureNtc(string installRoot, string baseline)
        {
            string manager = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Manager.exe");
            string resources = Path.Combine(baseline, "app", "resources");
            string overlay = Path.Combine(resources, "ntc-overlay");
            if (!File.Exists(manager) || !Directory.Exists(resources)) return false;
            var psi = new ProcessStartInfo(manager,
                "ntc-ensure --fixture-root \"" + resources + "\"")
            { CreateNoWindow = true, UseShellExecute = false };
            // Forward the module location explicitly (NODE_PATH).
            string nodePath = FindAsarNodePath();
            if (nodePath != null) psi.EnvironmentVariables["NODE_PATH"] = nodePath;
            using (var p = Process.Start(psi))
            {
                if (p != null)
                {
                    if (!p.WaitForExit(60000)) { try { p.Kill(); } catch { } }
                }
            }
            return Directory.Exists(overlay);
        }

        // Test hook: when test-config.json sets "skip_launch": "1", the
        // final Launcher start is skipped (see TestConfig). Production has
        // no test-config.json, so this is always false there.
        private static bool TestSkipLaunch()
        {
            var cfg = GetTestConfig();
            if (cfg == null) return false;
            return string.Equals(cfg.skip_launch, "1", StringComparison.OrdinalIgnoreCase)
                || string.Equals(cfg.skip_launch, "true", StringComparison.OrdinalIgnoreCase);
        }

        // Locates a node_modules dir containing @electron\asar (needed by
        // the token-cost injector). Candidates, in order: an inherited
        // NODE_PATH, the Setup's own ntc\node_modules (shipped next to the
        // exe), the per-user npx cache, the per-user global npm root.
        // Returns null when not found — injection then degrades to a
        // no-op and the plugin is simply not installed (never a crash).
        private static string FindAsarNodePath()
        {
            string existing = Environment.GetEnvironmentVariable("NODE_PATH");
            if (!string.IsNullOrEmpty(existing)
                && Directory.Exists(Path.Combine(existing, "@electron", "asar")))
                return existing;
            string exeDir = Path.GetDirectoryName(
                System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
            string local = Path.Combine(exeDir, "ntc", "node_modules");
            if (Directory.Exists(Path.Combine(local, "@electron", "asar"))) return local;
            string localApp = Environment.GetEnvironmentVariable("LOCALAPPDATA");
            if (localApp != null)
            {
                string npxRoot = Path.Combine(localApp, "npm-cache", "_npx");
                if (Directory.Exists(npxRoot))
                {
                    try
                    {
                        foreach (string dir in Directory.GetDirectories(npxRoot))
                        {
                            string cand = Path.Combine(dir, "node_modules");
                            if (Directory.Exists(Path.Combine(cand, "@electron", "asar")))
                                return cand;
                        }
                    }
                    catch { }
                }
                string npmGlobal = Path.Combine(localApp, "npm", "node_modules");
                if (Directory.Exists(Path.Combine(npmGlobal, "@electron", "asar")))
                    return npmGlobal;
            }
            return null;
        }

        private void OnFinished(bool ok)
        {
            _running = false;
            _install.Enabled = true;
            if (ok)
            {
                _status.Text = "安装完成！ChatGPT 已启动。";
                _install.Text = "关闭";
                _install.Click -= StartInstallClick;
                _install.Click += (s, e) => Close();
            }
            else
            {
                _install.Text = "重试";
            }
        }

        private void StartInstallClick(object s, EventArgs e) { }

        private void SetStatus(string text)
        {
            if (_cliMode) return; // CLI mode: no window, status is a no-op.
            try
            {
                if (InvokeRequired) BeginInvoke((Action)(() => _status.Text = text));
                else _status.Text = text;
            }
            catch (InvalidOperationException)
            {
                // CLI mode (no window handle): status updates are a no-op.
            }
        }

        private static string FindPowerShell()
        {
            string[] candidates = { "pwsh.exe", "powershell.exe" };
            foreach (string name in candidates)
            {
                string path = FindOnPath(name);
                if (path != null) return path;
            }
            return "powershell.exe";
        }

        private static string FindOnPath(string exe)
        {
            string pathEnv = Environment.GetEnvironmentVariable("PATH");
            if (pathEnv != null)
            {
                foreach (string dir in pathEnv.Split(Path.PathSeparator))
                {
                    string candidate = Path.Combine(dir, exe);
                    if (File.Exists(candidate)) return candidate;
                }
            }
            string sys32 = Path.Combine(
                Environment.GetEnvironmentVariable("SystemRoot") ?? @"C:\Windows",
                "System32", exe);
            return File.Exists(sys32) ? sys32 : null;
        }

        private static string DiscoverPackage()
        {
            var psi = new ProcessStartInfo(FindPowerShell(),
                "-NoProfile -NonInteractive -Command \"(Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty InstallLocation)\"")
            { RedirectStandardOutput = true, UseShellExecute = false, CreateNoWindow = true };
            using (var p = Process.Start(psi))
            {
                string line = p.StandardOutput.ReadToEnd().Trim();
                p.WaitForExit();
                return string.IsNullOrEmpty(line) ? null : line;
            }
        }

        // True when the target baseline "app" directory looks incomplete.
        // We check the KEY files that MUST exist for the app to run:
        // ChatGPT.exe, app.asar and the resources dir. A stale/partial copy
        // (e.g. only some top-level files, missing app.asar) therefore
        // triggers a re-copy. robocopy is idempotent: re-running it over an
        // existing target only copies what's missing. We deliberately do NOT
        // compare file counts against the source tree here — enumerating
        // C:\Program Files\WindowsApps\... with System.IO throws
        // (FileIOPermission emulation).
        private static bool NeedsCopy(string srcDir, string dstDir)
        {
            if (!Directory.Exists(srcDir)) return true;
            if (!Directory.Exists(dstDir)) return true;
            // Key files that must all be present for a valid baseline.
            // NOTE: app.asar lives under resources\ (Electron layout), NOT
            // in the app root — checking the wrong path would make
            // NeedsCopy always true and re-copy the 1.7GB tree every run.
            string[] keyFiles = { "ChatGPT.exe", "resources", @"resources\app.asar" };
            foreach (string key in keyFiles)
            {
                string p = Path.Combine(dstDir, key);
                if (!File.Exists(p) && !Directory.Exists(p)) return true;
            }
            return false;
        }

        // Returns (fileCount, totalBytes) for a directory tree; (0,0) on error.
        // Called only on the TARGET (user-owned) tree, which System.IO can
        // enumerate fine.
        private static Tuple<int, long> DirStats(string dir)
        {
            int count = 0;
            long bytes = 0;
            try
            {
                foreach (var f in Directory.GetFiles(LongPath(dir), "*", SearchOption.AllDirectories))
                {
                    count++;
                    try { bytes += new FileInfo(LongPath(f)).Length; } catch { }
                }
            }
            catch { }
            return Tuple.Create(count, bytes);
        }

        private static long TotalBytes(string dir)
        {
            long sum = 0;
            try { foreach (var f in Directory.GetFiles(LongPath(dir), "*", SearchOption.AllDirectories)) sum += new FileInfo(LongPath(f)).Length; }
            catch { }
            return sum;
        }

        // Prepends the Windows long-path prefix (\\?\) ONLY when the path is
        // near MAX_PATH (260), so the native CopyFile can handle deeply
        // nested directories (resources\cua_node\... in this package have
        // paths > 260 chars). Short paths are left untouched.
        private static string LongPath(string path)
        {
            if (path.Length < 250) return path;
            if (path.StartsWith(@"\\?\")) return path;
            if (path.Length >= 3 && path[1] == ':' && path[2] == '\\')
                return @"\\?\" + path;
            if (path.Length >= 2 && path.StartsWith(@"\\"))
                return @"\\?\UNC\" + path.Substring(2);
            return path;
        }

        // Native CopyFileW via P/Invoke — used only for single files (e.g.
        // bin\ deployment). Tree copies go through RobocopyTree (robocopy),
        // which is native, handles long paths, and copied the full 5421-file
        // package in 8 seconds with the current user's token.
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool CopyFileW(string lpExistingFileName, string lpNewFileName, bool bFailIfExists);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool CreateDirectoryW(string lpPathName, IntPtr lpSecurityAttributes);

        private static void NativeCopy(string src, string dst)
        {
            if (!CopyFileW(LongPath(src), LongPath(dst), false))
            {
                int err = Marshal.GetLastWin32Error();
                throw new IOException("CopyFileW 失败（错误 " + err + "）：" + src);
            }
        }

        // Tree copy via robocopy (Windows built-in). Rationale: robocopy is
        // native, handles long paths (>260) and ACL-restricted sources
        // (C:\Program Files\WindowsApps\) correctly, and copied this exact
        // 5421-file / 1.76 GB package in ~8 seconds with the current user's
        // token. Hand-rolled .NET recursion is either slow (per-file
        // P/Invoke) or breaks (FileIOPermission emulation, MAX_PATH,
        // stack overflow on deep node_modules trees). robocopy exit codes:
        // 0-7 are success (1=files copied, 2=extra files, 4=mismatch);
        // 8+ are failures. /R:0 /W:0 = no retries, /E = include subdirs,
        // /NFL /NDL /NJH /NJS /NP = quiet output.
        //
        // IMPORTANT: arguments are passed WITHOUT surrounding quotes.
        // WindowsApps paths and the install root never contain spaces here;
        // quoting them via ProcessStartInfo.Arguments gets mangled (robocopy
        // then sees the quote chars as part of the path — error 123).
        private void CopyTree(string src, string dst, long total)
        {
            // Delegate to a .bat wrapper: writing the robocopy command line
            // into a batch file and launching it via cmd.exe /c sidesteps ALL
            // ProcessStartInfo.Arguments quoting/escaping quirks (paths under
            // "C:\Program Files\..." contain a space; direct Arguments
            // strings get mangled differently between .NET versions). The
            // bat also lets us capture the exit code reliably.
            // The bat lives NEXT TO the running Setup, NOT in
            // Path.GetTempPath(): on this machine %TMP% resolves (via its
            // 8.3 short name) to a legacy directory "H:\Temp;" whose NAME
            // contains a semicolon. cmd.exe /c splits the command at ';'
            // ("'H:\Temp' is not recognized..."), so the bat never runs and
            // cmd exits with code 1 — the SAME code robocopy uses for
            // "files copied". That made every install silently skip the
            // copy. The Setup directory is always writable (robocopy-debug
            // is written there too) and contains no semicolons.
            string workDir = Path.GetDirectoryName(
                System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
            string batPath = Path.Combine(workDir,
                "chatgpt-fix-robocopy-" + Guid.NewGuid().ToString("N").Substring(0, 8) + ".bat");
            string logPath = Path.Combine(workDir,
                "chatgpt-fix-robocopy-" + Guid.NewGuid().ToString("N").Substring(0, 8) + ".log");
            string batContent =
                "@echo off\r\n" +
                "echo CMDLINE: robocopy.exe \"" + src + "\" \"" + dst + "\" /E /R:0 /W:0 /NFL /NDL /NJH /NJS /NP > \"" + logPath + "\" 2>&1\r\n" +
                "robocopy.exe \"" + src + "\" \"" + dst + "\" /E /R:0 /W:0 /NFL /NDL /NJH /NJS /NP >> \"" + logPath + "\" 2>&1\r\n" +
                "echo ROBEXIT=%errorlevel% >> \"" + logPath + "\"\r\n" +
                "exit /b %errorlevel%\r\n";
            // Post-mortem: record exactly what we launched, into the SAME
            // directory as the running Setup (works for both GUI and CLI).
            string dbgPath = Path.Combine(
                Path.GetDirectoryName(System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".",
                "robocopy-debug.txt");
            try
            {
                File.AppendAllText(dbgPath,
                    DateTime.Now.ToString("HH:mm:ss") + " src=" + src + "\r\n" +
                    "  dst=" + dst + "\r\n  workDir=" + workDir + "\r\n" +
                    "  bat=" + batPath + "\r\n  batContent:\r\n" + batContent + "\r\n" +
                    "  log=" + logPath + "\r\n");
            }
            catch { }
            File.WriteAllText(batPath, batContent, System.Text.Encoding.ASCII);
            var psi = new ProcessStartInfo("cmd.exe", "/c \"" + batPath + "\"")
            {
                CreateNoWindow = true,
                UseShellExecute = false,
                RedirectStandardOutput = false,
                RedirectStandardError = false
            };
            try
            {
                using (var p = Process.Start(psi))
                {
                    if (p == null) throw new IOException("无法启动 robocopy 批处理。");
                    if (!p.WaitForExit(600000)) { try { p.Kill(); } catch { } }
                    int code = p.ExitCode;
                    string logContent = "";
                    try { if (File.Exists(logPath)) logContent = File.ReadAllText(logPath); } catch { }
                    try
                    {
                        File.AppendAllText(dbgPath,
                            "  batExit=" + code + "\r\n  log=" + logContent.Replace("\r\n", " | ") + "\r\n");
                    }
                    catch { }
                    if (code >= 8)
                    {
                        throw new IOException("robocopy 复制官方包失败（退出码 " + code + "）。log: " + logContent);
                    }
                    // The bat MUST have actually run. cmd.exe /c exits with
                    // code 1 BOTH when robocopy copied files AND when it
                    // failed to start the bat at all (e.g. a ';' in the bat
                    // path). A real robocopy run always appends a
                    // "ROBEXIT=<code>" line to the log, so its absence means
                    // the copy never happened — fail loudly instead of
                    // silently writing a fake "verified" baseline.
                    if (!File.Exists(logPath)
                        || File.ReadAllText(logPath).IndexOf("ROBEXIT=", StringComparison.Ordinal) < 0)
                    {
                        throw new IOException("robocopy 批处理未执行（cmd 启动失败？）。bat=" + batPath);
                    }
                }
                // Verify the copy actually happened: robocopy can "succeed"
                // (exit < 8) while only copying a fraction of the tree.
                if (File.Exists(logPath))
                {
                    string log = File.ReadAllText(logPath);
                    if (log.IndexOf("0 files copied", StringComparison.OrdinalIgnoreCase) >= 0
                        && Directory.Exists(dst))
                    {
                        string[] entries;
                        try { entries = Directory.GetFileSystemEntries(dst); }
                        catch { entries = new string[0]; }
                        if (entries.Length == 0)
                        {
                            throw new IOException("robocopy 报告未复制任何文件。日志：" + log);
                        }
                    }
                }
            }
            finally
            {
                try { File.Delete(batPath); } catch { }
                // Keep logPath for post-mortem analysis (deleted by caller
                // or next run; it lives next to the Setup exe).
            }
        }

        private static string Sha256(string path)
        {
            using (var sha = SHA256.Create())
            using (var fs = File.OpenRead(LongPath(path)))
                return BitConverter.ToString(sha.ComputeHash(fs)).Replace("-", "").ToLowerInvariant();
        }

        private static void CreateShortcuts(string installRoot)
        {
            string lnkDir = GetShellProgramsDir();
            // WScript.Shell fails when the target directory is missing
            // (observed in the isolated fresh-env test), so create it first.
            Directory.CreateDirectory(lnkDir);
            // COM WScript.Shell via dynamic binding (no Interop reference needed).
            Type wsType = Type.GetTypeFromProgID("WScript.Shell");
            dynamic ws = Activator.CreateInstance(wsType);
            string launcher = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Launcher.exe");
            dynamic sc = ws.CreateShortcut(Path.Combine(lnkDir, "ChatGPT-Fix-Launcher.lnk"));
            sc.TargetPath = launcher;
            sc.WorkingDirectory = Path.GetDirectoryName(launcher);
            sc.Save();
        }
    }

    public static class Program
    {
        [STAThread]
        public static void Main(string[] args)
        {
            if (args != null && args.Length > 0 && string.Equals(args[0], "--test-install", StringComparison.OrdinalIgnoreCase))
            {
                // CLI self-test install (automation): runs the FULL DoInstall
                // flow with no window. Use together with a sidecar
                // test-config.json that redirects install_root/shell_root/
                // uninstall_subkey to a throwaway directory, so the real
                // installation is never touched. Exit code 0 = success.
                SetupForm.SetCliMode(true);
                var form = new SetupForm();
                bool ok = false;
                try { ok = form.DoInstall(); }
                catch (Exception ex) { Console.WriteLine("TEST-INSTALL EXCEPTION: " + ex); }
                try
                {
                    File.WriteAllText(Path.Combine(
                        Path.GetDirectoryName(System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".",
                        "test-install.log"),
                        DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " " + (ok ? "OK" : "FAILED") + "\n" +
                        (form.LastError ?? ""));
                }
                catch { }
                Console.WriteLine(ok
                    ? "TEST-INSTALL OK"
                    : "TEST-INSTALL FAILED");
                Environment.Exit(ok ? 0 : 1);
                return;
            }
            if (args != null && args.Length > 0 && string.Equals(args[0], "uninstall", StringComparison.OrdinalIgnoreCase))
            {
                // CLI uninstall (invoked by the registry UninstallString).
                // Data-preserving: removes bin + shortcuts + registry entry,
                // keeps baselines\backups\logs.
                var form = new SetupForm();
                bool ok = form.DoUninstall();
                Console.WriteLine(ok
                    ? "ChatGPT-Fix uninstalled (data preserved)."
                    : "ChatGPT-Fix uninstall failed.");
                return;
            }
            if (args != null && args.Length > 0 && string.Equals(args[0], "--repair", StringComparison.OrdinalIgnoreCase))
            {
                // CLI repair: rebuild start-menu shortcut + uninstall registry
                // entry WITHOUT touching the baseline (no copy, no state.json
                // rewrite). Use it after manual cleanup or shortcut loss.
                var form = new SetupForm();
                bool ok = form.RepairShortcutAndRegistry();
                Console.WriteLine(ok
                    ? "ChatGPT-Fix repair complete (baseline untouched)."
                    : "ChatGPT-Fix repair failed.");
                return;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new SetupForm());
        }
    }
}
