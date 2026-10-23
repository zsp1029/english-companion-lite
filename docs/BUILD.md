# Windows 源码构建与打包

## 标准构建

1. 安装 Git 和 Rust Stable x64。
2. 推荐 MSVC：安装 Visual Studio Build Tools 的“使用 C++ 的桌面开发”、MSVC x64 与 Windows SDK，然后重新打开终端。
3. 获取源码后在项目根目录运行：

```powershell
cargo test --locked
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

产物为 `target\release\english-companion-lite.exe`。首次下载依赖需要联网。Windows 系统组件在运行时由系统提供，不能据此保证任意工具链构建的 EXE 都没有其他 DLL 依赖。

`build.rs` 直接生成资源文件，读取 `assets/companion.ico` 和 `assets/app.manifest`。不要省略这些资源。GNU 分支资源对象针对 x64，当前仅支持发布 Windows x64；不要按 ARM64/x86 发布。

## 本地开发脚本

`scripts/dev.ps1` 优先使用 PATH 中的 Cargo；若没有，则尝试项目本地 `.tools/cargo/bin`。`.tools` 不随源码发布，普通开发者应自行安装工具链。

```powershell
.\scripts\dev.ps1 -Action test
.\scripts\dev.ps1 -Action fmt
.\scripts\dev.ps1 -Action clippy
.\scripts\dev.ps1 -Action release
```

当前机器使用 GNU x64 / Rust 1.99.1 验证，最低 Rust 1.85 和 MSVC 未单独验证。不需要 Python 构建；`scripts/create-icon.py` 仅用于重新生成图标，需 Python 3 且会改写两个图标文件，请仅在有意修改图标时运行。

## 独立构建，不覆盖现有 EXE

```powershell
$env:CARGO_TARGET_DIR = Join-Path $PWD 'artifacts\open-source-prep\build'
$env:RUSTFLAGS = "--remap-path-prefix=$($PWD.Path)=."
.\scripts\dev.ps1 -Action release
Remove-Item Env:CARGO_TARGET_DIR
Remove-Item Env:RUSTFLAGS
```

产物位于 `artifacts/open-source-prep/build/release/`。路径映射避免将工作区绝对路径写入编译错误位置等字符串。本地工具和测试结果始终被 Git 忽略。请在独立终端使用上述示例，避免覆盖已有环境变量。

## 创建发布候选包

```powershell
.\scripts\package-release.ps1 -Exe .\artifacts\open-source-prep\build\release\english-companion-lite.exe
```

脚本不编译、不启动应用、不读取个人配置、不上传。版本来自 `Cargo.toml`，创建 `release-dist/english-companion-lite-v0.1.14-windows-x64.zip` 和 `SHA256SUMS.txt`。已存在的同名目录或 ZIP 会导致停止，避免覆盖。为其他 EXE 打包前，维护者必须确认它对应当前源码版本、Windows x64、必需 DLL 和验证记录。

包内包含 EXE、配置示例、双语说明、许可证、依赖声明和构建/验证文档，不包含测试截图、个人配置或编译工具。可用 `-OutputDirectory .\release-dist\final` 创建新候选，保留已有候选。发布前检查 DLL 导入，确保需要的非系统 DLL 已正确处理；如改用其他工具链，请重新检查运行库许可。

上传前先审阅本地文件清单与敏感信息审计。仓库创建、push 和 Release 上传需要用户最终确认。操作说明见 `docs/GITHUB_PUBLISH.md`。
