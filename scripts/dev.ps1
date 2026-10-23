param([ValidateSet('test','check','build','release','fmt','clippy')][string]$Action = 'test')
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    # Prefer an installed toolchain; a project-local toolchain can be used without changing PATH globally.
    if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
        if (!(Test-Path '.tools\cargo\bin\cargo.exe')) { throw '请先按 README 安装 Rust Stable 和 C++ Build Tools。' }
        $env:RUSTUP_HOME = Join-Path $PWD '.tools\rustup'
        $env:CARGO_HOME = Join-Path $PWD '.tools\cargo'
        $env:PATH = "$PWD\.tools\cargo\bin;$env:PATH"
    }
    switch ($Action) {
        'release' { cargo build --locked --release }
        'fmt' { cargo fmt --all -- --check }
        'clippy' { cargo clippy --locked --all-targets -- -D warnings }
        default { cargo $Action --locked }
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo $Action failed ($LASTEXITCODE)" }
} finally { Pop-Location }
