# Invoked by smoke-settings.ps1 in its isolated desktop session.
param([IntPtr]$Window, [string]$Root)
$ErrorActionPreference='Stop'
function Render-Control([IntPtr]$control) {
    $bounds=New-Object EclSettingsNative+Rect
    $frame=New-Object EclSettingsNative+Rect
    $null=[EclSettingsNative]::GetWindowRect($control,[ref]$bounds)
    $null=[EclSettingsNative]::GetWindowRect($Window,[ref]$frame)
    $full=New-Object Drawing.Bitmap ($frame.Right-$frame.Left),($frame.Bottom-$frame.Top)
    $graphics=[Drawing.Graphics]::FromImage($full)
    $dc=$graphics.GetHdc()
    try {$null=[EclSettingsNative]::PrintWindow($Window,$dc,0)}
    finally {$graphics.ReleaseHdc($dc);$graphics.Dispose()}
    try {
        $crop=New-Object Drawing.Rectangle ($bounds.Left-$frame.Left),($bounds.Top-$frame.Top),($bounds.Right-$bounds.Left),($bounds.Bottom-$bounds.Top)
        return $full.Clone($crop,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
    } finally {$full.Dispose()}
}

# Capture this test window through the compositor as well as PrintWindow:
# DWM caption colors are not represented by the legacy offscreen NC rendering.
try {
    $null=[EclSettingsNative]::SetWindowPos($Window,[IntPtr](-1),0,0,0,0,0x13)
    Start-Sleep -Milliseconds 100
    $screenBounds=New-Object EclSettingsNative+Rect
    $null=[EclSettingsNative]::GetWindowRect($Window,[ref]$screenBounds)
    $screenBitmap=New-Object Drawing.Bitmap ($screenBounds.Right-$screenBounds.Left),($screenBounds.Bottom-$screenBounds.Top)
    $screenGraphics=[Drawing.Graphics]::FromImage($screenBitmap)
    try {
        $screenGraphics.CopyFromScreen($screenBounds.Left,$screenBounds.Top,0,0,$screenBitmap.Size)
        $screenBitmap.Save((Join-Path $Root 'artifacts\minimal-caption-desktop.png'),[Drawing.Imaging.ImageFormat]::Png)
    } finally {$screenGraphics.Dispose();$screenBitmap.Dispose()}
} finally {$null=[EclSettingsNative]::SetWindowPos($Window,[IntPtr](-2),0,0,0,0,0x13)}

$nativeDpi=[EclSettingsNative]::GetDpiForWindow($Window)
$outer=New-Object EclSettingsNative+Rect
$client=New-Object EclSettingsNative+Rect
$null=[EclSettingsNative]::GetWindowRect($Window,[ref]$outer)
$null=[EclSettingsNative]::GetClientRect($Window,[ref]$client)
$frameWidth=$outer.Right-$outer.Left-$client.Right
$frameHeight=$outer.Bottom-$outer.Top-$client.Bottom
$toggle=[EclSettingsNative]::GetDlgItem($Window,2203)
$originalCheck=[EclSettingsNative]::SendMessage($toggle,0xF0,[IntPtr]::Zero,[IntPtr]::Zero)
try {
    foreach ($dpi in @($nativeDpi,120,144)) {
        $suggested=New-Object EclSettingsNative+Rect
        $suggested.Left=$outer.Left;$suggested.Top=$outer.Top
        $suggested.Right=$outer.Left+[int](920*$dpi/96)+$frameWidth
        $suggested.Bottom=$outer.Top+[int](650*$dpi/96)+$frameHeight
        $null=[EclSettingsNative]::DpiMessage($Window,0x2E0,[IntPtr]($dpi -bor ($dpi -shl 16)),[ref]$suggested)
        foreach ($state in @(0,1)) {
            $null=[EclSettingsNative]::SendMessage($toggle,0xF1,[IntPtr]$state,[IntPtr]::Zero)
            $bitmap=Render-Control $toggle
            try {
                $colors=New-Object 'System.Collections.Generic.HashSet[int]'
                $left=$bitmap.Width-[int][math]::Ceiling(50*$dpi/96)
                for($y=0;$y -lt $bitmap.Height;$y++) {
                    for($x=$left;$x -lt $bitmap.Width;$x++) {$null=$colors.Add($bitmap.GetPixel($x,$y).ToArgb())}
                }
                Assert-True ($colors.Count -gt 16) "DPI=$dpi 开关 state=$state 边缘有抗锯齿过渡（$($colors.Count) 色）"
                $bitmap.Save((Join-Path $Root "artifacts\toggle-$dpi-$state.png"),[Drawing.Imaging.ImageFormat]::Png)
            } finally {$bitmap.Dispose()}
        }
        $bitmap=Render-Control ([EclSettingsNative]::GetDlgItem($Window,2101))
        try {
            $minX=$bitmap.Width;$maxX=0;$minY=$bitmap.Height;$maxY=0
            $inset=[int](8*$dpi/96)
            for($y=$inset;$y -lt $bitmap.Height-$inset;$y++) {
                for($x=$inset;$x -lt $bitmap.Width-$inset;$x++) {
                    $color=$bitmap.GetPixel($x,$y)
                    if($color.R -gt 245 -and $color.G -gt 245 -and $color.B -gt 245) {
                        $minX=[math]::Min($minX,$x);$maxX=[math]::Max($maxX,$x)
                        $minY=[math]::Min($minY,$y);$maxY=[math]::Max($maxY,$y)
                    }
                }
            }
            $horizontal=[math]::Abs(($minX+$maxX)/2-($bitmap.Width-1)/2)
            $vertical=[math]::Abs(($minY+$maxY)/2-($bitmap.Height-1)/2)
            Assert-True ($minX -lt $maxX -and $horizontal -le 2 -and $vertical -le 2) "DPI=$dpi 保存文字居中（偏差 $horizontal / $vertical px）"
            $bitmap.Save((Join-Path $Root "artifacts\save-$dpi.png"),[Drawing.Imaging.ImageFormat]::Png)
        } finally {$bitmap.Dispose()}
        $bounds=New-Object EclSettingsNative+Rect
        $null=[EclSettingsNative]::GetWindowRect($Window,[ref]$bounds)
        $bitmap=New-Object Drawing.Bitmap ($bounds.Right-$bounds.Left),($bounds.Bottom-$bounds.Top)
        $graphics=[Drawing.Graphics]::FromImage($bitmap);$dc=$graphics.GetHdc()
        try {$null=[EclSettingsNative]::PrintWindow($Window,$dc,0)}
        finally {$graphics.ReleaseHdc($dc);$graphics.Dispose()}
        try {$bitmap.Save((Join-Path $Root "artifacts\settings-dpi-$dpi.png"),[Drawing.Imaging.ImageFormat]::Png)}
        finally {$bitmap.Dispose()}
    }
} finally {
    $null=[EclSettingsNative]::SendMessage($toggle,0xF1,$originalCheck,[IntPtr]::Zero)
    $null=[EclSettingsNative]::DpiMessage($Window,0x2E0,[IntPtr]($nativeDpi -bor ($nativeDpi -shl 16)),[ref]$outer)
}

# Real native button messages exercise mouse/keyboard focus and check semantics.
$bounds=New-Object EclSettingsNative+Rect
$null=[EclSettingsNative]::GetClientRect($toggle,[ref]$bounds)
$point=[IntPtr](($bounds.Right-20) -bor (($bounds.Bottom/2 -as [int]) -shl 16))
[EclSettingsNative]::FocusTestControl($toggle)
$null=[EclSettingsNative]::SendMessage($toggle,0x201,[IntPtr]1,$point)
$null=[EclSettingsNative]::SendMessage($toggle,0x202,[IntPtr]::Zero,$point)
$mouseCheck=[EclSettingsNative]::SendMessage($toggle,0xF0,[IntPtr]::Zero,[IntPtr]::Zero)
Assert-True ($mouseCheck -ne $originalCheck) '鼠标点击仍能切换原生开关'
$bitmap=Render-Control $toggle
try {
    $rowBlue=0
    for($y=1;$y -le 7;$y++) {
        for($x=2;$x -lt $bitmap.Width-60;$x++) {
            $c=$bitmap.GetPixel($x,$y)
            if($c.B -gt 170 -and $c.B-$c.R -gt 80){$rowBlue++}
        }
    }
    Assert-True ($rowBlue -eq 0) '鼠标点击开关不会出现整行焦点框'
    $bitmap.Save((Join-Path $Root 'artifacts\toggle-mouse-focus.png'),[Drawing.Imaging.ImageFormat]::Png)
} finally {$bitmap.Dispose()}
foreach($shortcutKey in @(16,91,83)) {
    $null=[EclSettingsNative]::SendMessage($toggle,0x100,[IntPtr]$shortcutKey,[IntPtr]::Zero)
    $null=[EclSettingsNative]::SendMessage($toggle,0x101,[IntPtr]$shortcutKey,[IntPtr]::Zero)
}
$bitmap=Render-Control $toggle
try {
    $shortcutFocus=0
    for($y=1;$y -lt $bitmap.Height-1;$y++) {
        for($x=$bitmap.Width-53;$x -lt $bitmap.Width-49;$x++) {
            $c=$bitmap.GetPixel($x,$y)
            if($c.B -gt 170 -and $c.B-$c.R -gt 80){$shortcutFocus++}
        }
    }
    Assert-True ($shortcutFocus -eq 0) '修饰键和截图快捷键不会误触发键盘焦点提示'
} finally {$bitmap.Dispose()}
$null=[EclSettingsNative]::SendMessage($toggle,0x100,[IntPtr]32,[IntPtr]::Zero)
$null=[EclSettingsNative]::SendMessage($toggle,0x101,[IntPtr]32,[IntPtr]::Zero)
$keyboardCheck=[EclSettingsNative]::SendMessage($toggle,0xF0,[IntPtr]::Zero,[IntPtr]::Zero)
Assert-True ($keyboardCheck -ne $mouseCheck) '键盘空格仍能切换开关'
$bitmap=Render-Control $toggle
try {
    $rowBlue=0;$localBlue=0
    for($y=1;$y -lt $bitmap.Height-1;$y++) {
        for($x=2;$x -lt $bitmap.Width-49;$x++) {
            $c=$bitmap.GetPixel($x,$y)
            if($c.B -gt 170 -and $c.B-$c.R -gt 80) {
                if($x -lt $bitmap.Width-60){if($y -le 7 -or $y -ge $bitmap.Height-8){$rowBlue++}}else{$localBlue++}
            }
        }
    }
    $bitmap.Save((Join-Path $Root 'artifacts\toggle-keyboard-focus.png'),[Drawing.Imaging.ImageFormat]::Png)
    Assert-True ($rowBlue -eq 0 -and $localBlue -gt 0) '键盘焦点只圈住开关附近，保留可访问提示'
    $bitmap.Save((Join-Path $Root 'artifacts\toggle-keyboard-focus.png'),[Drawing.Imaging.ImageFormat]::Png)
} finally {$bitmap.Dispose()}
$null=[EclSettingsNative]::SendMessage($toggle,0xF1,$originalCheck,[IntPtr]::Zero)

# Keep the actual pointer inside the test control while replaying movement;
# otherwise Windows correctly delivers WM_MOUSELEAVE for synthetic messages.
foreach($hoverTarget in @(@(2300,'导航项'),@(2203,'开关'))) {
$nav=[EclSettingsNative]::GetDlgItem($Window,$hoverTarget[0])
$cursor=New-Object EclSettingsNative+CursorPoint
$navBounds=New-Object EclSettingsNative+Rect
$null=[EclSettingsNative]::GetCursorPos([ref]$cursor)
$null=[EclSettingsNative]::GetWindowRect($nav,[ref]$navBounds)
$pointerX=$navBounds.Left+20;$pointerY=$navBounds.Top+20
try {
    $null=[EclSettingsNative]::SetWindowPos($Window,[IntPtr](-1),0,0,0,0,0x13)
    if(![EclSettingsNative]::SetCursorPos($pointerX,$pointerY)){throw 'Cannot position pointer in test control'}
    Start-Sleep -Milliseconds 80
    $null=[EclSettingsNative]::SendMessage($nav,0x200,[IntPtr]::Zero,[IntPtr]0x00100010)
    $null=[EclSettingsNative]::UpdateWindow($Window)
    $null=[EclSettingsNative]::UpdateWindow($nav)
    $redundant=0
    for($i=0;$i -lt 40;$i++) {
        $null=[EclSettingsNative]::SendMessage($nav,0x200,[IntPtr]::Zero,[IntPtr](0x00100010+$i))
        $invalid=New-Object EclSettingsNative+Rect
        if([EclSettingsNative]::GetUpdateRect($nav,[ref]$invalid,$false)){$redundant++}
        $null=[EclSettingsNative]::UpdateWindow($Window)
        $null=[EclSettingsNative]::UpdateWindow($nav)
    }
    Assert-True ($redundant -eq 0) "同一$($hoverTarget[1])内移动 40 次不反复重绘（$redundant 次额外失效）"
} finally {
    $null=[EclSettingsNative]::SetWindowPos($Window,[IntPtr](-2),0,0,0,0,0x13)
    $currentCursor=New-Object EclSettingsNative+CursorPoint
    $null=[EclSettingsNative]::GetCursorPos([ref]$currentCursor)
    if($currentCursor.X -eq $pointerX -and $currentCursor.Y -eq $pointerY) {
        $null=[EclSettingsNative]::SetCursorPos($cursor.X,$cursor.Y)
    }
}
$null=[EclSettingsNative]::SendMessage($nav,0x2A3,[IntPtr]::Zero,[IntPtr]::Zero)

}

$email=[EclSettingsNative]::GetDlgItem($Window,2212)
$null=[EclSettingsNative]::SetText($email,0xC,[IntPtr]::Zero,'draft+ui@example.com')
$pendingLabel=New-Object Text.StringBuilder 256
$null=[EclSettingsNative]::GetWindowText([EclSettingsNative]::GetDlgItem($Window,2250),$pendingLabel,256)
Assert-True ($pendingLabel.ToString().Contains('未保存')) '修改设置后显示明确的未保存主提示'
$bitmap=Render-Control $Window
try {$bitmap.Save((Join-Path $Root 'artifacts\feedback-pending.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
$numericField=[EclSettingsNative]::GetDlgItem($Window,2205)
$null=[EclSettingsNative]::SetText($numericField,0xC,[IntPtr]::Zero,'0')
$null=[EclSettingsNative]::SendMessage($Window,0x111,[IntPtr]2101,[IntPtr]::Zero)
$bitmap=Render-Control $Window
try {$bitmap.Save((Join-Path $Root 'artifacts\feedback-error.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
$null=[EclSettingsNative]::SetText($numericField,0xC,[IntPtr]::Zero,'300')
$beforeResources=[EclSettingsNative]::GetGuiResources($app.Handle,0)
for($i=0;$i -lt 12;$i++) {
    foreach($id in @(2301,2302,2303,2300)) {
        $null=[EclSettingsNative]::SendMessage([EclSettingsNative]::GetDlgItem($Window,$id),0xF5,[IntPtr]::Zero,[IntPtr]::Zero)
        $null=[EclSettingsNative]::UpdateWindow($Window)
    }
}
$afterResources=[EclSettingsNative]::GetGuiResources($app.Handle,0)
$draft=New-Object Text.StringBuilder 256
$null=[EclSettingsNative]::ReadText($email,0xD,[IntPtr]256,$draft)
Assert-True ($draft.ToString() -eq 'draft+ui@example.com') '快速往返切页保留未保存输入'
Assert-True ($afterResources-$beforeResources -le 8) "反复切页后 GDI 对象数稳定（增量 $($afterResources-$beforeResources)）"
$null=[EclSettingsNative]::SetText($email,0xC,[IntPtr]::Zero,'')
