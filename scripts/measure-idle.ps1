param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\english-companion-lite.exe'), [ValidateRange(5,60)][int]$Seconds = 15)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class EclMeasureNative {
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern IntPtr FindWindow(string c,string t);
    public static IntPtr Manager() { return FindWindow("ECL.Manager", null); }
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
}
'@
if ([EclMeasureNative]::Manager() -ne [IntPtr]::Zero) { throw '请先退出当前英语伴学实例，再测量独立进程。' }
$root = Split-Path $PSScriptRoot -Parent
$profile = Join-Path $root 'artifacts\measurement-profile'
$configDir = Join-Path $profile 'EnglishCompanionLite'
New-Item -ItemType Directory -Force -Path $configDir | Out-Null
$config = (Get-Content (Join-Path $root 'config.example.toml') -Raw).Replace('allow_notepad = false','allow_notepad = true').Replace('mock_mode = false','mock_mode = true')
Set-Content (Join-Path $configDir 'config.toml') -Value $config -Encoding utf8
$previous = $env:LOCALAPPDATA
$p = $null
$results = @()
try {
    $env:LOCALAPPDATA = $profile
    $p = Start-Process -FilePath (Resolve-Path $Exe) -PassThru
    $env:LOCALAPPDATA = $previous
    foreach ($mode in @('paused','enabled_idle')) {
        if ($mode -eq 'enabled_idle') { $null = [EclMeasureNative]::PostMessage([EclMeasureNative]::Manager(),0x111,[IntPtr]1001,[IntPtr]::Zero) }
        Start-Sleep -Seconds 2
        if ($p.HasExited -or [EclMeasureNative]::Manager() -eq [IntPtr]::Zero) { throw '测量实例未正常启动。' }
        $p.Refresh(); $cpuStart = $p.TotalProcessorTime.TotalMilliseconds
        $watch = [Diagnostics.Stopwatch]::StartNew()
        Start-Sleep -Seconds $Seconds
        $watch.Stop(); $p.Refresh()
        $results += [pscustomobject]@{
            mode = $mode; elapsed_seconds = [math]::Round($watch.Elapsed.TotalSeconds,3)
            cpu_ms = $p.TotalProcessorTime.TotalMilliseconds - $cpuStart
            cpu_one_core_percent = [math]::Round(($p.TotalProcessorTime.TotalMilliseconds-$cpuStart) / $watch.Elapsed.TotalMilliseconds * 100,4)
            working_set_bytes = $p.WorkingSet64; private_bytes = $p.PrivateMemorySize64
            thread_count = $p.Threads.Count; exe_bytes = (Get-Item $Exe).Length
        }
    }
    $output = Join-Path $root 'artifacts\resource-measurements.json'
    $results | ConvertTo-Json | Set-Content $output -Encoding utf8
    $results | Format-Table -AutoSize
} finally {
    $env:LOCALAPPDATA = $previous
    if ($p -and !$p.HasExited) {
        $null = [EclMeasureNative]::PostMessage([EclMeasureNative]::Manager(),0x111,[IntPtr]1004,[IntPtr]::Zero)
        if (!$p.WaitForExit(15000)) { Write-Warning '应用未及时退出，请检查托盘。' }
    }
}
