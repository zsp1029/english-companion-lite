param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\english-companion-lite.exe'), [switch]$ExportDiagnostics, [switch]$VisualQa)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class EclSettingsNative {
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern IntPtr FindWindow(string c,string t);
    public static IntPtr Find(string c) { return FindWindow(c,null); }
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h,int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll",CharSet=CharSet.Unicode,EntryPoint="SendMessageW")] public static extern IntPtr SetText(IntPtr h,uint message,IntPtr w,string text);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h,System.Text.StringBuilder text,int length);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
    [DllImport("user32.dll")] private static extern bool AttachThreadInput(uint a,uint b,bool attach);
    [DllImport("user32.dll")] private static extern IntPtr SetFocus(IntPtr h);
    public static void FocusTestControl(IntPtr h) {
        uint p; uint target=GetWindowThreadProcessId(h,out p);uint caller=GetCurrentThreadId();
        if(!AttachThreadInput(caller,target,true)) throw new InvalidOperationException("Cannot attach test input queue");
        try {SetFocus(h);} finally {AttachThreadInput(caller,target,false);}
    }
    [StructLayout(LayoutKind.Sequential)] public struct CursorPoint { public int X,Y; }
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out CursorPoint p);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll",CharSet=CharSet.Unicode,EntryPoint="SendMessageW")] public static extern IntPtr ReadText(IntPtr h,uint m,IntPtr count,System.Text.StringBuilder text);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int height,uint flags);
    [DllImport("user32.dll")] public static extern bool UpdateWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetUpdateRect(IntPtr h,out Rect r,bool erase);
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process,uint flags);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h,out Rect r);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
    [DllImport("user32.dll",EntryPoint="SendMessageW")] public static extern IntPtr DpiMessage(IntPtr h,uint m,IntPtr w,ref Rect r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
}
'@
function Assert-True($value,$message) { if (!$value) { throw $message }; Write-Output "PASS: $message" }
if ([EclSettingsNative]::Find('ECL.Manager') -ne [IntPtr]::Zero) { throw '请先退出当前英语伴学实例。' }
$root = Split-Path $PSScriptRoot -Parent
$profile = Join-Path $root ('artifacts\settings-profile-' + [guid]::NewGuid().ToString('N'))
$previous = $env:LOCALAPPDATA
$app = $null
$diagnosticNotepad = $null
if ($ExportDiagnostics -and @(Get-Process notepad -ErrorAction SilentlyContinue).Count -ne 0) {
    throw '诊断导出测试需要没有已打开记事本的起始状态，以免影响个人文档。'
}
try {
    $env:LOCALAPPDATA = $profile
    $app = Start-Process -FilePath (Resolve-Path $Exe) -PassThru
    $env:LOCALAPPDATA = $previous
    Start-Sleep -Milliseconds 900
    $h = [EclSettingsNative]::Find('ECL.Settings')
    Assert-True ($h -ne [IntPtr]::Zero -and [EclSettingsNative]::IsWindowVisible($h)) '首次启动自动显示原生设置'
    $windowName=New-Object Text.StringBuilder 256
    $null=[EclSettingsNative]::GetWindowText($h,$windowName,256)
    Assert-True ($windowName.ToString().Contains('英语伴学 Lite')) '隐藏视觉标题后仍保留系统可访问窗口名'
    $captionBounds=New-Object EclSettingsNative+Rect
    $clientBounds=New-Object EclSettingsNative+Rect
    $null=[EclSettingsNative]::GetWindowRect($h,[ref]$captionBounds)
    $null=[EclSettingsNative]::GetClientRect($h,[ref]$clientBounds)
    $captionY=$captionBounds.Top+[int](($captionBounds.Bottom-$captionBounds.Top-$clientBounds.Bottom)/2)
    $hitPoint=[IntPtr](($captionBounds.Left+220) -bor ($captionY -shl 16))
    Assert-True ([EclSettingsNative]::SendMessage($h,0x84,[IntPtr]::Zero,$hitPoint) -eq [IntPtr]2) '空白标题栏保留原生拖动命中区域'
    $null=[EclSettingsNative]::SendMessage($h,0x112,[IntPtr]0xF020,[IntPtr]::Zero)
    Assert-True ([EclSettingsNative]::IsIconic($h)) '保留原生最小化行为'
    $null=[EclSettingsNative]::SendMessage($h,0x112,[IntPtr]0xF120,[IntPtr]::Zero)
    Assert-True (![EclSettingsNative]::IsIconic($h) -and [EclSettingsNative]::IsWindowVisible($h)) '最小化后可以恢复设置窗口'
    $null=[EclSettingsNative]::SendMessage($h,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
    Assert-True (![EclSettingsNative]::IsWindowVisible($h)) '关闭设置仍然隐藏窗口并保留托盘'
    $null=[EclSettingsNative]::SendMessage([EclSettingsNative]::Find('ECL.Manager'),0x111,[IntPtr]1002,[IntPtr]::Zero)
    Assert-True ([EclSettingsNative]::IsWindowVisible($h)) '托盘命令可以重新打开极简标题栏窗口'
    Assert-True ([EclSettingsNative]::GetDlgItem($h,2103) -ne [IntPtr]::Zero) '设置提供导出诊断按钮'
    $configPath = Join-Path $profile 'EnglishCompanionLite\config.toml'
    $config = Get-Content $configPath -Raw
    foreach ($name in @('enabled','allow_notepad','allow_network','mock_mode')) {
        Assert-True ($config -match "(?m)^$name = false") "安全默认值 $name = false"
    }
    Assert-True ($config -match '(?m)^provider = "mymemory"') '新配置默认选择免费 MyMemory'
    $combo = [EclSettingsNative]::GetDlgItem($h,2211)
    Assert-True ([EclSettingsNative]::SendMessage($combo,0x147,[IntPtr]::Zero,[IntPtr]::Zero) -eq [IntPtr]0) '下拉框显示 MyMemory'
    $rect = New-Object EclSettingsNative+Rect
    $null = [EclSettingsNative]::GetWindowRect($h,[ref]$rect)
    $bmp = New-Object Drawing.Bitmap ($rect.Right-$rect.Left),($rect.Bottom-$rect.Top)
    $g = [Drawing.Graphics]::FromImage($bmp)
    $dc = $g.GetHdc()
    try { Assert-True ([EclSettingsNative]::PrintWindow($h,$dc,0)) '原生设置窗口可以离屏绘制' }
    finally { $g.ReleaseHdc($dc); $g.Dispose() }
    try { $bmp.Save((Join-Path $root 'artifacts\native-settings.png'),[Drawing.Imaging.ImageFormat]::Png) }
    finally { $bmp.Dispose() }
    # Exercise real native navigation, keeping edits while changing sections.
    $originalHash = (Get-FileHash -LiteralPath $configPath).Hash
    foreach ($section in @(@(2301,2202,'permissions'),@(2302,2205,'timing'),@(2303,2210,'diagnostics'),@(2300,2201,'general'))) {
        $null = [EclSettingsNative]::SendMessage([EclSettingsNative]::GetDlgItem($h,$section[0]),0x00F5,[IntPtr]::Zero,[IntPtr]::Zero)
        Assert-True ([EclSettingsNative]::IsWindowVisible([EclSettingsNative]::GetDlgItem($h,$section[1]))) "导航切换到 $($section[2])"
        $null = [EclSettingsNative]::GetWindowRect($h,[ref]$rect)
        $pageBmp = New-Object Drawing.Bitmap ($rect.Right-$rect.Left),($rect.Bottom-$rect.Top)
        $pageGraphics = [Drawing.Graphics]::FromImage($pageBmp)
        $pageDc = $pageGraphics.GetHdc()
        try { $null = [EclSettingsNative]::PrintWindow($h,$pageDc,0) }
        finally { $pageGraphics.ReleaseHdc($pageDc); $pageGraphics.Dispose() }
        try { $pageBmp.Save((Join-Path $root "artifacts\settings-$($section[2]).png"),[Drawing.Imaging.ImageFormat]::Png) }
        finally { $pageBmp.Dispose() }
    }
    Assert-True ((Get-FileHash -LiteralPath $configPath).Hash -eq $originalHash) '切换页面不会写配置或改变授权'
    if ($VisualQa) { & (Join-Path $PSScriptRoot 'settings-visual-qa.ps1') -Window $h -Root $root }
    $notepadToggle = [EclSettingsNative]::GetDlgItem($h,2202)
    $null = [EclSettingsNative]::SendMessage($notepadToggle,0x00F5,[IntPtr]::Zero,[IntPtr]::Zero)
    Assert-True ([EclSettingsNative]::SendMessage($notepadToggle,0x00F0,[IntPtr]::Zero,[IntPtr]::Zero) -eq [IntPtr]1) '自绘开关保留原生勾选行为'
    $null = [EclSettingsNative]::SendMessage($notepadToggle,0x00F5,[IntPtr]::Zero,[IntPtr]::Zero)
    $null = [EclSettingsNative]::SetText([EclSettingsNative]::GetDlgItem($h,2205),0x000C,[IntPtr]::Zero,'0')
    $null = [EclSettingsNative]::SendMessage($h,0x111,[IntPtr]2101,[IntPtr]::Zero)
    Assert-True ((Get-FileHash -LiteralPath $configPath).Hash -eq $originalHash) '非法数值不会写入配置'
    $validation = New-Object Text.StringBuilder 1024
    $null = [EclSettingsNative]::GetWindowText([EclSettingsNative]::GetDlgItem($h,2250),$validation,1024)
    Assert-True ($validation.ToString().Contains('范围')) '非法数值显示页内错误反馈'
    $null = [EclSettingsNative]::SetText([EclSettingsNative]::GetDlgItem($h,2205),0x000C,[IntPtr]::Zero,'300')
    if ($ExportDiagnostics) {
        $null = [EclSettingsNative]::PostMessage($h,0x111,[IntPtr]2103,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 1000
        $reports = @(Get-ChildItem -LiteralPath (Split-Path $configPath -Parent) -Filter 'diagnostics-*.txt')
        Assert-True ($reports.Count -eq 1) '点击导出生成完整诊断 TXT'
        $report = Get-Content -LiteralPath $reports[0].FullName -Raw -Encoding utf8
        Assert-True ($report.Contains('最近授权应用') -and $report.Contains('最近事件') -and $report.Contains('Mock=false')) '导出包含完整状态和生效模式'
        $ownedNotepads = @(Get-Process notepad -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 })
        Assert-True ($ownedNotepads.Count -eq 1) '导出自动打开本次诊断文件供查看'
        $diagnosticNotepad = $ownedNotepads[0]
    }
    foreach ($id in @(2201,2202,2204)) { $null = [EclSettingsNative]::SendMessage([EclSettingsNative]::GetDlgItem($h,$id),0x00F1,[IntPtr]1,[IntPtr]::Zero) }
    $null = [EclSettingsNative]::PostMessage($h,0x111,[IntPtr]2101,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 700
    $saved = Get-Content $configPath -Raw
    Assert-True ($saved -match '(?m)^enabled = true' -and $saved -match '(?m)^allow_notepad = true' -and $saved -match '(?m)^mock_mode = true' -and $saved -match '(?m)^allow_network = false') '原生设置保存和应用授权，联网仍关闭'
    $null = [EclSettingsNative]::SendMessage($combo,0x14E,[IntPtr]1,[IntPtr]::Zero)
    $null = [EclSettingsNative]::PostMessage($h,0x111,[IntPtr](2211 + 65536),[IntPtr]::Zero)
    $null = [EclSettingsNative]::PostMessage($h,0x111,[IntPtr]2101,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    Assert-True ((Get-Content $configPath -Raw) -match '(?m)^provider = "azure"') '设置可切换并保存 Azure'
    $notice = New-Object Text.StringBuilder 1024
    $null = [EclSettingsNative]::GetWindowText([EclSettingsNative]::GetDlgItem($h,2203),$notice,1024)
    Assert-True ($notice.ToString().Contains('Microsoft Azure Translator')) '联网授权文字显示实际选择的服务'
    $null = [EclSettingsNative]::SendMessage($combo,0x14E,[IntPtr]::Zero,[IntPtr]::Zero)
    $null = [EclSettingsNative]::SetText([EclSettingsNative]::GetDlgItem($h,2212),0x000C,[IntPtr]::Zero,'contact+test@example.com')
    $null = [EclSettingsNative]::PostMessage($h,0x111,[IntPtr]2101,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 300
    $saved = Get-Content $configPath -Raw
    Assert-True ($saved -match '(?m)^provider = "mymemory"' -and $saved.Contains('contact+test@example.com')) '可切回免费服务并保存可选联系邮箱'
    Assert-True ($saved -match '(?m)^allow_network = false') '切换服务不会自动开启联网授权'
    $null = [EclSettingsNative]::PostMessage([EclSettingsNative]::Find('ECL.Manager'),0x111,[IntPtr]1001,[IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    Assert-True ((Get-Content $configPath -Raw) -match '(?m)^enabled = false') '托盘暂停命令持久化配置'
} finally {
    $env:LOCALAPPDATA = $previous
    if ($diagnosticNotepad -and !$diagnosticNotepad.HasExited) {
        [void]$diagnosticNotepad.CloseMainWindow()
        Assert-True ($diagnosticNotepad.WaitForExit(5000)) '正常关闭本次诊断文件记事本'
    }
    if ($app -and !$app.HasExited) {
        $null = [EclSettingsNative]::PostMessage([EclSettingsNative]::Find('ECL.Manager'),0x111,[IntPtr]1004,[IntPtr]::Zero)
        Assert-True ($app.WaitForExit(10000)) '正常退出完成线程和窗口清理'
        Assert-True ([EclSettingsNative]::Find('ECL.Manager') -eq [IntPtr]::Zero) '退出后控制窗口已释放'
    }
}
