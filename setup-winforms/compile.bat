@echo off
rem Build the .NET WinForms ChatGPT-Fix Setup installer.
rem Uses the .NET Framework compiler that ships with Windows (no SDK needed).
setlocal
set CSC=C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe
if not exist "%CSC%" set CSC=C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe
if not exist "%CSC%" (
  echo csc.exe not found
  exit /b 2
)
"%CSC%" /nologo /target:winexe /out:ChatGPT-Fix-Setup.exe /win32manifest:app.manifest SetupForm.cs
if %errorlevel%==0 (
  echo OK: ChatGPT-Fix-Setup.exe built.
) else (
  echo BUILD FAILED
)
endlocal
exit /b %errorlevel%
