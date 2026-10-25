import Foundation
import Darwin

@main struct ProjectFileAccessChecks {
    static func main() throws {
        if CommandLine.arguments.count == 3 {
            let source = URL(fileURLWithPath: CommandLine.arguments[1])
            try Data("original".utf8).write(to: source)
            let bookmark = try source.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
            try bookmark.write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
            return
        }
        precondition(CommandLine.arguments.count == 4)
        let selected = URL(fileURLWithPath: CommandLine.arguments[1])
        let phase = CommandLine.arguments[2], token = CommandLine.arguments[3]
        precondition(UUID(uuidString: token) != nil)
        let state = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
            .appendingPathComponent("capy-project-access-" + token, isDirectory: true)
        let access = ProjectFileAccess(state: state), uri = selected.absoluteString
        func balanced() { precondition((try? Data(contentsOf: selected)) == nil, "Every scope acquisition must be balanced") }
        balanced()
        if phase == "remember" {
            let existing = try access.resolve(uri)
            precondition(existing == nil)
            let data = try Data(contentsOf: Bundle.main.url(forResource: "input", withExtension: "bookmark")!)
            var stale = false
            let destination = try URL(resolvingBookmarkData: data, options: [.withoutUI, .withoutImplicitStartAccessing],
                relativeTo: nil, bookmarkDataIsStale: &stale)
            precondition(destination.standardizedFileURL == selected.standardizedFileURL, "The picked grant must identify the exact selected file")
            try access.remember(destination)
            try Data(String(ProcessInfo.processInfo.processIdentifier).utf8).write(to: state.appendingPathComponent("remember.pid"))
            balanced()
            print("PASS: remembered an app-scoped file grant for a fresh process")
            return
        }
        precondition(phase == "restore")
        defer { try? FileManager.default.removeItem(at: state) }
        let previousPID = try String(contentsOf: state.appendingPathComponent("remember.pid"), encoding: .utf8)
        precondition(previousPID != String(ProcessInfo.processInfo.processIdentifier), "The grant must survive an independent process")
        func destination() throws -> URL {
            guard let resolved = try access.resolve(uri) else { preconditionFailure("The stored grant must resolve without reading the picked bookmark") }
            precondition(resolved.standardizedFileURL == selected.standardizedFileURL, "The persisted grant must resolve the exact selected file")
            return resolved
        }
        let unrelated = try access.resolve(selected.deletingLastPathComponent().appendingPathComponent("unselected.capy").absoluteString)
        precondition(unrelated == nil, "Remembered access must be keyed by the exact original URI")
        func read() throws -> Data { try ProjectFileIO.coordinate(destination(), writing: false) { try Data(contentsOf: $0) } }
        func write(_ value: String, fail: Bool = false, cancel: Bool = false) throws {
            try ProjectFileIO.coordinate(destination(), writing: true) { location in
                try ProjectFileIO.atomicWrite(to: location, beforeCommit: {
                    if cancel { throw HostFailure(message: "cancelled") }
                }) { descriptor in
                    let bytes = Array(value.utf8)
                    precondition(bytes.withUnsafeBytes { Darwin.write(descriptor, $0.baseAddress, $0.count) } == bytes.count)
                    if fail { throw HostFailure(message: "write failed") }
                }
            }
        }
        let original = try read()
        precondition(original == Data("original".utf8))
        balanced()
        for (fail, cancel) in [(true, false), (false, true)] {
            do { try write("incomplete", fail: fail, cancel: cancel); preconditionFailure("Expected cancellation/failure") }
            catch let error as HostFailure { precondition(error.message == (fail ? "write failed" : "cancelled")) }
            let unchanged = try read()
            precondition(unchanged == original, "Failed saves must leave the original intact")
            balanced()
        }
        for revision in 1...3 {
            let text = "saved revision \(revision)"
            try write(text)
            let saved = try read()
            precondition(saved == Data(text.utf8), "Repeated Save must replace the selected file")
            balanced()
        }
        try ProjectFileIO.coordinate(destination(), writing: true) { location in
            let sibling = location.deletingLastPathComponent().appendingPathComponent("unselected.capy")
            do { try Data().write(to: sibling); preconditionFailure("The grant must not cover arbitrary siblings") }
            catch { precondition((error as NSError).domain == NSCocoaErrorDomain) }
        }
        balanced()
        print("PASS: independently restored file-only grant, failed/cancelled write preservation, three atomic replacements, exact reads and balanced access")
    }
}
