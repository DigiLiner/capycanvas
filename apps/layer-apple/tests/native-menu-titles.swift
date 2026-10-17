import AppKit

@main enum NativeMenuTitlesChecks {
    @MainActor static func main() throws {
        func require(_ value: Bool, _ message: String) throws {
            if !value { throw NSError(domain: "NativeMenuTitlesChecks", code: 1,
                userInfo: [NSLocalizedDescriptionKey: message]) }
        }
        let root = NSMenu(title: "Root"), target = NSObject()
        var menus: [String: NSMenu] = [:]
        var parents: [String: NSMenuItem] = [:]
        var commands: [NSMenuItem] = []
        for id in ["window", "foreign", "view", "file", "edit"] {
            let menu = NSMenu(title: "Native " + id)
            let parent = NSMenuItem(title: id == "foreign" ? "File" : "Old " + id,
                action: nil, keyEquivalent: "")
            parent.submenu = menu; root.addItem(parent)
            menus[id] = menu; parents[id] = parent
            let actions = ["file": #selector(NSWindow.performClose(_:)),
                "edit": #selector(NSText.cut(_:)), "view": #selector(NSWindow.toggleFullScreen(_:))]
            let item = NSMenuItem(title: "Literal " + id, action: actions[id], keyEquivalent: "q")
            item.keyEquivalentModifierMask = [.command, .option]
            item.target = target; item.representedObject = target
            item.isEnabled = false; item.state = .on
            menu.addItem(item); commands.append(item)
        }
        let japanese = ["file": "ファイル", "edit": "編集", "view": "表示", "window": "ウインドウ"]
        let english = ["file": "File", "edit": "Edit", "view": "View", "window": "Window"]
        let foreignTitle = menus["foreign"]?.title
        let originalActions = commands.map(\.action)
        let originalTitles = commands.map(\.title)
        for labels in [japanese, english] {
            projectMacMenuTitles(labels, in: root, windowsMenu: menus["window"])
            for (id, label) in labels {
                try require(menus[id]?.title == label && parents[id]?.title == label,
                    "The native heading must follow shared copy for " + id)
            }
            try require(parents["foreign"]?.title == "File" && menus["foreign"]?.title == foreignTitle,
                "An unrelated matching title must retain its menu")
            try require(commands.map(\.action) == originalActions && commands.map(\.title) == originalTitles,
                "Relabeling headings must retain command actions and literal titles")
            for item in commands {
                try require(item.target === target && (item.representedObject as? NSObject) === target
                    && item.keyEquivalent == "q" && item.keyEquivalentModifierMask == [.command, .option]
                    && !item.isEnabled && item.state == .on,
                    "Relabeling headings must retain targets, ownership, shortcuts and state")
            }
        }
        let ambiguous = NSMenu(title: "Ambiguous")
        ambiguous.addItem(NSMenuItem(title: "Other close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: ""))
        let ambiguousParent = NSMenuItem(title: "Other file", action: nil, keyEquivalent: "")
        ambiguousParent.submenu = ambiguous; root.insertItem(ambiguousParent, at: 0)
        let ambiguousTitle = ambiguous.title
        projectMacMenuTitles(["file": "ファイル"], in: root, windowsMenu: nil)
        try require(menus["file"]?.title == "File" && ambiguous.title == ambiguousTitle
            && ambiguousParent.title == "Other file", "An ambiguous native role must not select a menu")
        let replacement = NSMenu(title: "Replacement")
        projectMacMenuTitles(japanese, in: replacement, windowsMenu: menus["window"])
        try require(menus["window"]?.title == "Window" && replacement.items.isEmpty,
            "A retained windows menu from a different root must not change")
        projectMacMenuTitles([:], in: root, windowsMenu: menus["window"])
        try require(menus["window"]?.title == "Window", "Missing shared labels must retain native titles")
        print("PASS native menu headings: role identity, language round-trip, ambiguous roles and retained actions")
    }
}
