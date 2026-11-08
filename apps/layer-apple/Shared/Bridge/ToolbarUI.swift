import Foundation
import os

private func resolveUI(_ request: [String: Any], using call: (UnsafePointer<CChar>?) -> UInt64, failure: String) -> JSON {
    do {
        let text = try JSON(request).encoded()
        guard let response = text.withCString({ takeNativeText(call($0)) }) else {
            throw HostFailure(message: failure)
        }
        return try JSON.decode(response)
    } catch { return JSON(["error": error.localizedDescription]) }
}

enum ToolbarUI {
    static let metrics = resolve(["type": "metrics"])
    static func resolve(_ request: [String: Any], language: String = "en") -> JSON {
        resolveUI(["language": language, "request": request], using: capy_apple_toolbar_ui, failure: "Toolbar request failed")
    }
    private static let cache = OSAllocatedUnfairLock(initialState: [String: JSON]())
    static func cached(_ request: [String: Any], language: String = "en") -> JSON {
        let key = language + ":" + JSON(request).stableKey
        if let value = cache.withLock({ $0[key] }) { return value }
        let value = resolve(request, language: language)
        cache.withLock {
            if $0.count > 512 { $0.removeAll() }
            $0[key] = value
        }
        return value
    }
    static func formatted(_ control: JSON, value: Double, units: Bool = true, language: String = "en") -> JSON {
        resolve(["type": "number", "request": ["control": control.raw, "value": value, "operation": ["type": "format"]],
            "compact": true, "units": units], language: language)
    }
}

enum PhoneUI {
    static func resolve(_ request: [String: Any]) -> JSON {
        resolveUI(request, using: capy_apple_phone_ui, failure: "Phone request failed")
    }
}

enum DialogUI {
    static func resolve(_ request: [String: Any]) -> JSON {
        resolveUI(request, using: capy_apple_dialog_ui, failure: "Dialog request failed")
    }
}

/// Shared parsing and color transforms, independent of the renderer owner.
enum ColorUI {
    static func resolve(_ request: [String: Any], language: String = "en") -> JSON {
        resolveUI(["language": language, "request": request], using: capy_apple_color_ui, failure: "Color input failed")
    }
    static func profileEntries(_ entries: [JSON], language: String, query: String = "") -> [JSON] {
        let rows = resolve(["type": "profile_entries", "entries": entries.map(\.raw), "query": query], language: language).array
        return rows.map { row in row.replacing("visible", with: entries.first { $0["id"].string == row["id"].string }?["visible"] ?? JSON(false)) }
            .sorted { $0["name"].string.localizedStandardCompare($1["name"].string) == .orderedAscending }
    }
    static func preview(_ color: JSON) -> JSON {
        resolve(["type": "preview", "colors": [color.raw], "display_space": "DisplayP3"])[0]
    }
}
