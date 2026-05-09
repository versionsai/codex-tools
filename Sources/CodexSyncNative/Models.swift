import Foundation

struct SyncConfig: Codable, Equatable {
    var baseURL: String = ""
    var username: String = ""
    var password: String = ""
    var verifyTLS: Bool = true
    var codexDirectory: String = AppPaths.defaultCodexDirectory.path

    enum CodingKeys: String, CodingKey {
        case baseURL = "base_url"
        case username
        case password
        case verifyTLS = "verify_tls"
    }

    var normalizedBaseURL: String {
        guard !baseURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return ""
        }
        let trimmed = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        let withSlash = trimmed.hasSuffix("/") ? trimmed : "\(trimmed)/"
        guard let url = URL(string: withSlash) else {
            return withSlash
        }
        let path = url.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let lastComponent = path.split(separator: "/").last.map(String.init)
        guard lastComponent == "sessions" || lastComponent == "archived_sessions" else {
            return withSlash
        }
        var components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        let parentPath = (url.path as NSString).deletingLastPathComponent
        components?.path = parentPath.hasSuffix("/") ? parentPath : "\(parentPath)/"
        return components?.url?.absoluteString ?? withSlash
    }

    var codexURL: URL {
        URL(fileURLWithPath: NSString(string: codexDirectory).expandingTildeInPath)
    }
}

struct Summary: Equatable {
    var provider: String
    var activeSessions: Int
    var archivedSessions: Int
}

struct RemoteEntry: Hashable {
    var relativePath: String
    var isDirectory: Bool
    var lastModified: Date?
    var size: Int64?
}

struct LogLine: Identifiable, Hashable {
    let id = UUID()
    let timestamp: Date
    let message: String
}

enum StatusBadge: String {
    case ready = "就绪"
    case saving = "保存中"
    case pulling = "正在拉取"
    case pushing = "正在推送"
    case merging = "正在合并"
    case success = "已完成"
    case failed = "失败"
}

enum AppPaths {
    static let defaultCodexDirectory: URL = {
        if let codexHome = ProcessInfo.processInfo.environment["CODEX_HOME"], !codexHome.isEmpty {
            return URL(fileURLWithPath: NSString(string: codexHome).expandingTildeInPath)
        }
        return FileManager.default.homeDirectoryForCurrentUser.appending(path: ".codex")
    }()
    static let configURL = defaultCodexDirectory.appending(path: "webdav_sync_config.json")
}

enum SyncError: LocalizedError {
    case invalidConfiguration(String)
    case missingConfigToml
    case providerNotFound
    case sqlite(String)
    case webdav(String)
    case io(String)

    var errorDescription: String? {
        switch self {
        case .invalidConfiguration(let message),
                .sqlite(let message),
                .webdav(let message),
                .io(let message):
            return message
        case .missingConfigToml:
            return "未找到 config.toml"
        case .providerNotFound:
            return "未找到当前 Provider"
        }
    }
}
