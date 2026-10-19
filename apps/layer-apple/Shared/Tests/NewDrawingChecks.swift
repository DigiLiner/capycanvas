import XCTest

extension XCTestCase {
    @MainActor func newDrawingNumber(_ name: String, in app: XCUIApplication) -> XCUIElement {
        app.textFields["number-entry-new-drawing-" + name]
    }
    @MainActor func newDrawingElement(_ id: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }
    @MainActor func chooseNewDrawingOption(_ id: String, _ index: Int, in app: XCUIApplication) {
        #if os(iOS)
        dismissEditorKeyboard(in: app)
        #endif
        workspaceActivate(app.buttons[id].firstMatch)
        workspaceActivate(app.buttons["\(id)-option-\(index)"].firstMatch)
    }
    @MainActor func enterNewDrawingNumber(_ name: String, _ value: String, in app: XCUIApplication) {
        let field = newDrawingNumber(name, in: app)
        replaceEditorText(field, with: value, in: app)
        #if os(macOS)
        field.typeKey(XCUIKeyboardKey.tab.rawValue, modifierFlags: [])
        #else
        dismissEditorKeyboard(in: app)
        #endif
    }
    @MainActor func expectNewDrawingText(_ id: String, in app: XCUIApplication, _ shows: @escaping (String) -> Bool) {
        let element = newDrawingElement(id, in: app)
        expectation(for: NSPredicate { _, _ in shows(self.editorText(element)) }, evaluatedWith: app)
        waitForExpectations(timeout: 15)
    }
    @MainActor func expectNewDrawingValue(_ id: String, _ value: String, in app: XCUIApplication) {
        expectation(for: NSPredicate(format: "value == %@", value), evaluatedWith: app.buttons[id].firstMatch)
        waitForExpectations(timeout: 15)
    }
    @MainActor func createNewDrawing(width: String, height: String, depth: Int? = nil, in app: XCUIApplication) {
        let create = app.buttons["new-drawing-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 60), "The New drawing dialog did not open")
        chooseNewDrawingOption("new-drawing-unit", 3, in: app)
        enterNewDrawingNumber("width", width, in: app)
        enterNewDrawingNumber("height", height, in: app)
        if let depth { chooseNewDrawingOption("new-drawing-depth", depth, in: app) }
        workspaceActivate(create)
        XCTAssertTrue(create.waitForNonExistence(timeout: 60))
    }

    @MainActor func checkNewDrawingDialog(in app: XCUIApplication, theme: String) {
        let create = app.buttons["new-drawing-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 60), "The New drawing dialog did not open after canvas startup")
        expectNewDrawingText("new-drawing-caption", in: app) { $0.hasPrefix("Sketchbook A4") }
        let resolution = newDrawingNumber("resolution", in: app)
        XCTAssertEqual(resolution.value as? String, "300")
        XCTAssertTrue(create.isEnabled)
        attachEditor(in: app, name: "new-drawing-main-" + theme)

        workspaceActivate(app.buttons["new-drawing-resolution-choices"])
        let line = app.buttons["new-drawing-resolution-4"]
        XCTAssertTrue(line.waitForExistence(timeout: 10))
        attachEditor(in: app, name: "new-drawing-resolutions-" + theme)
        workspaceActivate(line)
        expectation(for: NSPredicate(format: "value == %@", "600"), evaluatedWith: resolution)
        waitForExpectations(timeout: 15)

        workspaceActivate(app.buttons["new-drawing-presets-button"])
        let search = app.textFields["new-drawing-search"]
        XCTAssertTrue(search.waitForExistence(timeout: 10))
        XCTAssertTrue(create.waitForNonExistence(timeout: 5), "The presets cover the page")
        attachEditor(in: app, name: "new-drawing-presets-" + theme)
        replaceEditorText(search, with: "chalk", in: app)
        let chalkboard = app.buttons["new-drawing-preset-0"]
        expectation(for: NSPredicate(format: "label == %@", "Chalkboard"), evaluatedWith: chalkboard)
        waitForExpectations(timeout: 15)
        #if os(iOS)
        dismissEditorKeyboard(in: app)
        #endif
        workspaceActivate(chalkboard)
        expectNewDrawingText("new-drawing-summary-name", in: app) { $0 == "Chalkboard" }
        expectNewDrawingText("new-drawing-summary-details", in: app) { $0.hasSuffix("Sanded 50 %, Chalkboard") }
        workspaceActivate(app.buttons["new-drawing-save-as"])
        let saveName = app.textFields["new-drawing-save-name"]
        XCTAssertTrue(saveName.waitForExistence(timeout: 10))
        replaceEditorText(saveName, with: "Studio wall", in: app)
        #if os(iOS)
        dismissEditorKeyboard(in: app)
        #endif
        attachEditor(in: app, name: "new-drawing-save-as-" + theme)
        workspaceActivate(app.buttons["new-drawing-save-confirm"])
        XCTAssertTrue(saveName.waitForNonExistence(timeout: 10))
        expectNewDrawingText("new-drawing-summary-name", in: app) { $0 == "Studio wall" }
        workspaceActivate(app.buttons["new-drawing-select"])
        XCTAssertTrue(search.waitForNonExistence(timeout: 10))
        expectNewDrawingText("new-drawing-caption", in: app) { $0.hasPrefix("Studio wall") }
        XCTAssertEqual(resolution.value as? String, "150", "Selecting a preset restores its resolution")

        enterNewDrawingNumber("resolution", "10000", in: app)
        expectation(for: NSPredicate(format: "enabled == NO"), evaluatedWith: create)
        waitForExpectations(timeout: 15)
        expectNewDrawingText("new-drawing-size", in: app) { $0.hasPrefix("Too large for this device") }
        enterNewDrawingNumber("resolution", "150", in: app)
        expectation(for: NSPredicate(format: "enabled == YES"), evaluatedWith: create)
        waitForExpectations(timeout: 15)

        checkNewDrawingPaper(in: app, theme: theme)

        let paperColor = app.buttons["new-drawing-paper-color"]
        workspaceActivate(paperColor)
        let useColor = app.buttons["new-drawing-color-use"]
        XCTAssertTrue(useColor.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["color-pick"].exists, "The paper color has no eyedropper")
        let kraft = app.buttons["new-drawing-paper-color-5"]
        XCTAssertEqual(kraft.label, "Kraft")
        workspaceActivate(kraft)
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: kraft)
        waitForExpectations(timeout: 10)
        attachEditor(in: app, name: "new-drawing-paper-color-" + theme)
        workspaceActivate(useColor)
        XCTAssertTrue(useColor.waitForNonExistence(timeout: 10))
        expectNewDrawingValue("new-drawing-paper-color", "Kraft", in: app)
        workspaceActivate(paperColor)
        XCTAssertTrue(useColor.waitForExistence(timeout: 10))
        workspaceActivate(app.buttons["new-drawing-color-mode-transparent"])
        XCTAssertTrue(app.staticTexts["color-transparent"].waitForExistence(timeout: 10))
        attachEditor(in: app, name: "new-drawing-transparent-" + theme)
        workspaceActivate(useColor)
        XCTAssertTrue(useColor.waitForNonExistence(timeout: 10))
        expectNewDrawingValue("new-drawing-paper-color", "Transparent", in: app)

        chooseNewDrawingOption("new-drawing-unit", 3, in: app)
        enterNewDrawingNumber("width", "640", in: app)
        enterNewDrawingNumber("height", "480", in: app)
        let name = app.textFields["new-drawing-name"]
        replaceEditorText(name, with: "Harbor study", in: app)
        #if os(iOS)
        dismissEditorKeyboard(in: app)
        #endif
        workspaceActivate(create)
        XCTAssertTrue(create.waitForNonExistence(timeout: 60))
        let title = editorDocumentTitle(in: app)
        expectation(for: NSPredicate(format: "label BEGINSWITH %@ OR value BEGINSWITH %@", "Harbor study", "Harbor study"), evaluatedWith: title)
        waitForExpectations(timeout: 30)
        let created = editorText(title)
        attachEditor(in: app, name: "new-drawing-created-" + theme)

        #if os(macOS)
        app.typeKey("n", modifierFlags: .command)
        #else
        editorMenu(in: app, menu: "File", id: "new_document", label: "New drawing")
        #endif
        XCTAssertTrue(create.waitForExistence(timeout: 15))
        XCTAssertEqual(newDrawingNumber("width", in: app).value as? String, "640", "New reopens with the last drawing")
        workspaceActivate(app.buttons["new-drawing-cancel"])
        XCTAssertTrue(create.waitForNonExistence(timeout: 15))
        XCTAssertEqual(editorText(title), created, "Cancel keeps the current drawing")
        XCTAssertFalse(app.staticTexts["Canvas error"].exists)
    }

    @MainActor func checkNewDrawingPaper(in app: XCUIApplication, theme: String) {
        let textureTile = app.buttons["new-drawing-texture"], useTexture = app.buttons["new-drawing-paper-use"]
        let relief = newDrawingNumber("paper-relief", in: app)
        expectNewDrawingValue("new-drawing-texture", "Sanded, 50 %", in: app)
        workspaceActivate(textureTile)
        XCTAssertTrue(useTexture.waitForExistence(timeout: 10))
        XCTAssertEqual(relief.value as? String, "50", "Texture visibility opens with the sheet's value")
        XCTAssertTrue(relief.isEnabled)
        let noPaper = app.buttons["new-drawing-paper-tile-0"]
        XCTAssertTrue(noPaper.waitForExistence(timeout: 10))
        let none = noPaper.label
        workspaceActivate(noPaper)
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: noPaper)
        expectation(for: NSPredicate(format: "enabled == NO"), evaluatedWith: relief)
        waitForExpectations(timeout: 10)
        attachEditor(in: app, name: "new-drawing-no-paper-" + theme)
        workspaceActivate(useTexture)
        XCTAssertTrue(useTexture.waitForNonExistence(timeout: 10))
        expectNewDrawingValue("new-drawing-texture", none, in: app)

        let coldPress = app.buttons["new-drawing-paper-tile-2"]
        func pickColdPress() {
            workspaceActivate(textureTile)
            XCTAssertTrue(useTexture.waitForExistence(timeout: 10))
            XCTAssertFalse(relief.isEnabled, "No paper has no texture to show")
            workspaceActivate(coldPress)
            expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: coldPress)
            expectation(for: NSPredicate(format: "enabled == YES"), evaluatedWith: relief)
            waitForExpectations(timeout: 10)
            XCTAssertEqual(relief.value as? String, "50")
        }
        pickColdPress()
        let texture = coldPress.label
        let importTile = app.buttons["new-drawing-paper-import"]
        XCTAssertTrue(importTile.waitForExistence(timeout: 10))
        XCTAssertEqual(importTile.label, "Import…")
        XCTAssertEqual(importTile.frame.size, noPaper.frame.size, "Import… is a tile like No paper")
        revealEditorControl(importTile, in: app.scrollViews.containing(.button, identifier: importTile.identifier).firstMatch)
        XCTAssertEqual(newDrawingElement("new-drawing-papers", in: app).buttons.allElementsBoundByIndex.last?.identifier,
            importTile.identifier, "Import… follows the imported papers")
        attachEditor(in: app, name: "new-drawing-paper-import-" + theme)
        workspaceActivate(importTile)
        #if os(macOS)
        let chooser = app.windows.buttons["OKButton"].firstMatch
        XCTAssertTrue(chooser.waitForExistence(timeout: 20), "Import… opens the file chooser")
        app.typeKey(XCUIKeyboardKey.escape.rawValue, modifierFlags: [])
        #else
        let chooser = app.buttons.matching(NSPredicate(format: "label == %@ AND NOT (identifier BEGINSWITH %@)", "Cancel", "new-drawing-")).firstMatch
        XCTAssertTrue(chooser.waitForExistence(timeout: 20), "Import… opens the file chooser")
        workspaceActivate(chooser)
        #endif
        XCTAssertTrue(chooser.waitForNonExistence(timeout: 15))
        XCTAssertTrue(useTexture.exists, "Cancelling the import keeps the paper drawer")
        XCTAssertTrue(coldPress.isSelected, "Import… keeps the marked paper")
        XCTAssertFalse(importTile.isSelected)

        replaceEditorText(relief, with: "250", in: app)
        #if os(iOS)
        dismissEditorKeyboard(in: app)
        #endif
        workspaceActivate(app.buttons["new-drawing-paper-cancel"])
        XCTAssertTrue(useTexture.waitForNonExistence(timeout: 10))
        expectNewDrawingValue("new-drawing-texture", none, in: app)

        pickColdPress()
        replaceEditorText(relief, with: "150", in: app)
        #if os(iOS)
        dismissEditorKeyboard(in: app)
        #endif
        attachEditor(in: app, name: "new-drawing-paper-" + theme)
        workspaceActivate(useTexture)
        XCTAssertTrue(useTexture.waitForNonExistence(timeout: 10))
        expectNewDrawingValue("new-drawing-texture", texture + ", 150 %", in: app)
        attachEditor(in: app, name: "new-drawing-paper-relief-" + theme)
    }
}
