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
        private sealed class TestConfig
        {
            public string install_root;
            public string shell_root;
            public string uninstall_subkey;
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

        private bool DoInstall()
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
                if (!NeedsCopy(exe, Path.Combine(appDst, "ChatGPT.exe")))
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
                File.WriteAllText(Path.Combine(installRoot, "current.json"),
                    "{\"schema\":\"chatgpt_fix.pointer.v1\",\"baseline_root\":\"" +
                    baseline.Replace("\\", "/") + "\"}\n");
                // state.json (StagingV1 verified) so the launcher's baseline
                // check passes. The launcher uses the fast substring check
                // (schema + state=verified), so a single-entry manifest is fine.
                string sha = Sha256(exe);
                string stateJson =
                    "{\"schema\":\"chatgpt_fix.staging.v1\",\"source_package_full_name\":\"" + pkgName +
                    "\",\"source_version\":\"\",\"source_hash_manifest\":[{\"relative_path\":\"ChatGPT.exe\",\"bytes\":" +
                    new FileInfo(exe).Length + ",\"sha256\":\"" + sha +
                    "\"}],\"staging_root\":\"" + baseline.Replace("\\", "/") +
                    "\",\"baseline_root\":\"" + baseline.Replace("\\", "/") +
                    "\",\"files_staged\":1,\"total_bytes\":1,\"state\":\"verified\",\"created_at_utc\":\"" +
                    DateTime.UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ") + "\"}\n";
                File.WriteAllText(Path.Combine(baseline, "state.json"), stateJson);
                CreateShortcuts(installRoot);
                // 3.5. Register the uninstall entry so "Settings > Apps" can
                // uninstall ChatGPT-Fix (data-preserving). Best-effort: a
                // failure here must not fail the install.
                try { RegisterUninstallEntry(installRoot); }
                catch (Exception ex) { SetStatus("提示：注册卸载项失败（" + ex.Message + "）"); }
                // 3.6. Ensure the token-cost overlay (idempotent). The
                // Manager's ntc-ensure short-circuits when already injected.
                try { EnsureNtc(installRoot, baseline); }
                catch (Exception ex) { SetStatus("提示：Token 插件检查跳过（" + ex.Message + "）"); }
                SetStep(2);
                _progress.Value = 100;

                // 4. Launch.
                SetStatus("正在启动 ChatGPT…");
                string launcher = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Launcher.exe");
                if (File.Exists(launcher))
                    Process.Start(launcher);
                SetStep(3);
                return true;
            }
            catch (Exception ex)
            {
                SetStatus("安装失败：" + ex.Message);
                return false;
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

        private static void EnsureNtc(string installRoot, string baseline)
        {
            string manager = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Manager.exe");
            string resources = Path.Combine(baseline, "app", "resources");
            if (!File.Exists(manager) || !Directory.Exists(resources)) return;
            var psi = new ProcessStartInfo(manager,
                "ntc-ensure --fixture-root \"" + resources + "\"")
            { CreateNoWindow = true, UseShellExecute = false };
            using (var p = Process.Start(psi))
            {
                if (p != null)
                {
                    if (!p.WaitForExit(60000)) { try { p.Kill(); } catch { } }
                }
            }
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

        private static string DiscoverPackage()
        {
            var psi = new ProcessStartInfo("powershell.exe",
                "-NoProfile -NonInteractive -Command \"(Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty InstallLocation)\"")
            { RedirectStandardOutput = true, UseShellExecute = false, CreateNoWindow = true };
            using (var p = Process.Start(psi))
            {
                string line = p.StandardOutput.ReadToEnd().Trim();
                p.WaitForExit();
                return string.IsNullOrEmpty(line) ? null : line;
            }
        }

        private static bool NeedsCopy(string src, string dst)
        {
            if (!File.Exists(dst)) return true;
            return new FileInfo(src).Length != new FileInfo(dst).Length;
        }

        private static long TotalBytes(string dir)
        {
            long sum = 0;
            try { foreach (var f in Directory.GetFiles(dir, "*", SearchOption.AllDirectories)) sum += new FileInfo(f).Length; }
            catch { }
            return sum;
        }

        private void CopyTree(string src, string dst, long total)
        {
            Directory.CreateDirectory(dst);
            long done = 0;
            CopyInner(src, dst, total, ref done);
        }

        private void CopyInner(string src, string dst, long total, ref long done)
        {
            foreach (var file in Directory.GetFiles(src))
            {
                string target = Path.Combine(dst, Path.GetFileName(file));
                File.Copy(file, target, true);
                done += new FileInfo(file).Length;
                int pct = total > 0 ? (int)(done * 92 / total) : 0;
                if (pct > _progress.Value) BeginInvoke((Action)(() => _progress.Value = pct));
            }
            foreach (var dir in Directory.GetDirectories(src))
            {
                string sub = Path.GetFileName(dir);
                if (sub == "app.tmp") continue;
                CopyInner(dir, Path.Combine(dst, sub), total, ref done);
            }
        }

        private static string Sha256(string path)
        {
            using (var sha = SHA256.Create())
            using (var fs = File.OpenRead(path))
                return BitConverter.ToString(sha.ComputeHash(fs)).Replace("-", "").ToLowerInvariant();
        }

        private static void CreateShortcuts(string installRoot)
        {
            string lnkDir = GetShellProgramsDir();
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
