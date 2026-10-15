import XCTest

extension XCTestCase {
    #if os(iOS)
    @MainActor func dismissEditorKeyboard(in app: XCUIApplication) {
        func zeroKey(_ node: any XCUIElementSnapshot) -> Bool {
            node.elementType == .key && node.label == "0" || node.children.contains(where: zeroKey)
        }
        typealias Pad = (frame: CGRect, backdrop: CGRect?)
        func numberPads(_ node: any XCUIElementSnapshot) -> [Pad] {
            node.children.flatMap { child -> [Pad] in
                let nested = numberPads(child)
                if !nested.isEmpty { return nested }
                guard child.elementType == .popover && zeroKey(child) else { return [] }
                let regions = node.children.filter { $0.identifier == "PopoverDismissRegion" }
                return [(child.frame, regions.count == 1 ? regions[0].frame : nil)]
            }
        }
        func hasField(_ node: any XCUIElementSnapshot) -> Bool {
            node.elementType == .textField || node.children.contains(where: hasField)
        }
        func usable(_ frame: CGRect) -> Bool {
            [frame.minX, frame.minY, frame.maxX, frame.maxY].allSatisfy(\.isFinite) && frame.width > 0 && frame.height > 0
        }
        func fail(_ message: String) {
            let image = XCTAttachment(screenshot: app.screenshot()), tree = XCTAttachment(string: app.debugDescription)
            image.name = "keyboard-dismissal-blocked"; tree.name = "keyboard-dismissal-hierarchy"
            image.lifetime = .keepAlways; tree.lifetime = .keepAlways; add(image); add(tree)
            XCTFail(message)
        }
        let keyboard = app.keyboards.firstMatch
        let hide = keyboard.buttons["Hide keyboard"]
        var snapshotFailure: Error?
        let visible = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
            guard snapshotFailure == nil else { return false }
            if keyboard.exists { return true }
            do { return !numberPads(try app.snapshot()).isEmpty }
            catch { snapshotFailure = error; return false }
        }, object: app)
        let ready = XCTWaiter.wait(for: [visible], timeout: 5)
        if let snapshotFailure { XCTFail("Cannot inspect the editor keyboard: \(snapshotFailure)"); return }
        guard ready == .completed else { return }
        let pads: [Pad]
        do { pads = numberPads(try app.snapshot()) }
        catch { XCTFail("Cannot inspect the editor keyboard: \(error)"); return }
        guard pads.count <= 1 else { fail("The editor exposes multiple number pads: \(pads)"); return }
        if let pad = pads.first {
            let window = workspaceViewport(in: app), bounds = window.frame
            guard let backdrop = pad.backdrop, usable(backdrop), usable(pad.frame), usable(bounds), pad.frame.intersects(bounds) else {
                fail("The number pad needs its own dismissal region: \(pad), window \(bounds)")
                return
            }
            let region = backdrop.intersection(bounds), covered = pad.frame.intersection(region)
            guard usable(region), usable(covered) else {
                fail("The number pad needs a visible dismissal region: \(pad), window \(bounds)"); return
            }
            let candidates = [
                CGRect(x: region.minX, y: region.minY, width: covered.minX - region.minX, height: region.height),
                CGRect(x: covered.maxX, y: region.minY, width: region.maxX - covered.maxX, height: region.height),
                CGRect(x: region.minX, y: region.minY, width: region.width, height: covered.minY - region.minY),
                CGRect(x: region.minX, y: covered.maxY, width: region.width, height: region.maxY - covered.maxY)
            ].filter { usable($0) && region.contains($0) }
            guard let target = candidates.max(by: { $0.width * $0.height < $1.width * $1.height }) else {
                fail("The number pad covers its dismissal region: \(pad), window \(bounds)")
                return
            }
            let point = CGPoint(x: target.midX, y: target.midY)
            guard point.x.isFinite, point.y.isFinite, bounds.contains(point), region.contains(point), !pad.frame.contains(point) else {
                fail("The number pad must have a dismissal point inside its editor window: \(pad), window \(bounds)")
                return
            }
            window.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(
                dx: point.x - bounds.minX, dy: point.y - bounds.minY)).clickOrTap()
            let absent = XCTNSPredicateExpectation(predicate: NSPredicate { _, _ in
                guard snapshotFailure == nil else { return false }
                do {
                    let snapshot = try app.snapshot()
                    return hasField(snapshot) && numberPads(snapshot).isEmpty
                }
                catch { snapshotFailure = error; return false }
            }, object: app)
            let result = XCTWaiter.wait(for: [absent], timeout: 5)
            if let snapshotFailure { XCTFail("Cannot inspect the editor keyboard: \(snapshotFailure)"); return }
            guard result == .completed else { fail("The number pad must close while its editor fields remain visible"); return }
        }
        if keyboard.waitForExistence(timeout: 5) {
            guard hide.waitForExistence(timeout: 15), hide.isHittable else { fail("The keyboard needs a hittable Hide button"); return }
            hide.clickOrTap()
            XCTAssertTrue(keyboard.waitForNonExistence(timeout: 5))
        }
    }
    #endif
    @MainActor func replaceEditorText(_ field: XCUIElement, with value: String, in app: XCUIApplication) {
        #if os(macOS)
        workspaceActivate(field)
        field.typeKey("a", modifierFlags: .command)
        field.typeText(value)
        #else
        if !field.isHittable { dismissEditorKeyboard(in: app) }
        let focused = app.textFields.matching(identifier: field.identifier)
            .matching(NSPredicate(format: "hasKeyboardFocus == true")).firstMatch
        let scroll = app.scrollViews.containing(.textField, identifier: field.identifier).firstMatch
        for _ in 0..<2 {
            if focused.exists { break }
            if scroll.exists { revealEditorControl(field, in: scroll) }
            workspaceActivate(field)
            if focused.waitForExistence(timeout: 1) { break }
        }
        XCTAssertTrue(focused.waitForExistence(timeout: 3))
        let current = field.value as? String ?? ""
        let count = current == field.placeholderValue ? 0 : current.count
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: count) + value)
        #endif
        XCTAssertEqual(field.value as? String, value)
    }
    @MainActor func checkNativeSDRCreationAndColorEditing(in app: XCUIApplication) {
        // Initial actions run only after native GPU startup completes.
        guard app.textFields["new-document-width"].waitForExistence(timeout: 60) else {
            XCTFail("The initial New Drawing form did not open after canvas startup")
            return
        }
        func choose(_ id: String, _ label: String) {
            #if os(macOS)
            workspaceActivate(app.descendants(matching: .any).matching(identifier: id).firstMatch)
            workspaceActivate(app.menuItems[label].firstMatch)
            #else
            workspaceActivate(app.buttons[id].firstMatch)
            workspaceActivate(app.buttons[label].firstMatch)
            #endif
        }
        choose("new-document-preset", "Wide color")
        choose("new-document-depth", "16-bit SDR")
        choose("new-document-background", "Transparent")
        let width = app.textFields["new-document-width"]
        let create = app.buttons["new-document-create"]
        replaceEditorText(width, with: "0", in: app); XCTAssertFalse(create.isEnabled)
        replaceEditorText(width, with: "79", in: app); replaceEditorText(app.textFields["new-document-height"], with: "53", in: app)
        replaceEditorText(app.textFields["new-document-preset-name"], with: "Studio P3", in: app)
        #if os(macOS)
        workspaceActivate(app.checkBoxes["new-document-defaults"])
        #else
        workspaceActivate(app.switches["new-document-defaults"])
        #endif
        attachEditor(in: app, name: "sdr-new-options")
        workspaceActivate(create)
        #if os(iOS)
        if !create.waitForNonExistence(timeout: 2) { workspaceActivate(create) }
        #endif
        XCTAssertTrue(create.waitForNonExistence(timeout: 30))
        let title = editorDocumentTitle(in: app)
        expectation(for: NSPredicate(format: "label == %@ OR value == %@", "Untitled · 79 × 53", "Untitled · 79 × 53"), evaluatedWith: title)
        waitForExpectations(timeout: 30)
        workspaceActivate(app.buttons["paint-edit-color"].firstMatch)
        let use = app.buttons["color-use"]
        XCTAssertTrue(use.waitForExistence(timeout: 10))
        editColorValue("0-0", "37", in: app)
        workspaceActivate(use)
        XCTAssertTrue(use.waitForNonExistence(timeout: 10))
        workspaceActivate(app.buttons["paint-edit-color"].firstMatch)
        XCTAssertTrue(use.waitForExistence(timeout: 10))
        XCTAssertEqual(colorValueText("0-0", in: app), "37")
        attachEditor(in: app, name: "sdr-tagged-paint")
        workspaceActivate(app.buttons["color-cancel"])
        XCTAssertTrue(use.waitForNonExistence(timeout: 10))
        XCTAssertFalse(app.buttons["paint-palettes"].exists, "The picker uses Edit Color instead of a palette popup")
        #if os(macOS)
        app.typeKey("n", modifierFlags: .command)
        #else
        editorMenu(in: app, menu: "File", id: "new_document", label: "New drawing")
        #endif
        XCTAssertTrue(width.waitForExistence(timeout: 10))
        XCTAssertEqual(width.value as? String, "79")
        XCTAssertEqual(app.textFields["new-document-height"].value as? String, "53")
        choose("new-document-preset", "Studio P3")
        workspaceActivate(app.buttons["new-document-cancel"])
        XCTAssertTrue(create.waitForNonExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["Canvas error"].exists)
    }
}
