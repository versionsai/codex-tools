# Codex Tools

Codex Tools 是一个面向 Codex 的跨平台桌面辅助工具。它的目标很明确：只服务 Codex 的 Provider 切换、会话归并和线程同步，不试图做成一个泛用的 AI CLI 管理器。

当前版本基于 Tauri 2、React、TypeScript 和 Rust 构建，计划支持 macOS 与 Windows。

## 核心功能

- Provider 管理：在可视化界面中维护多个 Codex Provider。
- 一键切换 Provider：切换时写入当前用户的 Codex 配置文件。
- 线程 Provider 归并：切换 Provider 后自动统一本地线程中的 `model_provider`，避免历史会话因为 Provider 变化而不可见。
- 官方 ChatGPT 登录模式：内置 `openai` Provider，用于 Codex 官方账号登录模式，不覆盖 Codex 内建 Provider。
- 三方 API Key 模式：支持为自定义 Provider 写入 `config.toml` 和 `auth.json`。
- WebDAV 配置入口：用于线程同步能力的配置管理。
- macOS 状态栏模式：启动后常驻系统状态栏，不占用程序坞；从状态栏可直接切换 Provider。

## 配置文件位置

Codex Tools 不写死任何用户机器上的绝对路径。

默认情况下，它会读取当前用户目录下的 Codex 配置：

```text
~/.codex
```

在 Windows 上通常对应：

```text
C:\Users\<用户名>\.codex
```

如果你使用了自定义 Codex 目录，可以通过 `CODEX_HOME` 指定：

```bash
export CODEX_HOME="/path/to/.codex"
```

```powershell
$env:CODEX_HOME="D:\Codex\.codex"
```

Provider 快照会保存在应用自己的配置目录中，例如 macOS 下通常是：

```text
~/Library/Application Support/codex-tools/providers.json
```

Windows 下通常是：

```text
%APPDATA%\codex-tools\providers.json
```

## Provider 逻辑

Codex Tools 会区分两类 Codex 使用方式。

官方 ChatGPT 登录模式：

- Provider ID 固定为 `openai`。
- 通过 Codex 官方登录 token 工作。
- 不会写入 `[model_providers.openai]`，避免覆盖 Codex 内建 Provider。
- `auth.json` 保留 Codex 官方登录需要的 `auth_mode` 和 `tokens` 信息。

三方 API Key 模式：

- Provider ID 由用户自定义，但不能重复。
- `auth.json` 使用 `OPENAI_API_KEY`。
- `config.toml` 写入 `model_provider`、`model`、`model_reasoning_effort` 和对应的 `[model_providers.<id>]`。
- 默认使用 `wire_api = "responses"`，并开启 `disable_response_storage = true`。

切换 Provider 时，工具会先保存当前 Provider 的配置快照，再应用目标 Provider，并执行线程 Provider 归并。

## 与 cc-switch、codex- 的关系

这个项目不是凭空发明的，也不应该抹掉前人工作的价值。

Codex Tools 在设计和实现时参考了以下项目：

- `cc-switch`：主要借鉴了 Provider 管理、Provider 切换、状态栏常驻、快速切换入口等交互和产品思路。
- `codex-`：主要借鉴了 Codex 会话与线程同步相关的方向，包括通过 WebDAV 推送、拉取线程，以及本地多 Provider 线程归并的需求。

Codex Tools 的定位是在这些成熟思路基础上，做一个更聚焦 Codex 的桌面工具。项目不会刻意抹除或替代原项目的功能边界；如果你需要更完整的 Claude Code / 多 CLI 切换能力，请优先查看 `cc-switch`；如果你关注 Codex 线程同步的原始实现方向，也建议查看 `codex-`。

如果后续发布正式开源版本，建议在仓库说明、Release 文档和许可证信息中继续保留对这两个项目的致谢与说明。

## 开发环境

需要安装：

- Node.js
- npm
- Rust 工具链
- macOS 打包需要 Xcode Command Line Tools
- Windows 打包建议在 Windows 环境安装 Visual Studio Build Tools

安装依赖：

```bash
npm install
```

只运行前端：

```bash
npm run dev
```

运行 Tauri 开发版：

```bash
npm run tauri dev
```

## 构建

构建前端：

```bash
npm run build
```

构建桌面应用：

```bash
npm run tauri build
```

macOS 通常会产出：

```text
src-tauri/target/release/codex-tools
src-tauri/target/release/bundle/macos/Codex Tools.app
src-tauri/target/release/bundle/dmg/Codex Tools_0.1.0_aarch64.dmg
```

Windows 通常会产出：

```text
src-tauri/target/release/codex-tools.exe
src-tauri/target/release/bundle/nsis/*.exe
src-tauri/target/release/bundle/msi/*.msi
```

面向普通用户分发时，建议：

- macOS 使用 `.dmg` 或压缩后的 `.app`。
- Windows 使用 NSIS `.exe` 安装包。

## 当前状态

已完成：

- Provider 列表、编辑、新建、删除。
- Provider 唯一性校验。
- 官方 `openai` Provider 固定保留。
- 三方 API Key Provider 配置写入。
- 切换 Provider 时自动合并线程 Provider。
- 从 Codex 当前配置反向同步到当前 Provider 详情页。
- macOS 状态栏常驻与状态栏 Provider 快速切换。
- WebDAV 配置保存与读取。

仍需完善：

- WebDAV 推送线程到远端。
- WebDAV 拉取远端线程。
- 冲突检测与合并预览。
- Windows 安装包 CI 验证。
- 第三方项目致谢与依赖许可证整理。

## 风险提示

Codex Tools 会读写当前用户的 Codex 配置文件：

```text
~/.codex/config.toml
~/.codex/auth.json
```

建议在正式使用前先备份 `.codex` 目录，尤其是已有多个 Provider、多个设备同步或长期会话历史的用户。

## 许可证

本项目采用 MIT License，详见仓库根目录的 `LICENSE` 文件。
