param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\english-companion-lite.exe'), [switch]$LiveMymemory)
$ErrorActionPreference = 'Stop'
# This test owns a new blank Notepad window and an isolated application configuration.
# It exercises real UIA events with programmatic edits. It does NOT verify Microsoft Pinyin.
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class EclSmokeNative {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
    public static IntPtr ByClass(string c) { return FindWindow(c, null); }
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder b, int n);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h,int id);
    public static uint ForegroundPid() { uint p; GetWindowThreadProcessId(GetForegroundWindow(), out p); return p; }
}
'@
if ([EclSmokeNative]::ByClass('ECL.Manager') -ne [IntPtr]::Zero) { throw '请先退出正在运行的英语伴学，以免影响个人会话。' }
$root = Split-Path $PSScriptRoot -Parent
$profile = Join-Path $root 'artifacts\smoke-profile'
$configDir = Join-Path $profile 'EnglishCompanionLite'
New-Item -ItemType Directory -Force -Path $configDir | Out-Null
$config = Get-Content (Join-Path $root 'config.example.toml') -Raw
$config = $config.Replace('enabled = false','enabled = true').Replace('allow_notepad = false','allow_notepad = true').Replace('mock_mode = false','mock_mode = true')
Set-Content (Join-Path $configDir 'config.toml') -Value $config -Encoding utf8
$document = Join-Path $profile 'uia-smoke.txt'
[IO.File]::WriteAllText($document,'')
$previous = $env:LOCALAPPDATA
$previousKey = $env:AZURE_TRANSLATOR_KEY
$before = @(Get-Process notepad -ErrorAction SilentlyContinue | ForEach-Object Id)
if ($before.Count -ne 0) { throw '此测试需要独立记事本进程。请先保存并关闭所有记事本窗口，再运行测试。' }
$owned = @()
$app = $null
function Assert-True($value, $message) { if (!$value) { throw $message }; Write-Output "PASS: $message" }
function Overlay-Visible { $h = [EclSmokeNative]::ByClass('ECL.Overlay'); return $h -ne [IntPtr]::Zero -and [EclSmokeNative]::IsWindowVisible($h) }
function Save-WindowImage([IntPtr]$h, [string]$file) {
    $rect = New-Object EclSmokeNative+Rect
    if (![EclSmokeNative]::GetWindowRect($h,[ref]$rect)) { throw '无法读取测试窗口矩形。' }
    $bmp = New-Object Drawing.Bitmap ($rect.Right-$rect.Left),($rect.Bottom-$rect.Top)
    $g = [Drawing.Graphics]::FromImage($bmp)
    try { $g.CopyFromScreen($rect.Left,$rect.Top,0,0,$bmp.Size); $bmp.Save($file,[Drawing.Imaging.ImageFormat]::Png) }
    finally { $g.Dispose(); $bmp.Dispose() }
}
try {
    $env:LOCALAPPDATA = $profile
    $env:AZURE_TRANSLATOR_KEY = '' # Missing-key test must never send a real request.
    $app = Start-Process -FilePath (Resolve-Path $Exe) -PassThru
    $env:LOCALAPPDATA = $previous
    $env:AZURE_TRANSLATOR_KEY = $previousKey
    Start-Sleep -Milliseconds 600
    Assert-True ([EclSmokeNative]::ByClass('ECL.Manager') -ne [IntPtr]::Zero) '程序控制窗口实际创建'
    Assert-True ([EclSmokeNative]::ByClass('ECL.Overlay') -ne [IntPtr]::Zero) '程序悬浮窗口实际创建'
    $null = Start-Process notepad.exe -ArgumentList "`"$document`"" -PassThru
    Start-Sleep -Seconds 2
    $owned = @(Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $_.Id -notin $before -and $_.MainWindowHandle -ne 0 })
    if ($owned.Count -ne 1) { throw '无法唯一确定新建记事本进程；请关闭测试窗口后重试。' }
    $window = [Windows.Automation.AutomationElement]::FromHandle($owned[0].MainWindowHandle)
    $condition = New-Object Windows.Automation.PropertyCondition([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Document)
    $editor = $window.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
    if (!$editor) { throw '记事本没有 UIA Document 编辑器。可运行 examples/uia_probe。' }
    $null = [EclSmokeNative]::SetForegroundWindow($owned[0].MainWindowHandle)
    $editor.SetFocus()
    Start-Sleep -Milliseconds 800
    Assert-True ([EclSmokeNative]::ForegroundPid() -eq $owned[0].Id) "测试记事本确实处于前台（test PID $($owned[0].Id), foreground PID $([EclSmokeNative]::ForegroundPid())）"
    Assert-True (!(Overlay-Visible)) '焦点进入空白编辑器不弹提示'
    $editHwnd = [IntPtr]$editor.Current.NativeWindowHandle
    if ($editHwnd -eq [IntPtr]::Zero) { throw '此记事本未公开原生编辑 HWND，自动编辑测试跳过；请人工输入。' }
    $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'今天工作比较忙，晚上想早点休息。') # EM_REPLACESEL
    $textPattern = $editor.GetCurrentPattern([Windows.Automation.TextPattern]::Pattern)
    Assert-True ($textPattern.DocumentRange.GetText(100).Contains('今天工作比较忙，晚上想早点休息。')) '测试文字确实进入本次新建记事本编辑器'
    Start-Sleep -Milliseconds 1000
    Assert-True (Overlay-Visible) '实际文本变化经 UIA 触发 Mock 悬浮窗'
    $caption = New-Object Text.StringBuilder 1024
    $null = [EclSmokeNative]::GetWindowText([EclSmokeNative]::ByClass('ECL.Overlay'),$caption,1024)
    Assert-True ($caption.ToString().Contains('[MOCK / 测试模式]')) '悬浮窗明确标记 Mock'
    $foregroundPid = [EclSmokeNative]::ForegroundPid()
    $sameEditor = [Windows.Automation.Automation]::Compare($editor,[Windows.Automation.AutomationElement]::FocusedElement)
    Assert-True ($foregroundPid -eq $owned[0].Id -and $sameEditor) "悬浮窗不抢占输入焦点（test PID $($owned[0].Id), foreground PID $foregroundPid, same editor $sameEditor）"
    Save-WindowImage ([EclSmokeNative]::ByClass('ECL.Overlay')) (Join-Path $root 'artifacts\mock-overlay.png')
    $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'我')
    Start-Sleep -Milliseconds 250
    Assert-True (!(Overlay-Visible)) '继续输入隐藏旧提示'
    $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'明天晚上还想继续学习英语')
    Start-Sleep -Milliseconds 700
    Assert-True (!(Overlay-Visible)) '无标点输入不会过早触发'
    Start-Sleep -Milliseconds 1100
    Assert-True (Overlay-Visible) '无标点停顿后真实 UIA 链路触发'
    $popup = [EclSmokeNative]::ByClass('ECL.Overlay')
    $bodyHwnd = [EclSmokeNative]::GetDlgItem($popup,3101)
    Assert-True ($bodyHwnd -ne [IntPtr]::Zero) '提示包含可选择复制的原生文本控件'
    # Activate only our test popup, as Windows does for an intentional mouse click.
    # Do not invoke Copy or modify the clipboard during the integration test.
    $null = [EclSmokeNative]::SetForegroundWindow($popup)
    Start-Sleep -Milliseconds 500
    Assert-True ((Overlay-Visible) -and [EclSmokeNative]::ForegroundPid() -eq $app.Id) '主动激活提示后 UIA 焦点切换不会隐藏提示'
    $null = [EclSmokeNative]::PostMessage([EclSmokeNative]::ByClass('ECL.Manager'),0x111,[IntPtr]1001,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 400
    Assert-True (!(Overlay-Visible)) '托盘暂停命令隐藏提示'
    $null = [EclSmokeNative]::SetForegroundWindow($owned[0].MainWindowHandle)
    $editor.SetFocus()
    Start-Sleep -Milliseconds 200
    $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'暂停后继续输入不会触发翻译。')
    Start-Sleep -Milliseconds 1800
    Assert-True (!(Overlay-Visible)) '暂停后不触发'
    # Explicitly authorize only our tool's EXE; Notepad is now outside the whitelist.
    $notAllowed = $config.Replace('allow_notepad = true','allow_notepad = false').Replace('allowed_executables = []',"allowed_executables = ['$( (Resolve-Path $Exe).Path )']")
    Set-Content (Join-Path $configDir 'config.toml') -Value $notAllowed -Encoding utf8
    $null = [EclSmokeNative]::PostMessage([EclSmokeNative]::ByClass('ECL.Manager'),0x111,[IntPtr]1003,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 600
    $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'白名单以外的应用不应触发翻译。')
    Start-Sleep -Milliseconds 800
    Assert-True (!(Overlay-Visible)) '非白名单进程不触发'
    $diagnostic = New-Object Text.StringBuilder 1024
    $null = [EclSmokeNative]::GetWindowText([EclSmokeNative]::ByClass('ECL.Manager'),$diagnostic,1024)
    Assert-True ($diagnostic.ToString().Contains('未授权')) '诊断明确标记当前进程未授权'
    $noNetwork = $config.Replace('mock_mode = true','mock_mode = false')
    Set-Content (Join-Path $configDir 'config.toml') -Value $noNetwork -Encoding utf8
    $null = [EclSmokeNative]::PostMessage([EclSmokeNative]::ByClass('ECL.Manager'),0x111,[IntPtr]1003,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 600
    Assert-True (!(Overlay-Visible)) '重新进入授权控件不翻译已有内容'
    $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'没有联网授权也没有选择测试模式。')
    Start-Sleep -Milliseconds 800
    $diagnostic.Clear() | Out-Null
    $null = [EclSmokeNative]::GetWindowText([EclSmokeNative]::ByClass('ECL.Manager'),$diagnostic,1024)
    Assert-True ($diagnostic.ToString().Contains('联网翻译未授权')) '联网未授权时不调用服务'
    $missingKey = $noNetwork.Replace('allow_network = false','allow_network = true').Replace("provider = 'mymemory'", "provider = 'azure'")
    Set-Content (Join-Path $configDir 'config.toml') -Value $missingKey -Encoding utf8
    $null = [EclSmokeNative]::PostMessage([EclSmokeNative]::ByClass('ECL.Manager'),0x111,[IntPtr]1003,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 600
    $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'缺少翻译密钥时程序应当保持稳定。')
    Start-Sleep -Milliseconds 800
    $diagnostic.Clear() | Out-Null
    $null = [EclSmokeNative]::GetWindowText([EclSmokeNative]::ByClass('ECL.Manager'),$diagnostic,1024)
    Assert-True ($diagnostic.ToString().Contains('未设置 AZURE_TRANSLATOR_KEY')) '缺少 API Key 安全失败且不联网'
    Assert-True (!$app.HasExited) '错误场景后进程保持运行'
    if ($LiveMymemory) {
        # Explicit opt-in: only fixed synthetic text in this owned Notepad is sent online.
        $liveConfig = $noNetwork.Replace('allow_network = false','allow_network = true')
        Set-Content (Join-Path $configDir 'config.toml') -Value $liveConfig -Encoding utf8
        $null = [EclSmokeNative]::PostMessage([EclSmokeNative]::ByClass('ECL.Manager'),0x111,[IntPtr]1003,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 600
        $null = [EclSmokeNative]::SendMessage($editHwnd,0x00C2,[IntPtr]::Zero,'今天工作比较忙，晚上想早点休息。')
        for ($attempt = 0; $attempt -lt 50 -and !(Overlay-Visible); $attempt++) { Start-Sleep -Milliseconds 300 }
        Assert-True (Overlay-Visible) '固定中文句通过真实 UIA、MyMemory 免费接口到英文提示'
        $liveText = New-Object Text.StringBuilder 4096
        $null = [EclSmokeNative]::GetWindowText([EclSmokeNative]::ByClass('ECL.Overlay'),$liveText,4096)
        Assert-True (!$liveText.ToString().Contains('[MOCK') -and $liveText.ToString() -match '(?i)work|busy|rest|evening') '实际英文译文包含原句含义且不是 Mock'
        Write-Output ('Live translation: ' + $liveText.ToString())
    }
    $null = [EclSmokeNative]::PostMessage([EclSmokeNative]::ByClass('ECL.Manager'),0x111,[IntPtr]1002,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    Assert-True ([EclSmokeNative]::IsWindowVisible([EclSmokeNative]::ByClass('ECL.Settings'))) '原生设置窗口可打开'
    Save-WindowImage ([EclSmokeNative]::ByClass('ECL.Settings')) (Join-Path $root 'artifacts\native-settings.png')
} catch {
    $diagnostic = New-Object Text.StringBuilder 1024
    $null = [EclSmokeNative]::GetWindowText([EclSmokeNative]::ByClass('ECL.Manager'),$diagnostic,1024)
    Write-Output "Diagnostic: $diagnostic"
    throw
} finally {
    $env:LOCALAPPDATA = $previous
    $env:AZURE_TRANSLATOR_KEY = $previousKey
    if ($app -and !$app.HasExited) {
        $null = [EclSmokeNative]::PostMessage([EclSmokeNative]::ByClass('ECL.Manager'),0x111,[IntPtr]1004,[IntPtr]::Zero)
        if (!$app.WaitForExit(15000)) { Write-Warning '测试应用未及时退出，请通过托盘退出。' }
    }
    # The script starts with no existing Notepad. Only its newly created test process is closed.
    if ($owned.Count -eq 1) {
        [void]$owned[0].CloseMainWindow()
        if (!$owned[0].WaitForExit(1500)) { $owned[0].Kill(); $owned[0].WaitForExit() }
    }
}

