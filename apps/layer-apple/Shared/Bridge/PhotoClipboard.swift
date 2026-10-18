import Foundation
import UniformTypeIdentifiers
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// One deferred transport item, shared by picker, clipboard and external drops.
/// Files stay streamed/coordinated; encoded representations are never redrawn.
@MainActor struct PhotoItem {
    enum Content { case file(URL), image(Data) }
    let name: String
    typealias Loader = (@escaping (Result<Content, Error>) -> Void) -> Void
    let representations: [Loader]
    var load: Loader { representations[0] }
    init(name: String = "Pasted image", load: @escaping (@escaping (Result<Content, Error>) -> Void) -> Void) {
        self.name = name; self.representations = [load]
    }
    init(fileURL: URL) {
        name = fileURL.lastPathComponent; representations = [{ $0(.success(.file(fileURL))) }]
    }
    init(name: String = "Pasted image", representations: [Loader]) { self.name = name; self.representations = representations }
    nonisolated static func content(_ data: Data, file: Bool) throws -> Content {
        if !file { return .image(data) }
        guard let url = URL(dataRepresentation: data, relativeTo: nil), url.isFileURL else {
            throw HostFailure(message: "The image provider did not supply a local file")
        }
        return .file(url)
    }
    static func provider(_ provider: NSItemProvider) -> PhotoItem? { makeProvider(provider, types: UTType.capyPhotoTypes) }
    static func drawingProvider(_ provider: NSItemProvider) -> PhotoItem? { makeProvider(provider, types: [.capyProject] + UTType.capyPhotoTypes) }
    static func clipboardProvider(_ provider: NSItemProvider, limit: Int, tooLarge: String) -> PhotoItem? {
        makeProvider(provider, types: UTType.capyPhotoTypes, limit: (limit, tooLarge))
    }
    private static func makeProvider(_ provider: NSItemProvider, types: [UTType], limit: (bytes: Int, message: String)? = nil) -> PhotoItem? {
        let available = ([UTType.fileURL] + types).filter { provider.hasItemConformingToTypeIdentifier($0.identifier) }
        guard !available.isEmpty else { return nil }
        return PhotoItem(name: provider.suggestedName ?? "Imported image", representations: available.map { type in
            { done in
                if let limit, type != .fileURL {
                    provider.loadFileRepresentation(forTypeIdentifier: type.identifier) { url, error in
                        let result = Result { () throws -> Content in
                            guard let url else { throw error ?? HostFailure(message: "Could not read an image") }
                            let file = try FileHandle(forReadingFrom: url)
                            defer { try? file.close() }
                            let data = try file.read(upToCount: limit.bytes + 1) ?? Data()
                            guard data.count <= limit.bytes else { throw HostFailure(message: limit.message) }
                            return .image(data)
                        }
                        DispatchQueue.main.async { done(result) }
                    }
                    return
                }
                provider.loadDataRepresentation(forTypeIdentifier: type.identifier) { data, error in
                    DispatchQueue.main.async {
                        if let data { done(Result { try content(data, file: type == .fileURL) }) }
                        else { done(.failure(error ?? HostFailure(message: "Could not read an image"))) }
                    }
                }
            }
        })
    }
    static func declaresDrawing(_ provider: NSItemProvider) -> Bool {
        provider.hasItemConformingToTypeIdentifier(UTType.capyProject.identifier)
    }
    static func drawings(_ providers: ArraySlice<NSItemProvider>, found: [Bool] = [], _ done: @escaping ([Bool]) -> Void) {
        guard let provider = providers.first else { done(found); return }
        let next = { (drawing: Bool) in drawings(providers.dropFirst(), found: found + [drawing], done) }
        if declaresDrawing(provider) { next(true); return }
        guard !UTType.capyPhotoTypes.contains(where: { provider.hasItemConformingToTypeIdentifier($0.identifier) }),
            provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) else { next(false); return }
        provider.loadDataRepresentation(forTypeIdentifier: UTType.fileURL.identifier) { data, _ in
            let drawing = data.flatMap { URL(dataRepresentation: $0, relativeTo: nil) }?.withUnsafeFileSystemRepresentation { capy_drawing_path($0) } == true
            DispatchQueue.main.async { next(drawing) }
        }
    }
}

@MainActor enum PhotoClipboard {
    private static let nonceType = "art.capycanvas.clip.nonce"
    #if os(macOS)
    private final class PNGProvider: NSObject, NSPasteboardItemDataProvider {
        let png: Data
        init(_ png: Data) { self.png = png }
        func pasteboard(_ pasteboard: NSPasteboard?, item: NSPasteboardItem, provideDataForType type: NSPasteboard.PasteboardType) {
            item.setData(png, forType: type)
        }
    }
    private static var provider: PNGProvider?
    #endif
    static func write(png: Data, nonce: String, failure: String) throws {
        #if os(macOS)
        let item = NSPasteboardItem(), provider = PNGProvider(png)
        item.setDataProvider(provider, forTypes: [.png])
        item.setString(nonce, forType: NSPasteboard.PasteboardType(nonceType))
        NSPasteboard.general.clearContents()
        guard NSPasteboard.general.writeObjects([item]) else {
            throw HostFailure(message: failure)
        }
        self.provider = provider
        #else
        let item = NSItemProvider()
        item.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { $0(png, nil); return nil }
        item.registerDataRepresentation(forTypeIdentifier: nonceType, visibility: .all) { $0(Data(nonce.utf8), nil); return nil }
        UIPasteboard.general.setItemProviders([item], localOnly: false, expirationDate: nil)
        #endif
    }
    static var nonce: String? {
        #if os(macOS)
        NSPasteboard.general.string(forType: NSPasteboard.PasteboardType(nonceType))
        #else
        let board = UIPasteboard.general
        guard board.contains(pasteboardTypes: [nonceType]) else { return nil }
        return board.data(forPasteboardType: nonceType).flatMap { String(data: $0, encoding: .utf8) }
        #endif
    }
    static func read(tooLarge: String, _ completion: @escaping (Result<[PhotoItem], Error>) -> Void) {
        let limit = capy_clipboard_image_bytes()
        #if os(macOS)
        let types = UTType.capyPhotoTypes, generation = NSPasteboard.general.changeCount
        let images = (NSPasteboard.general.pasteboardItems ?? []).enumerated().compactMap { index, item -> PhotoItem? in
            let available = ([UTType.fileURL] + types).map { NSPasteboard.PasteboardType($0.identifier) }.filter { item.types.contains($0) }
            guard !available.isEmpty else { return nil }
            return PhotoItem(representations: available.map { type in
                { done in
                    DispatchQueue.global(qos: .userInitiated).async {
                        let result = Result { () throws -> PhotoItem.Content in
                            let board = NSPasteboard.general
                            guard board.changeCount == generation, let items = board.pasteboardItems, index < items.count,
                                let data = items[index].data(forType: type), board.changeCount == generation else {
                                throw HostFailure(message: "Could not read a clipboard image")
                            }
                            guard data.count <= limit else { throw HostFailure(message: tooLarge) }
                            return try PhotoItem.content(data, file: type.rawValue == UTType.fileURL.identifier)
                        }
                        DispatchQueue.main.async { done(result) }
                    }
                }
            })
        }
        #else
        let images = UIPasteboard.general.itemProviders.compactMap { PhotoItem.clipboardProvider($0, limit: limit, tooLarge: tooLarge) }
        #endif
        // Decode each item before requesting the next encoded representation.
        // Shared batch limits can stop loading without retaining every input.
        if !images.isEmpty { completion(.success(images)); return }
        completion(.failure(HostFailure(message: "Copy a supported image to paste.")))
    }
}
