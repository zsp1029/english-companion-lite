# 第三方声明

依赖清单来自 `Cargo.lock` 对应的 Cargo 元数据，含其他 Windows 架构和仅编译期依赖，因此是保守的完整依赖清单。锁定版本与许可证文本一起保存。项目 MIT 许可证不覆盖这些依赖的独立版权声明。

| Crate | 版本 | SPDX 许可证表达式 | 原始许可证 |
| --- | --- | --- | --- |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | [声明](third-party/licenses/equivalent-1.0.3/) |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | [声明](third-party/licenses/hashbrown-0.17.1/) |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | [声明](third-party/licenses/indexmap-2.14.2/) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [声明](third-party/licenses/itoa-1.0.18/) |
| memchr | 2.8.3 | Unlicense OR MIT | [声明](third-party/licenses/memchr-2.8.3/) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [声明](third-party/licenses/proc-macro2-1.0.107/) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [声明](third-party/licenses/quote-1.0.47/) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [声明](third-party/licenses/serde-1.0.229/) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [声明](third-party/licenses/serde_core-1.0.229/) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [声明](third-party/licenses/serde_derive-1.0.229/) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | [声明](third-party/licenses/serde_json-1.0.154/) |
| serde_spanned | 0.6.9 | MIT OR Apache-2.0 | [声明](third-party/licenses/serde_spanned-0.6.9/) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [声明](third-party/licenses/syn-2.0.119/) |
| syn | 3.0.9 | MIT OR Apache-2.0 | [声明](third-party/licenses/syn-3.0.9/) |
| toml | 0.8.23 | MIT OR Apache-2.0 | [声明](third-party/licenses/toml-0.8.23/) |
| toml_datetime | 0.6.11 | MIT OR Apache-2.0 | [声明](third-party/licenses/toml_datetime-0.6.11/) |
| toml_edit | 0.22.27 | MIT OR Apache-2.0 | [声明](third-party/licenses/toml_edit-0.22.27/) |
| toml_write | 0.1.2 | MIT OR Apache-2.0 | [声明](third-party/licenses/toml_write-0.1.2/) |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | [声明](third-party/licenses/unicode-ident-1.0.27/) |
| windows | 0.58.0 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows-0.58.0/) |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_aarch64_gnullvm-0.52.6/) |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_aarch64_msvc-0.52.6/) |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_i686_gnu-0.52.6/) |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_i686_gnullvm-0.52.6/) |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_i686_msvc-0.52.6/) |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_x86_64_gnu-0.52.6/) |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_x86_64_gnullvm-0.52.6/) |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows_x86_64_msvc-0.52.6/) |
| windows-core | 0.58.0 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows-core-0.58.0/) |
| windows-implement | 0.58.0 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows-implement-0.58.0/) |
| windows-interface | 0.58.0 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows-interface-0.58.0/) |
| windows-result | 0.2.0 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows-result-0.2.0/) |
| windows-strings | 0.1.0 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows-strings-0.1.0/) |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | [声明](third-party/licenses/windows-targets-0.52.6/) |
| winnow | 0.7.15 | MIT | [声明](third-party/licenses/winnow-0.7.15/) |
| zmij | 1.0.23 | MIT | [声明](third-party/licenses/zmij-1.0.23/) |

`unicode-ident` 还包含 Unicode-3.0 数据许可；请保留其完整许可证。Rust 标准库与 GNU 工具链声明见下方工具链说明。

## 工具链与系统组件

本机使用 Rust GNU x64 工具链。Rust 标准库及 GNU/MinGW 可能带有独立的 MIT、Apache、BSD 类声明或运行库例外，不能仅凭项目 MIT 推断二进制全部组件的许可证。工具链内可取得的版权声明在 `third-party/toolchain/` 中保留。候选 EXE 的实际 DLL 导入列表和随包文件检查见本地发布审核记录。

Windows 的 Win32、UIA、GDI+、WinHTTP 等由操作系统提供，不随 ZIP 分发。不会打包 `.tools/` 内的编译器、链接器或 SDK。

已保留本机工具链随附的 GNU 运行库例外、GPLv3 和 Rust 标准库声明；`COPYRIGHT.html` 是整套编译器的额外本地参考，已忽略，不随项目分发。GNU README 说明了原始运行库源码来源。另附 MinGW-w64 官方固定提交的 COPYING 和 AUTHORS 作为参考，其具体版本尚未与本机工具链静态运行库逐一对应；不能把它描述为完成了逐目标文件的法律审计。该项目部分文件存在独立 BSD/LGPL 等例外，后续升级工具链需要重新复核。

## 资源来源

`assets/companion.svg` 为项目内几何绘制的气泡 E 图标源；ICO 来自本地生成脚本，未发现下载的图库、第三方字体或图片。生成记录已复制到 `scripts/create-icon.py`。静态检查不能证明所有素材的权属；发布者仍需确认没有复制无权授权的内容。

## 在线服务

MyMemory 和 Azure Translator 是外部服务，不受本项目 MIT 授权覆盖。需遵守服务条款、额度及隐私规则。

- [MyMemory API](https://mymemory.translated.net/doc/spec.php) / [服务条款](https://mymemory.translated.net/terms-and-conditions)

- [Azure Translator](https://learn.microsoft.com/en-us/azure/ai-services/translator/)
