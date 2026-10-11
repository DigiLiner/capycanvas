import XCTest

extension XCTestCase {
    @MainActor func checkLayerPropertiesDrawer(in app: XCUIApplication, theme: String) {
        app.launchEnvironment["CAPY_INITIAL_ACTIONS"] = #"[{"type":"set_theme","theme":"\#(theme)"},{"type":"workspace_manager","command":{"type":"switch","id":"builtin:workspace:painter"}}]"#
        app.launch()
        let canvas = app.descendants(matching: .any)["canvas"].firstMatch
        expectation(for: NSPredicate(format: "value == %@", "Canvas ready"), evaluatedWith: canvas)
        waitForExpectations(timeout: 30)
        workspaceActivate(app.buttons.matching(NSPredicate(format: "identifier == %@ AND label == %@", "header-tool-panel", "Layers")).firstMatch)
        let footer = app.buttons["layer-properties-disclosure"], label = app.staticTexts["layer-properties-disclosure-label"]
        XCTAssertTrue(footer.waitForExistence(timeout: 10))
        let drawer = app.descendants(matching: .any)["tool-drawer"].firstMatch
        var measuredHeight: CGFloat = 0, stableSince = Date()
        expectation(for: NSPredicate { _, _ in
            let height = drawer.frame.height
            if abs(height - measuredHeight) > 0.5 { measuredHeight = height; stableSince = Date() }
            return height > 200 && Date().timeIntervalSince(stableSince) > 0.3
        }, evaluatedWith: drawer)
        waitForExpectations(timeout: 10)
        XCTAssertEqual(label.label, "Layer properties")
        let floor = drawer.frame.height
        for _ in 0..<6 {
            let rows = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "layer-row-"))
            let before = rows.firstMatch.identifier
            workspaceActivate(app.buttons["layer-New layer"])
            expectation(for: NSPredicate { _, _ in rows.firstMatch.identifier != before }, evaluatedWith: rows.firstMatch)
            waitForExpectations(timeout: 5)
        }
        expectation(for: NSPredicate { _, _ in drawer.frame.height > floor + 20 }, evaluatedWith: drawer)
        waitForExpectations(timeout: 10)
        let grown = drawer.frame.height
        workspaceActivate(footer)
        expectation(for: NSPredicate { _, _ in label.label == "All layers" && abs(drawer.frame.height - floor) <= 2 }, evaluatedWith: drawer)
        waitForExpectations(timeout: 10)
        attachEditor(in: app, name: "layer-properties-\(theme)")
        workspaceActivate(footer)
        expectation(for: NSPredicate { _, _ in label.label == "Layer properties" && abs(drawer.frame.height - grown) <= 2 }, evaluatedWith: drawer)
        waitForExpectations(timeout: 10)
        workspaceActivate(app.buttons["layer-New group"])
        expectation(for: NSPredicate(format: "label == %@", "Group properties"), evaluatedWith: label)
        waitForExpectations(timeout: 5)
        attachEditor(in: app, name: "layer-properties-group-\(theme)")
        XCTAssertFalse(app.staticTexts["Canvas error"].exists)
    }
    @MainActor func checkLayerControls(in container: XCUIElement) {
        #if os(macOS)
        let candidates = container.groups
        #else
        let candidates = container.otherElements
        #endif
        let rows = candidates.matching(NSPredicate(format: "identifier BEGINSWITH %@", "layer-row-"))
        let initialCount = rows.count
        XCTAssertGreaterThanOrEqual(initialCount, 2)
        let original = rows.element(boundBy: 0)
        let originalID = original.identifier
        activate(container.buttons["layer-New layer"])
        expectation(for: NSPredicate { _, _ in rows.count == initialCount + 1 }, evaluatedWith: container)
        waitForExpectations(timeout: 5)
        let current = rows.element(boundBy: 0)
        let currentID = current.identifier
        XCTAssertNotEqual(currentID, originalID)
        let originalRow = candidates[originalID].firstMatch
        let checked = originalRow.buttons.matching(NSPredicate(format: "label == %@", "Select layer without changing drawing target")).firstMatch
        let currentRow = candidates[currentID].firstMatch
        let content = currentRow.buttons["Edit layer content"]
        XCTAssertTrue(content.isSelected, "New layer must become the drawing target")
        activate(checked)
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: checked)
        waitForExpectations(timeout: 5)
        XCTAssertTrue(content.isSelected, "Checking another row must preserve the drawing target")
        activate(container.buttons["layer-Add mask"])
        let mask = currentRow.buttons["Edit layer mask"]
        XCTAssertTrue(mask.waitForExistence(timeout: 5))
        activate(mask)
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: mask)
        waitForExpectations(timeout: 5)
        XCTAssertFalse(content.isSelected)
        activate(content)
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: content)
        waitForExpectations(timeout: 5)
        #if os(macOS)
        mask.rightClick()
        #else
        mask.press(forDuration: 0.6)
        #endif
        let deleteMask = container.buttons["Delete mask"]
        XCTAssertTrue(deleteMask.waitForExistence(timeout: 5), "The mask context gesture must open its mask menu")
        let menuCapture = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        menuCapture.name = "layer-mask-context-anchor"; menuCapture.lifetime = .keepAlways; add(menuCapture)
        activate(deleteMask)
        expectation(for: NSPredicate(format: "exists == NO"), evaluatedWith: mask)
        waitForExpectations(timeout: 5)
        XCTAssertFalse(container.staticTexts["Canvas error"].exists)
    }
    @MainActor private func activate(_ element: XCUIElement, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(element.exists, "Missing layer control: \(element)", file: file, line: line)
        element.clickOrTap()
    }
}
