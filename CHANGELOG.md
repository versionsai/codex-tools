# 更新日志

所有重要变更都会记录在这个文件里。

格式参考 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)。已发布版本使用对应的 Git tag 标题，例如 `v0.1.6`；尚未发布的变更先写在 `未发布` 下，发版时再移动到新的 tag 区块。

## 未发布

## v0.1.7 - 2026-05-11

### 变更

- 将 `Windows Build` 工作流改为仅支持手动触发，避免每次推送 `main` 时重复打 Windows 包；正式发布包只由 tag 触发的 `Release` 工作流生成。
- 修复通过 macOS 状态栏快捷切换 Provider 后，重新打开主窗口时界面状态没有自动刷新的问题。

## v0.1.6 - 2026-05-11

### 变更

- 将 Codex Tools 从 `codex-tools/` 子目录提升到仓库根目录，根目录现在就是 Tauri 项目。
- 移除独立的 Codex Sync SwiftUI 应用和早期 Python/Tkinter 原型，仓库只保留 Codex Tools。
- 更新 GitHub Actions，让 CI、Release 和 Windows Build 都从仓库根目录构建。
- 更新根目录 `README.md` 和 `.gitignore`，匹配新的单项目结构。
- 修复 macOS 下应用启动和重复点击时窗口不显示的问题。
- 优化 WebDAV 推送：缓存已确认的远端目录，避免重复 `MKCOL`；移除推送结束时仅用于展示统计的第二次远端全量扫描。
- 新增 `CHANGELOG.md`，后续重要变更按版本记录。
- 将项目版本提升到 `0.1.6`，对应 Git tag `v0.1.6`。

### 移除

- 移除 `Package.swift`。
- 移除 `Sources/CodexSyncNative/`。
- 移除 `build_swiftui_app.sh`。
- 移除 `build_mac_app.sh`。
- 移除 `codex_sync_gui.py`。
- 移除 `docs/codex-sync.md`。
- 移除 `generate_app_icon.py`。

### 验证

- `npm ci && npm run build`
- `cargo check`
- `npm run tauri build -- --bundles app`

## v0.1.5

### 说明

- 对应 Git tag `v0.1.5`。
- 该版本仍使用旧的 `codex-tools/` 子目录结构，并同时保留 Codex Sync 相关文件。
