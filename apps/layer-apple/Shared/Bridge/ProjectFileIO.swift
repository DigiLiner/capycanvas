import Foundation
import Darwin
import CoreGraphics
import UniformTypeIdentifiers
import CryptoKit

struct ProjectFileAccess: Sendable {
    private struct Entry: Codable { let uri: String; let bookmark: Data }
    private let directory: URL?
    init(state: URL?) { directory = state?.appendingPathComponent("FileAccess", isDirectory: true) }
    private func location(_ uri: String) -> URL? {
        let key = SHA256.hash(data: Data(uri.utf8)).map { String(format: "%02x", $0) }.joined()
        return directory?.appendingPathComponent(key + ".json")
    }
    func remember(_ url: URL) throws {
        let uri = url.absoluteString
        guard url.isFileURL, let destination = location(uri) else { return }
        let bookmark = try ProjectFileIO.coordinate(url, writing: false) { _ in
            #if os(macOS)
            try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil)
            #else
            try url.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil)
            #endif
        }
        try AtomicJSONFile.write(JSONEncoder().encode(Entry(uri: uri, bookmark: bookmark)), to: destination)
    }
    func resolve(_ uri: String) throws -> URL? {
        guard let source = location(uri), let data = try AtomicJSONFile.read(source) else { return nil }
        let entry = try JSONDecoder().decode(Entry.self, from: data)
        guard entry.uri == uri else { return nil }
        var stale = false
        #if os(macOS)
        let options: URL.BookmarkResolutionOptions = [.withoutUI, .withoutImplicitStartAccessing, .withSecurityScope]
        #else
        let options: URL.BookmarkResolutionOptions = [.withoutUI, .withoutImplicitStartAccessing]
        #endif
        let url = try URL(resolvingBookmarkData: entry.bookmark, options: options, relativeTo: nil, bookmarkDataIsStale: &stale)
        guard url.isFileURL else { return nil }
        return url
    }
}

extension UTType {
    static let capyProject = UTType(exportedAs: "art.capycanvas.project", conformingTo: .data)
    static let capyPhotoTypes: [UTType] = {
        guard let text = takeNativeText(capy_photo_formats()) else { return [] }

        guard let formats = try? JSON.decode(text) else { return [] }
        var types: [UTType] = []
        for format in formats.array {
            for ext in format["extensions"].array {
                if let type = UTType(filenameExtension: ext.string), !types.contains(type) { types.append(type) }
            }
        }
        return types
    }()
}

/// A job owns immutable Rust data and GPU preparation, never a NativeOwner.
/// Its final release can destroy a large retired document, so use the I/O queue.
final class NativeProjectTask: @unchecked Sendable {
    enum Kind: UInt32 { case save = 0, open = 1, place = 3, color, properties, source, lookup, clip }
    static let io = DispatchQueue(label: "art.capycanvas.project-files", qos: .userInitiated)
    let handle: UInt64
    init(_ handle: UInt64) { self.handle = handle }
    deinit { let handle = handle; Self.io.async { capy_project_free(nativeValue(handle)) } }
    func cancel() { capy_project_cancel(nativeValue(handle)) }
    private func check(_ result: Int32) throws {
        guard result < 0 else { return }
        let error = takeNativeText(capy_project_error(nativeValue(handle)))

        throw HostFailure(message: error ?? "Document operation failed")
    }
    func write(to url: URL, previewOnly: Bool = false, original: URL? = nil, destinationError: String = "", expectedURI: String? = nil) throws {
        try ProjectFileIO.coordinate(url, writing: true) { destination in
            if previewOnly, let original {
                let access = original.startAccessingSecurityScopedResource()
                defer { if access { original.stopAccessingSecurityScopedResource() } }
                if ProjectFileIO.sameFile(original, destination) { throw HostFailure(message: destinationError) }
            }
            try ProjectFileIO.atomicWrite(to: destination, beforeCommit: { [self] in
                let existing = try? FileHandle(forReadingFrom: destination)
                defer { try? existing?.close() }
                try (expectedURI ?? url.absoluteString).withCString { try self.check(capy_project_destination_matches(nativeValue(self.handle), existing?.fileDescriptor ?? -1, $0)) }
                guard capy_project_begin_commit(nativeValue(self.handle)) == 0 else { throw HostFailure(message: "Document operation cancelled") }
            }) { [self] descriptor in try self.check(previewOnly ? capy_project_export_preview(nativeValue(self.handle), descriptor) : capy_project_write(nativeValue(self.handle), descriptor)) }
        }
    }
    func read(from url: URL?) throws {
        guard let url else { try check(capy_project_new(nativeValue(handle))); return }
        try ProjectFileIO.coordinate(url, writing: false) { source in
            let file = try FileHandle(forReadingFrom: source)
            defer { try? file.close() }
            try url.lastPathComponent.withCString { try self.check(capy_project_read(nativeValue(self.handle), file.fileDescriptor, $0)) }
        }
    }
    func read(image: Data, name: String = "Pasted image") throws {
        try name.withCString { title in
            try image.withUnsafeBytes { try check(capy_project_read_bytes(nativeValue(handle),
                $0.bindMemory(to: UInt8.self).baseAddress, $0.count, title)) }
        }
    }
    func prepareEdit(_ choice: JSON?, copy: Bool) throws {
        try check(try (choice ?? JSON()).encoded().withCString { capy_project_edit_work(nativeValue(handle), $0, copy) })
    }
    func finishImages() throws { try check(capy_project_finish_images(nativeValue(handle))) }
    func compare() throws { try check(capy_project_compare(nativeValue(handle))) }
    var clipProgress: String? {
        guard let text = takeNativeText(capy_project_clip_progress(nativeValue(handle))) else { return nil }

        return text
    }
    func copyPixels(nonce: String) throws -> Data {
        try nonce.withCString { try check(capy_project_clip_run(nativeValue(handle), $0)) }
        var count = 0
        try check(capy_project_clip_png(nativeValue(handle), &count))
        guard let data = copyNativeData(count, copy: { capy_project_clip_png_copy(handle, $0, $1, $2) }) else {
            throw HostFailure(message: "The copy has no pixels")
        }
        return data
    }
    func configureExport(_ recipe: JSON) throws {
        try check(try recipe.encoded().withCString { capy_project_export_options(nativeValue(handle), $0) })
    }
    func buildProof() throws { try check(capy_project_proof_build(nativeValue(handle))) }
    func proofPreservation() throws -> Data? {
        var count = 0
        try check(capy_project_proof_preservation(nativeValue(handle), &count))
        guard count > 0 else { return nil }
        guard let data = copyNativeData(count, copy: { capy_project_proof_preservation_copy(handle, $0, $1, $2) }) else {
            throw HostFailure(message: "Proof is unavailable")
        }
        return data
    }
    func details() throws -> JSON {
        guard let text = takeNativeText(capy_project_details(nativeValue(handle))) else { try check(-1); return JSON() }

        return try JSON.decode(text)
    }
    func comparison(after: Bool) throws -> CGImage {
        try comparison(index: after ? 1 : 0)
    }
    func comparison(index: UInt32) throws -> CGImage {
        var preview = CapyProjectPreview()
        guard capy_project_preview_at(nativeValue(handle), index, &preview) == 0,
            preview.width > 0, preview.height > 0, preview.width <= 512, preview.height <= 384,
            preview.count == Int(preview.width * preview.height * 4),
            let space = CGColorSpace(name: CGColorSpace.displayP3),
            let data = copyNativeData(preview.count, copy: { capy_project_preview_copy(handle, index, $0, $1, $2) }),
            let provider = CGDataProvider(data: data as CFData),
            let image = CGImage(width: Int(preview.width), height: Int(preview.height), bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: Int(preview.width) * 4, space: space,
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true, intent: .relativeColorimetric) else {
            throw HostFailure(message: "The color comparison is unavailable")
        }
        return image
    }

    func packageSummary() throws -> JSON {
        guard let text = takeNativeText(capy_project_package_summary(nativeValue(handle))) else { throw HostFailure(message: "Document operation is unavailable") }

        return try JSON.decode(text)
    }
    func packagePreview() throws -> CGImage? {
        var preview = CapyProjectPreview()
        try check(capy_project_package_preview(nativeValue(handle), &preview))
        guard preview.count > 0 else { return nil }
        guard preview.width > 0, preview.height > 0, preview.width <= 1024, preview.height <= 1024,
            preview.count == Int(preview.width * preview.height * 4),
            let space = CGColorSpace(name: CGColorSpace.sRGB),
            let data = copyNativeData(preview.count, copy: { capy_project_package_preview_copy(handle, $0, $1, $2) }),
            let provider = CGDataProvider(data: data as CFData),
            let image = CGImage(width: Int(preview.width), height: Int(preview.height), bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: Int(preview.width) * 4, space: space,
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true, intent: .relativeColorimetric) else {
            throw HostFailure(message: "The drawing preview is unavailable")
        }
        return image
    }

    func pendingProfile() throws -> JSON {
        guard let value = takeNativeText(capy_project_profile(nativeValue(handle))) else { throw HostFailure(message: "Document operation is unavailable") }

        return try JSON.decode(value)
    }
    func assumeProfile(_ profile: JSON) throws {
        try check(try profile.encoded().withCString { capy_project_assume_profile(nativeValue(handle), $0) })
    }
}

enum ProjectFileIO {
    static func sameFile(_ original: URL, _ destination: URL) -> Bool {
        if original.standardizedFileURL.resolvingSymlinksInPath() == destination.standardizedFileURL.resolvingSymlinksInPath() { return true }
        guard let source = try? FileManager.default.attributesOfItem(atPath: original.path),
            let target = try? FileManager.default.attributesOfItem(atPath: destination.path),
            let sourceDevice = source[.systemNumber] as? NSNumber, let targetDevice = target[.systemNumber] as? NSNumber,
            let sourceFile = source[.systemFileNumber] as? NSNumber, let targetFile = target[.systemFileNumber] as? NSNumber else { return false }
        return sourceDevice == targetDevice && sourceFile == targetFile
    }
    /// File providers can substitute the coordinated URL. Scope and coordination
    /// enclose the entire read/write, including Rust validation/compression.
    static func coordinate<Value>(_ url: URL, writing: Bool, work: @escaping (URL) throws -> Value) throws -> Value {
        let access = url.startAccessingSecurityScopedResource()
        defer { if access { url.stopAccessingSecurityScopedResource() } }
        let coordinator = NSFileCoordinator()
        var coordinationError: NSError?
        var result: Result<Value, Error>?
        let operation: (URL) -> Void = { location in
            result = Result { try work(location) }
        }
        if writing {
            coordinator.coordinate(writingItemAt: url, options: .forReplacing,
                error: &coordinationError, byAccessor: operation)
        } else {
            coordinator.coordinate(readingItemAt: url, options: [],
                error: &coordinationError, byAccessor: operation)
        }
        guard let result else { throw coordinationError ?? HostFailure(message: "Could not access the selected file") }
        let value = try result.get()
        if let coordinationError { throw coordinationError }
        return value
    }
    /// Stream into a system-provided replacement directory on the same volume.
    /// A picked file grants access to that file, not arbitrary siblings or its
    /// parent directory. Foundation publishes the replacement within that grant.
    /// The destination is unchanged until validation/write/fsync all succeed.
    static func atomicWrite(to url: URL, beforeCommit: () throws -> Void = {}, write: (Int32) throws -> Void) throws {
        let manager = FileManager.default
        let directory = try manager.url(for: .itemReplacementDirectory, in: .userDomainMask,
            appropriateFor: url, create: true)
        defer { try? manager.removeItem(at: directory) }
        let temporary = directory.appendingPathComponent(".\(UUID().uuidString).capy-tmp")
        let descriptor = temporary.path.withCString { open($0, O_WRONLY | O_CREAT | O_EXCL, 0o600) }
        guard descriptor >= 0 else { throw posixFailure() }
        var opened = true
        defer { if opened { close(descriptor) }; try? FileManager.default.removeItem(at: temporary) }
        try write(descriptor)
        guard synchronize(descriptor) else { throw posixFailure() }
        let closed = close(descriptor); opened = false
        guard closed == 0 else { throw posixFailure() }
        try beforeCommit()
        if manager.fileExists(atPath: url.path) {
            _ = try manager.replaceItemAt(url, withItemAt: temporary)
        } else {
            try manager.moveItem(at: temporary, to: url)
        }
    }
    static func stagingURL(title: String, extension suffix: String = "capy") throws -> URL {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700])
        let name = URL(fileURLWithPath: title).deletingPathExtension().lastPathComponent
        return folder.appendingPathComponent(name.isEmpty ? "Untitled" : name).appendingPathExtension(suffix)
    }
    /// Flushes through the drive's cache; a volume without that command gets an ordinary flush.
    static func synchronize(_ descriptor: Int32) -> Bool { fcntl(descriptor, F_FULLFSYNC) != -1 || fsync(descriptor) == 0 }
    private static func posixFailure() -> NSError { NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
}
