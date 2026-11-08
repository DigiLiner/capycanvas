import AppKit
import SwiftUI

@main struct DialogPageLayoutChecks {
    @MainActor static func main() {
        _ = NSApplication.shared
        NSApp.setActivationPolicy(.accessory)
        Task { @MainActor in
            do { try await check(); exit(0) }
            catch { FileHandle.standardError.write(Data("FAIL: \(error)\n".utf8)); exit(1) }
        }
        NSApp.run()
    }
    @MainActor static func fields(_ view: NSView) -> [NSTextField] {
        (view as? NSTextField).map { [$0] } ?? view.subviews.flatMap(fields)
    }
    @MainActor static func check() async throws {
        let store = EditorStore(platform: 1, persistence: EditorPersistence(root: nil), managedWorkspaces: false)
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 402, height: 874),
            styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.contentView = nil; window.close() }
        func wait(_ description: String, until ready: () -> Bool) async throws {
            let deadline = Date().addingTimeInterval(15)
            while !ready() {
                if let failure = store.failure { throw HostFailure(message: failure) }
                guard Date() < deadline else { throw HostFailure(message: description) }
                window.contentView?.layoutSubtreeIfNeeded()
                try await Task.sleep(for: .milliseconds(5))
            }
        }
        try await wait("Dialog catalog loads") { !store.catalog.isNull }
        let profile = NSHostingView(rootView: PhotoProfileForm(preferences: store.colorPreferences,
            interpretation: JSON(["channels": "Rgb", "depth": "U8"]), spaces: store.catalog["document_spaces"].array,
            error: nil, busy: false, choose: { _ in }).environment(\.editorDialogPage, true)
            .environment(\.capyProfileCopy, store.catalog["profile_copy"]))
        window.setContentSize(CGSize(width: 320, height: 874)); window.contentView = profile
        var profilePicker: NSPopUpButton?
        try await wait("The actual photo profile picker mounts") {
            var pending = [profile as NSView]
            while let view = pending.popLast() {
                if let picker = view as? NSPopUpButton { profilePicker = picker; return true }
                pending += view.subviews
            }
            return false
        }
        profile.layoutSubtreeIfNeeded()
        let pickerFrame = profile.convert(profilePicker!.bounds, from: profilePicker!)
        let pickerTop = profile.isFlipped ? pickerFrame.minY : profile.bounds.height - pickerFrame.maxY
        guard pickerTop < profile.bounds.height / 3 else {
            throw HostFailure(message: "Photo profile page picker must remain near the top: picker=\(pickerFrame), bounds=\(profile.bounds), top=\(pickerTop)")
        }
        print("PASS: Photo profile page places its native picker below the top header")
        try await store.apply(["type": "invoke", "command": "new_document"])
        try await wait("New drawing opens") { !store.state["new_drawing"].isNull }
        for width: CGFloat in [320, 402] {
            let host = NSHostingView(rootView: NewDrawingDialog(store: store).environment(\.editorDialogPage, true)
                .environment(\.editorPopupStore, store).environment(\.capyCommonCopy, store.bootstrap["common"]))
            window.setContentSize(CGSize(width: width, height: 874)); window.contentView = host
            try await wait("New drawing lays out its native name field") { fields(host).contains { $0.isEditable } }
            host.layoutSubtreeIfNeeded()
            for field in fields(host).filter(\.isEditable) {
                let frame = host.convert(field.bounds, from: field)
                guard frame.minX >= -0.5 && frame.maxX <= width + 0.5 else {
                    throw HostFailure(message: "New drawing field exceeds its \(width) point allocation: \(frame)")
                }
            }
            print("PASS: New drawing uses the \(Int(width)) point page allocation")
        }
    }
}
