import Foundation
import SwiftUI

@MainActor
final class AppViewModel: ObservableObject {
    @Published var config = SyncConfig()
    @Published var provider = "--"
    @Published var activeSessions = 0
    @Published var archivedSessions = 0
    @Published var status: StatusBadge = .ready
    @Published var logs: [LogLine] = []
    @Published var isBusy = false
    @Published var lastError: String?

    func bootstrap() async {
        loadConfig()
        await refreshSummary()
    }

    func loadConfig() {
        do {
            config = try SyncService.loadConfig()
            appendLog("已加载配置：\(AppPaths.configURL.path)")
        } catch {
            appendLog("读取配置失败：\(error.localizedDescription)")
        }
    }

    func saveConfig() async {
        await perform(status: .saving) {
            try SyncService.saveConfig(self.config)
            self.appendLog("已保存配置：\(AppPaths.configURL.path)")
            self.status = .success
        }
    }

    func refreshSummary() async {
        do {
            let summary = try SyncService.readSummary(at: config.codexURL)
            provider = summary.provider
            activeSessions = summary.activeSessions
            archivedSessions = summary.archivedSessions
            if !isBusy {
                status = .ready
            }
        } catch {
            appendLog("刷新状态失败：\(error.localizedDescription)")
            lastError = error.localizedDescription
            status = .failed
        }
    }

    func pull() async {
        await perform(status: .pulling) {
            let logger: @Sendable (String) -> Void = { message in
                Task { @MainActor in
                    self.appendLog(message)
                }
            }
            let result = try await SyncService.pull(config: self.config, logger: logger)
            self.appendLog("拉取完成：下载 \(result.downloaded) 个，跳过 \(result.skipped) 个")
            self.status = .success
            try? await Task.sleep(for: .milliseconds(150))
            try? SyncService.readSummary(at: self.config.codexURL).apply(to: self)
        }
    }

    func push() async {
        await perform(status: .pushing) {
            let logger: @Sendable (String) -> Void = { message in
                Task { @MainActor in
                    self.appendLog(message)
                }
            }
            let result = try await SyncService.push(config: self.config, logger: logger)
            self.appendLog("推送完成：上传 \(result.uploaded) 个，跳过 \(result.skipped) 个")
            self.status = .success
            try? SyncService.readSummary(at: self.config.codexURL).apply(to: self)
        }
    }

    func unifyProvider() async {
        await perform(status: .merging) {
            self.appendLog("开始合并本地所有 Provider 线程")
            let codexURL = self.config.codexURL
            let logger: @Sendable (String) -> Void = { message in
                Task { @MainActor in
                    self.appendLog(message)
                }
            }
            let result = try await Task.detached(priority: .userInitiated) {
                try SyncService.unifyProvider(at: codexURL, logger: logger)
            }.value
            self.appendLog("合并完成：\(result)")
            self.status = .success
            try? SyncService.readSummary(at: codexURL).apply(to: self)
        }
    }

    func clearLogs() {
        logs.removeAll()
    }

    func appendLog(_ message: String) {
        logs.append(LogLine(timestamp: .now, message: message))
    }

    private func perform(status newStatus: StatusBadge, work: @escaping () async throws -> Void) async {
        guard !isBusy else { return }
        isBusy = true
        status = newStatus
        lastError = nil
        do {
            try await work()
            try await Task.sleep(for: .milliseconds(350))
            status = .ready
        } catch {
            lastError = error.localizedDescription
            appendLog("发生错误：\(error.localizedDescription)")
            status = .failed
        }
        await refreshSummary()
        isBusy = false
    }
}

private extension Summary {
    @MainActor
    func apply(to model: AppViewModel) throws {
        model.provider = provider
        model.activeSessions = activeSessions
        model.archivedSessions = archivedSessions
    }
}
