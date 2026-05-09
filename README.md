# Codex Sync / Codex Tools

这个仓库包含两个相互关联、但定位不同的 Codex 桌面工具：

- `Codex Sync`：SwiftUI macOS App，专注 WebDAV 线程同步与本地线程 Provider 归并。
- `Codex Tools`：Tauri + React + Rust 跨平台 App，专注 Provider 管理、Provider 快速切换和线程归并，目标支持 macOS 与 Windows。

两个项目都只围绕 Codex 工作流设计，不试图成为泛用 AI CLI 管理器。

## 项目结构

```text
.
├── Package.swift
├── Sources/CodexSyncNative
├── build_swiftui_app.sh
└── codex-tools
    ├── package.json
    ├── src
    └── src-tauri
```

根目录是 `Codex Sync` 的 Swift Package。`codex-tools/` 是独立的 Tauri 项目。

## Codex Sync

`Codex Sync` 是 macOS 原生 SwiftUI 工具，核心功能是：

- 通过 WebDAV 推送本地 Codex 线程到远端。
- 通过 WebDAV 拉取远端 Codex 线程到本地。
- 将本地所有 Codex 线程归并到当前 `model_provider`。

默认同步范围：

- `~/.codex/sessions/**/rollout-*.jsonl`
- `~/.codex/archived_sessions/**/rollout-*.jsonl`
- `~/.codex/session_index.jsonl`

不会同步：

- `~/.codex/state_5.sqlite`
- `~/.codex/logs_2.sqlite`
- `*.wal`
- `*.shm`

开发构建：

```bash
swift build -c release
```

打包 macOS App：

```bash
./build_swiftui_app.sh
```

产物位置：

```text
dist-swiftui/Codex Sync.app
```

## Codex Tools

`Codex Tools` 是跨平台桌面工具，目标是把 Provider 管理和线程归并做成更高频、更轻量的入口。

核心功能：

- Provider 列表、创建、编辑、删除。
- 官方 `openai` Provider 固定保留，用于 Codex 官方 ChatGPT 登录模式。
- 三方 API Key Provider 写入 `config.toml` 和 `auth.json`。
- 切换 Provider 时自动归并线程 Provider。
- macOS 状态栏常驻，状态栏菜单可直接切换 Provider。
- WebDAV 配置入口，为后续跨平台同步能力预留。

开发构建：

```bash
cd codex-tools
npm install
npm run build
npm run tauri build
```

macOS 常见产物：

```text
codex-tools/src-tauri/target/release/bundle/macos/Codex Tools.app
codex-tools/src-tauri/target/release/bundle/dmg/*.dmg
```

Windows 常见产物：

```text
codex-tools/src-tauri/target/release/codex-tools.exe
codex-tools/src-tauri/target/release/bundle/nsis/*.exe
codex-tools/src-tauri/target/release/bundle/msi/*.msi
```

## Codex 配置路径

项目不写死任何用户机器上的绝对路径。

默认 Codex 目录：

```text
~/.codex
```

Windows 下通常对应：

```text
C:\Users\<用户名>\.codex
```

`Codex Tools` 支持通过 `CODEX_HOME` 覆盖：

```bash
export CODEX_HOME="/path/to/.codex"
```

```powershell
$env:CODEX_HOME="D:\Codex\.codex"
```

## 与 cc-switch、codex- 的关系

这个仓库不是从零凭空设计的，也不应该抹掉已有项目的功能和价值。

设计与实现过程中参考了：

- `cc-switch`：主要参考 Provider 管理、Provider 切换、状态栏常驻、快速切换入口等成熟交互。
- `codex-`：主要参考 Codex 线程同步、WebDAV 推送/拉取、本地多 Provider 线程归并等方向。

本仓库的目标是在这些思路基础上做一个更聚焦 Codex 的工具集合。它不会刻意替代原项目，也不会抹除原项目的功能边界：

- 如果你需要更完整的 Claude Code / 多 CLI 切换能力，请优先查看 `cc-switch`。
- 如果你关注 Codex 线程同步的原始探索方向，也建议查看 `codex-`。

正式开源发布前，建议在 Release 说明、项目文档和许可证信息中继续保留对这些项目的致谢。

## CI/CD

仓库提供两个 GitHub Actions 工作流：

- `CI`：在 push 和 pull request 时验证 Swift、前端和 Rust/Tauri 构建检查。
- `Release`：在推送 `v*` tag 时构建并上传 macOS 与 Windows 产物。

推荐发布流程：

```bash
git tag v0.1.0
git push origin v0.1.0
```

随后 GitHub Actions 会生成 Release 草稿，并上传构建产物。

## 风险提示

这些工具会读写当前用户的 Codex 配置和线程文件。正式使用前建议先备份：

```text
~/.codex
```

尤其是已有长期会话、多 Provider、多设备同步或重要历史线程的用户。

## Legacy Python 版本

仓库中仍保留了早期 Python/Tkinter + PyInstaller 版本的脚本，用于记录早期探索过程。标准构建入口以 SwiftUI 版 `Codex Sync` 和 Tauri 版 `Codex Tools` 为准。

## License

本项目采用 MIT License，详见 [LICENSE](LICENSE)。
