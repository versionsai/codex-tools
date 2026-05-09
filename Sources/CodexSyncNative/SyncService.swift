import Foundation
import SQLite3

enum SyncService {
    private static let syncRoots = ["sessions", "archived_sessions"]
    private static let singleFiles = ["session_index.jsonl"]
    static let rfc1123Formatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "EEE',' dd MMM yyyy HH':'mm':'ss z"
        return formatter
    }()

    static func loadConfig() throws -> SyncConfig {
        guard FileManager.default.fileExists(atPath: AppPaths.configURL.path) else {
            return SyncConfig()
        }
        let data = try Data(contentsOf: AppPaths.configURL)
        var config = try JSONDecoder().decode(SyncConfig.self, from: data)
        if config.codexDirectory.isEmpty {
            config.codexDirectory = AppPaths.defaultCodexDirectory.path
        }
        return config
    }

    static func saveConfig(_ config: SyncConfig) throws {
        try FileManager.default.createDirectory(at: AppPaths.defaultCodexDirectory, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(config)
        try data.write(to: AppPaths.configURL)
    }

    static func readSummary(at codexURL: URL) throws -> Summary {
        let provider = (try? readProvider(from: codexURL.appending(path: "config.toml"))) ?? "--"
        let active = rolloutPaths(in: codexURL.appending(path: "sessions")).count
        let archived = rolloutPaths(in: codexURL.appending(path: "archived_sessions")).count
        return Summary(provider: provider, activeSessions: active, archivedSessions: archived)
    }

    static func unifyProvider(at codexURL: URL, logger: @Sendable (String) -> Void) throws -> [String: String] {
        let provider = try readProvider(from: codexURL.appending(path: "config.toml"))
        let sqliteURL = codexURL.appending(path: "state_5.sqlite")
        let startedAt = Date()
        let rolloutFiles = syncRolloutPaths(in: codexURL)
        logger("扫描当前线程文件：\(rolloutFiles.count) 个")

        var rolloutChanged = 0
        for fileURL in rolloutFiles {
            let original = try String(contentsOf: fileURL, encoding: .utf8)
            let (updated, changed) = replaceProvider(in: original, provider: provider)
            if changed {
                try updated.write(to: fileURL, atomically: true, encoding: .utf8)
                rolloutChanged += 1
            }
        }

        let updatedRows = try updateThreadProviders(in: sqliteURL, provider: provider)
        let indexEntries = try rebuildIndex(in: codexURL, provider: provider, logger: logger)
        let elapsed = String(format: "%.2f", Date().timeIntervalSince(startedAt))
        return [
            "provider": provider,
            "rolloutScanned": String(rolloutFiles.count),
            "rolloutChanged": String(rolloutChanged),
            "threadRowsUpdated": String(updatedRows),
            "sessionIndexEntries": String(indexEntries),
            "elapsedSeconds": elapsed
        ]
    }

    static func push(config: SyncConfig, logger: @Sendable (String) -> Void) async throws -> (uploaded: Int, skipped: Int) {
        let session = try makeWebDAVSession(config: config)
        let localFiles = iterLocalFiles(in: config.codexURL)
        logger("使用远端同步根目录：\(config.normalizedBaseURL)")
        let remoteEntries = try await remoteFileMap(session: session, baseURL: config.normalizedBaseURL)
        var uploaded = 0
        var skipped = 0

        for fileURL in localFiles {
            let relative = try relativePath(of: fileURL, root: config.codexURL)
            let attributes = try FileManager.default.attributesOfItem(atPath: fileURL.path)
            let localDate = attributes[.modificationDate] as? Date
            let localSize = (attributes[.size] as? NSNumber)?.int64Value
            if let remote = remoteEntries[relative],
               let remoteDate = remote.lastModified,
               let localDate,
               let localSize,
               remoteDate >= localDate,
               remote.size == localSize {
                skipped += 1
                continue
            }
            try await ensureRemoteDirectory(session: session, baseURL: config.normalizedBaseURL, relativeDir: URL(fileURLWithPath: relative).deletingLastPathComponent().path)
            var request = URLRequest(url: try joinURL(base: config.normalizedBaseURL, relativePath: relative))
            request.httpMethod = "PUT"
            request.httpBody = try Data(contentsOf: fileURL)
            let (_, response) = try await session.data(for: request)
            try validate(response: response, accepted: [200, 201, 204])
            uploaded += 1
            logger("已上传 \(relative)")
        }
        return (uploaded, skipped)
    }

    static func pull(config: SyncConfig, logger: @Sendable (String) -> Void) async throws -> (downloaded: Int, skipped: Int) {
        let session = try makeWebDAVSession(config: config)
        logger("使用远端同步根目录：\(config.normalizedBaseURL)")
        let remoteEntries = try await remoteFileMap(session: session, baseURL: config.normalizedBaseURL)
        var downloaded = 0
        var skipped = 0

        for relative in remoteEntries.keys.sorted() {
            guard let remote = remoteEntries[relative] else { continue }
            let localURL = config.codexURL.appending(path: relative)
            let exists = FileManager.default.fileExists(atPath: localURL.path)
            if exists,
               let attributes = try? FileManager.default.attributesOfItem(atPath: localURL.path),
               let localDate = attributes[.modificationDate] as? Date,
               let remoteDate = remote.lastModified,
               localDate >= remoteDate {
                skipped += 1
                continue
            }
            var request = URLRequest(url: try joinURL(base: config.normalizedBaseURL, relativePath: relative))
            request.httpMethod = "GET"
            let (data, response) = try await session.data(for: request)
            try validate(response: response, accepted: [200])
            try FileManager.default.createDirectory(at: localURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: localURL)
            downloaded += 1
            logger("已下载 \(relative)")
        }
        return (downloaded, skipped)
    }
}

private extension SyncService {
    static func timestamp() -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter.string(from: .now)
    }

    static func rolloutPaths(in root: URL) -> [URL] {
        guard FileManager.default.fileExists(atPath: root.path) else { return [] }
        let enumerator = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)
        var urls: [URL] = []
        while let next = enumerator?.nextObject() as? URL {
            if next.lastPathComponent.hasPrefix("rollout-") && next.pathExtension == "jsonl" {
                urls.append(next)
            }
        }
        return urls
    }

    static func syncRolloutPaths(in codexURL: URL) -> [URL] {
        syncRoots.flatMap { root in
            rolloutPaths(in: codexURL.appending(path: root))
        }
    }

    static func iterLocalFiles(in codexURL: URL) -> [URL] {
        var files: [URL] = []
        for root in syncRoots {
            files.append(contentsOf: rolloutPaths(in: codexURL.appending(path: root)))
        }
        for single in singleFiles {
            let url = codexURL.appending(path: single)
            if FileManager.default.fileExists(atPath: url.path) {
                files.append(url)
            }
        }
        return files
    }

    static func readProvider(from configToml: URL) throws -> String {
        guard FileManager.default.fileExists(atPath: configToml.path) else {
            throw SyncError.missingConfigToml
        }
        let content = try String(contentsOf: configToml, encoding: .utf8)
        for rawLine in content.components(separatedBy: .newlines) {
            let line = rawLine.trimmingCharacters(in: .whitespaces)
            guard line.hasPrefix("model_provider") else { continue }
            let parts = line.split(separator: "=", maxSplits: 1).map(String.init)
            guard parts.count == 2 else { continue }
            let provider = parts[1].trimmingCharacters(in: .whitespacesAndNewlines).trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
            if !provider.isEmpty {
                return provider
            }
        }
        throw SyncError.providerNotFound
    }

    static func replaceProvider(in content: String, provider: String) -> (String, Bool) {
        var changed = false
        let lines = content.components(separatedBy: .newlines).map { line -> String in
            guard !line.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return line }
            guard let data = line.data(using: .utf8),
                  let object = try? JSONSerialization.jsonObject(with: data),
                  var json = object as? [String: Any],
                  let type = json["type"] as? String,
                  type == "session_meta",
                  var payload = json["payload"] as? [String: Any] else {
                return line
            }
            if payload["model_provider"] as? String != provider {
                payload["model_provider"] = provider
                json["payload"] = payload
                changed = true
                guard let updatedData = try? JSONSerialization.data(withJSONObject: json, options: []) else {
                    return line
                }
                return String(decoding: updatedData, as: UTF8.self)
            }
            return line
        }
        return (lines.joined(separator: "\n") + "\n", changed)
    }

    static func updateThreadProviders(in sqliteURL: URL, provider: String) throws -> Int {
        var db: OpaquePointer?
        guard sqlite3_open(sqliteURL.path, &db) == SQLITE_OK else {
            throw SyncError.sqlite("无法打开线程数据库")
        }
        defer { sqlite3_close(db) }
        let sql = "UPDATE threads SET model_provider = ? WHERE model_provider != ?"
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
            throw SyncError.sqlite("无法准备更新语句")
        }
        defer { sqlite3_finalize(statement) }
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        sqlite3_bind_text(statement, 1, provider, -1, transient)
        sqlite3_bind_text(statement, 2, provider, -1, transient)
        guard sqlite3_step(statement) == SQLITE_DONE else {
            throw SyncError.sqlite("更新线程 Provider 失败")
        }
        return Int(sqlite3_changes(db))
    }

    static func rebuildIndex(in codexURL: URL, provider: String, logger: @Sendable (String) -> Void) throws -> Int {
        let sqliteURL = codexURL.appending(path: "state_5.sqlite")
        let indexURL = codexURL.appending(path: "session_index.jsonl")
        let existing = try loadExistingIndex(from: indexURL)

        var db: OpaquePointer?
        guard sqlite3_open(sqliteURL.path, &db) == SQLITE_OK else {
            throw SyncError.sqlite("无法打开索引数据库")
        }
        defer { sqlite3_close(db) }

        let sql = """
        SELECT id, title, updated_at, updated_at_ms, cwd, model_provider,
               git_origin_url, git_branch, archived
        FROM threads
        ORDER BY COALESCE(updated_at_ms, updated_at * 1000) DESC, id DESC
        """
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &statement, nil) == SQLITE_OK else {
            throw SyncError.sqlite("无法查询线程索引")
        }
        defer { sqlite3_finalize(statement) }

        var output: [String] = []
        while sqlite3_step(statement) == SQLITE_ROW {
            let id = stringValue(statement, column: 0)
            var item = existing[id] ?? [:]
            let title = optionalStringValue(statement, column: 1)
            let updatedAt = sqlite3_column_int64(statement, 2)
            let updatedAtMs = sqlite3_column_int64(statement, 3)
            let cwd = optionalStringValue(statement, column: 4)
            let gitOrigin = optionalStringValue(statement, column: 6)
            let gitBranch = optionalStringValue(statement, column: 7)
            let projectRoot = cwd?.lowercased() ?? ""
            let projectName = cwd.map { URL(fileURLWithPath: $0).lastPathComponent } ?? ""
            item["id"] = id
            item["thread_name"] = title
            item["updated_at"] = updatedAtMs == 0 ? updatedAt * 1000 : updatedAtMs
            item["cwd"] = cwd
            item["model_provider"] = provider
            item["git_origin_url"] = gitOrigin
            item["git_branch"] = gitBranch
            item["project_root"] = item["project_root"] ?? projectRoot
            item["project_name"] = item["project_name"] ?? projectName
            item["project_key"] = item["project_key"] ?? projectRoot
            let data = try JSONSerialization.data(withJSONObject: item, options: [])
            output.append(String(decoding: data, as: UTF8.self))
        }
        try output.joined(separator: "\n").appending("\n").write(to: indexURL, atomically: true, encoding: .utf8)
        logger("已重建索引，共 \(output.count) 条线程")
        return output.count
    }

    static func loadExistingIndex(from indexURL: URL) throws -> [String: [String: Any]] {
        guard FileManager.default.fileExists(atPath: indexURL.path) else { return [:] }
        let content = try String(contentsOf: indexURL, encoding: .utf8)
        var items: [String: [String: Any]] = [:]
        for line in content.components(separatedBy: .newlines) where !line.trimmingCharacters(in: .whitespaces).isEmpty {
            guard let data = line.data(using: .utf8),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let id = json["id"] as? String else { continue }
            items[id] = json
        }
        return items
    }

    static func stringValue(_ statement: OpaquePointer?, column: Int32) -> String {
        optionalStringValue(statement, column: column) ?? ""
    }

    static func optionalStringValue(_ statement: OpaquePointer?, column: Int32) -> String? {
        guard let cString = sqlite3_column_text(statement, column) else { return nil }
        return String(cString: cString)
    }

    static func relativePath(of fileURL: URL, root: URL) throws -> String {
        let rootPath = root.standardizedFileURL.path
        let filePath = fileURL.standardizedFileURL.path
        guard filePath.hasPrefix(rootPath) else {
            throw SyncError.io("路径不在 Codex 目录内")
        }
        return String(filePath.dropFirst(rootPath.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    static func makeWebDAVSession(config: SyncConfig) throws -> URLSession {
        guard !config.normalizedBaseURL.isEmpty else {
            throw SyncError.invalidConfiguration("请填写 WebDAV 服务地址")
        }
        guard !config.username.isEmpty else {
            throw SyncError.invalidConfiguration("请填写 WebDAV 用户名")
        }
        let configuration = URLSessionConfiguration.ephemeral
        let auth = Data("\(config.username):\(config.password)".utf8).base64EncodedString()
        configuration.httpAdditionalHeaders = [
            "Authorization": "Basic \(auth)"
        ]
        return URLSession(configuration: configuration, delegate: WebDAVDelegate(verifyTLS: config.verifyTLS), delegateQueue: nil)
    }

    static func remoteFileMap(session: URLSession, baseURL: String) async throws -> [String: RemoteEntry] {
        var entries: [String: RemoteEntry] = [:]
        for root in syncRoots {
            do {
                let tree = try await listRemoteTree(session: session, baseURL: baseURL, relativeRoot: root)
                for item in tree where !item.isDirectory {
                    entries[item.relativePath] = item
                }
            } catch {
                if isNotFound(error) {
                    continue
                }
                throw error
            }
        }
        if let item = try await statRemoteFile(session: session, baseURL: baseURL, relativePath: "session_index.jsonl") {
            entries[item.relativePath] = item
        }
        return entries
    }

    static func listRemoteTree(session: URLSession, baseURL: String, relativeRoot: String) async throws -> [RemoteEntry] {
        let responseText = try await propfind(session: session, url: try joinURL(base: baseURL, relativePath: relativeRoot + "/"), depth: "1")
        let directEntries = parseMultistatus(baseURL: baseURL, xmlText: responseText, targetRelative: relativeRoot)
        var results: [RemoteEntry] = []
        for entry in directEntries {
            results.append(entry)
            if entry.isDirectory {
                results.append(contentsOf: try await listRemoteTree(session: session, baseURL: baseURL, relativeRoot: entry.relativePath))
            }
        }
        return results
    }

    static func statRemoteFile(session: URLSession, baseURL: String, relativePath: String) async throws -> RemoteEntry? {
        do {
            let responseText = try await propfind(session: session, url: try joinURL(base: baseURL, relativePath: relativePath), depth: "0")
            return parseMultistatus(baseURL: baseURL, xmlText: responseText, targetRelative: "").first(where: { $0.relativePath == relativePath })
        } catch {
            if let syncError = error as? SyncError, case .webdav(let message) = syncError, message.contains("404") {
                return nil
            }
            return nil
        }
    }

    static func ensureRemoteDirectory(session: URLSession, baseURL: String, relativeDir: String) async throws {
        let normalized = relativeDir.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard !normalized.isEmpty, normalized != "." else { return }
        var current = ""
        for part in normalized.split(separator: "/") {
            current = current.isEmpty ? String(part) : "\(current)/\(part)"
            var request = URLRequest(url: try joinURL(base: baseURL, relativePath: current + "/"))
            request.httpMethod = "MKCOL"
            let (_, response) = try await session.data(for: request)
            try validate(response: response, accepted: [201, 301, 405])
        }
    }

    static func propfind(session: URLSession, url: URL, depth: String) async throws -> String {
        let body = """
        <?xml version="1.0" encoding="utf-8" ?>
        <d:propfind xmlns:d="DAV:">
          <d:prop>
            <d:displayname />
            <d:resourcetype />
            <d:getcontentlength />
            <d:getlastmodified />
          </d:prop>
        </d:propfind>
        """
        var request = URLRequest(url: url)
        request.httpMethod = "PROPFIND"
        request.setValue(depth, forHTTPHeaderField: "Depth")
        request.setValue("application/xml; charset=utf-8", forHTTPHeaderField: "Content-Type")
        request.httpBody = Data(body.utf8)
        let (data, response) = try await session.data(for: request)
        try validate(response: response, accepted: [207, 200])
        return String(decoding: data, as: UTF8.self)
    }

    static func parseMultistatus(baseURL: String, xmlText: String, targetRelative: String) -> [RemoteEntry] {
        let parser = MultistatusParser()
        parser.basePath = URL(string: baseURL)?.path ?? ""
        parser.targetPath = (parser.basePath as NSString).appendingPathComponent(targetRelative).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let data = Data(xmlText.utf8)
        let xmlParser = XMLParser(data: data)
        xmlParser.delegate = parser
        xmlParser.parse()
        return parser.entries
    }

    static func joinURL(base: String, relativePath: String) throws -> URL {
        guard let baseURL = URL(string: base) else {
            throw SyncError.invalidConfiguration("WebDAV 地址无效")
        }
        let encoded = relativePath
            .split(separator: "/")
            .map { String($0).addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? String($0) }
            .joined(separator: "/")
        guard let url = URL(string: encoded, relativeTo: baseURL)?.absoluteURL else {
            throw SyncError.invalidConfiguration("WebDAV 地址无效")
        }
        return url
    }

    static func validate(response: URLResponse, accepted: [Int]) throws {
        guard let http = response as? HTTPURLResponse else {
            throw SyncError.webdav("WebDAV 响应无效")
        }
        guard accepted.contains(http.statusCode) else {
            throw SyncError.webdav("WebDAV 请求失败：\(http.statusCode)")
        }
    }

    static func isNotFound(_ error: Error) -> Bool {
        guard let syncError = error as? SyncError else {
            return false
        }
        if case .webdav(let message) = syncError {
            return message.contains("404")
        }
        return false
    }
}

private final class WebDAVDelegate: NSObject, URLSessionDelegate {
    private let verifyTLS: Bool

    init(verifyTLS: Bool) {
        self.verifyTLS = verifyTLS
    }

    func urlSession(_ session: URLSession, didReceive challenge: URLAuthenticationChallenge) async -> (URLSession.AuthChallengeDisposition, URLCredential?) {
        guard !verifyTLS else {
            return (.performDefaultHandling, nil)
        }
        guard let trust = challenge.protectionSpace.serverTrust else {
            return (.performDefaultHandling, nil)
        }
        return (.useCredential, URLCredential(trust: trust))
    }
}

private final class MultistatusParser: NSObject, XMLParserDelegate {
    var entries: [RemoteEntry] = []
    var basePath = ""
    var targetPath = ""

    private var currentElement = ""
    private var currentHref = ""
    private var currentLength = ""
    private var currentModified = ""
    private var currentIsCollection = false
    private var inResponse = false

    func parser(_ parser: XMLParser, didStartElement elementName: String, namespaceURI: String?, qualifiedName qName: String?, attributes attributeDict: [String : String] = [:]) {
        currentElement = qName ?? elementName
        if currentElement.hasSuffix("response") {
            inResponse = true
            currentHref = ""
            currentLength = ""
            currentModified = ""
            currentIsCollection = false
        }
        if currentElement.hasSuffix("collection") {
            currentIsCollection = true
        }
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) {
        guard inResponse else { return }
        switch currentElement {
        case let element where element.hasSuffix("href"):
            currentHref += string
        case let element where element.hasSuffix("getcontentlength"):
            currentLength += string
        case let element where element.hasSuffix("getlastmodified"):
            currentModified += string
        default:
            break
        }
    }

    func parser(_ parser: XMLParser, didEndElement elementName: String, namespaceURI: String?, qualifiedName qName: String?) {
        let element = qName ?? elementName
        if element.hasSuffix("response") {
            inResponse = false
            let normalizedBase = basePath.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            let hrefPath = URL(string: currentHref)?.path ?? currentHref
            let normalizedHref = hrefPath.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            guard normalizedHref != targetPath else { return }
            var relative = normalizedHref
            if !normalizedBase.isEmpty, normalizedHref.hasPrefix(normalizedBase) {
                relative = String(normalizedHref.dropFirst(normalizedBase.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            }
            guard !relative.isEmpty else { return }
            let date = SyncService.rfc1123Formatter.date(from: currentModified.trimmingCharacters(in: .whitespacesAndNewlines))
            let size = Int64(currentLength.trimmingCharacters(in: .whitespacesAndNewlines))
            entries.append(RemoteEntry(relativePath: relative, isDirectory: currentIsCollection, lastModified: date, size: size))
        }
        currentElement = ""
    }
}
