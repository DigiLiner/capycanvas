import AppKit

@MainActor func projectMacMenuTitles(_ labels: [String: String], in root: NSMenu, windowsMenu: NSMenu?) {
    func project(_ id: String, onto menu: NSMenu) {
        guard let label = labels[id], menu.supermenu === root else { return }
        let index = root.indexOfItem(withSubmenu: menu)
        guard index >= 0, let item = root.item(at: index) else { return }
        if menu.title != label { menu.title = label }
        if item.title != label { item.title = label }
    }
    for (id, action) in [
        ("file", #selector(NSWindow.performClose(_:))),
        ("edit", #selector(NSText.cut(_:))),
        ("view", #selector(NSWindow.toggleFullScreen(_:)))
    ] {
        let menus = root.items.compactMap(\.submenu).filter { menu in
            menu.items.contains { $0.action == action }
        }
        if menus.count == 1 { project(id, onto: menus[0]) }
    }
    if let windowsMenu { project("window", onto: windowsMenu) }
}
