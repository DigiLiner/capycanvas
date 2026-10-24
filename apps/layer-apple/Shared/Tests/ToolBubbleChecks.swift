import XCTest

extension XCTestCase {
    @MainActor func checkToolBubbles(in app: XCUIApplication, theme: String) {
        app.launchEnvironment["CAPY_INITIAL_ACTIONS"] = #"[{"type":"set_theme","theme":"\#(theme)"},{"type":"workspace_manager","command":{"type":"switch","id":"builtin:workspace:painter"}}]"#
        app.launch()
        let canvas = app.descendants(matching: .any)["canvas"].firstMatch
        expectation(for: NSPredicate(format: "value == %@", "Canvas ready"), evaluatedWith: canvas)
        waitForExpectations(timeout: 60)
        let painter = app.buttons["workspace-switch-builtin:workspace:painter"]
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: painter)
        waitForExpectations(timeout: 30)
        let viewport = workspaceViewport(in: app)
        func destination(_ x: CGFloat, _ y: CGFloat) -> XCUICoordinate {
            viewport.coordinate(withNormalizedOffset: CGVector(dx: x, dy: y))
        }
        func take(_ source: XCUIElement, _ x: CGFloat, _ y: CGFloat) {
            XCTAssertTrue(source.waitForExistence(timeout: 10))
            source.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.85))
                .press(forDuration: 0.05, thenDragTo: destination(x, y), withVelocity: .slow, thenHoldForDuration: 0.1)
        }
        func faces() -> XCUIElementQuery {
            app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "bubble-face-"))
        }
        func menu(_ source: XCUIElement) {
            #if os(macOS)
            source.rightClick()
            #else
            source.press(forDuration: 1.2)
            #endif
            XCTAssertTrue(app.descendants(matching: .any)["bubble-context-menu"].firstMatch.waitForExistence(timeout: 10))
        }
        let brushSource = app.buttons["header-tool-drawing_brush"]
        take(brushSource, 0.32, 0.42)
        expectation(for: NSPredicate { _, _ in faces().count > 0 }, evaluatedWith: app)
        waitForExpectations(timeout: 10)
        let brush = faces().firstMatch
        XCTAssertTrue(brush.isSelected)
        expectation(for: NSPredicate { _, _ in
            #if os(macOS)
            let reference = app.windows.firstMatch.frame
            #else
            let reference = canvas.frame
            #endif
            let frame = brush.frame
            let points = [20.0, 32.0].map { distance in
                CGPoint(x: (frame.midX + distance - reference.minX) / reference.width,
                    y: (frame.midY - reference.minY) / reference.height)
            }
            let pixels = self.editorPixelSamples(in: app, at: points, size: 1)
            return (0..<3).map { abs(Int(pixels[0][$0]) - Int(pixels[1][$0])) }.max()! > 12
        }, evaluatedWith: brush)
        waitForExpectations(timeout: 10)
        attachEditor(in: app, name: "bubble-brush-halo-\(theme)")
        menu(brush)
        let putBack = app.buttons["menu-action-Put back"]
        XCTAssertTrue(putBack.waitForExistence(timeout: 5))
        workspaceActivate(putBack)
        expectation(for: NSPredicate { _, _ in faces().count == 0 }, evaluatedWith: app)
        waitForExpectations(timeout: 10)
        take(brushSource, 0.32, 0.42)
        XCTAssertTrue(brush.waitForExistence(timeout: 10))
        let colorSource = app.buttons.matching(NSPredicate(format: "label == %@", "Brush color")).firstMatch
        take(colorSource, 0.68, 0.42)
        let wheel = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "bubble-wheel-")).firstMatch
        XCTAssertTrue(wheel.waitForExistence(timeout: 10))
        let wheelPoint = wheel.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.12))
        wheelPoint.press(forDuration: 0.05, thenDragTo: wheel.coordinate(withNormalizedOffset: CGVector(dx: 0.85, dy: 0.5)),
            withVelocity: .slow, thenHoldForDuration: 0.1)
        menu(wheel)
        let recent = app.buttons["menu-action-Recent colors"], pair = app.buttons["menu-action-Primary and secondary"]
        XCTAssertTrue(recent.waitForExistence(timeout: 5)); workspaceActivate(recent)
        XCTAssertTrue(pair.waitForExistence(timeout: 5), "Changing one option retains the menu")
        workspaceActivate(pair)
        XCTAssertTrue(putBack.exists, "Several option edits retain Put back")
        workspaceActivate(putBack)
        XCTAssertTrue(wheel.waitForNonExistence(timeout: 10))
        take(colorSource, 0.68, 0.42)
        let background = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@", "bubble-bead-", "-background-0")).firstMatch
        let foreground = app.descendants(matching: .any).matching(NSPredicate(
            format: "identifier BEGINSWITH %@ AND identifier ENDSWITH %@", "bubble-bead-", "-foreground-0")).firstMatch
        XCTAssertTrue(background.waitForExistence(timeout: 10))
        XCTAssertTrue(foreground.waitForExistence(timeout: 10))
        expectation(for: NSPredicate { _, _ in
            #if os(macOS)
            let reference = app.windows.firstMatch.frame
            #else
            let reference = canvas.frame
            #endif
            let back = background.frame, front = foreground.frame
            let dx = back.midX - front.midX, dy = back.midY - front.midY, distance = max(1, hypot(dx, dy))
            let point = CGPoint(x: (back.midX + dx / distance * back.width / 4 - reference.minX) / reference.width,
                y: (back.midY + dy / distance * back.width / 4 - reference.minY) / reference.height)
            let pixel = self.editorPixels(in: app, at: point, size: 1)
            return (0..<3).allSatisfy { pixel[$0] >= 250 }
        }, evaluatedWith: background)
        waitForExpectations(timeout: 10)
        menu(wheel); workspaceActivate(putBack)
        XCTAssertTrue(wheel.waitForNonExistence(timeout: 10))
        let layersSource = app.buttons.matching(NSPredicate(format: "identifier == %@ AND label == %@", "header-tool-panel", "Layers panel")).firstMatch
        take(layersSource, 0.68, 0.5)
        let card = app.descendants(matching: .any)["layer-properties-pages"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 10))
        let add = app.buttons["bubble-new-layer"], properties = app.buttons["layer-properties-disclosure"]
        XCTAssertTrue(add.waitForExistence(timeout: 5))
        let rows = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH %@", "layer-row-"))
        let initial = rows.count
        XCTAssertGreaterThan(initial, 0, "The card exposes its layer rows")
        XCTAssertTrue(add.isEnabled)
        workspaceActivate(add)
        let added = XCTAttachment(string: app.debugDescription)
        added.name = "bubble-layers-after-add-\(theme)"; self.add(added)
        expectation(for: NSPredicate { _, _ in rows.count > initial }, evaluatedWith: card)
        waitForExpectations(timeout: 10)
        workspaceActivate(properties)
        expectation(for: NSPredicate(format: "label == %@", "All layers"), evaluatedWith: properties)
        waitForExpectations(timeout: 10)
        workspaceActivate(properties)
        XCTAssertTrue(add.waitForExistence(timeout: 10))
        attachEditor(in: app, name: "bubble-layers-card-\(theme)")
        destination(0.48, 0.7).tap()
        XCTAssertTrue(card.waitForNonExistence(timeout: 10), "An outside contact folds the card")
        let zen = app.buttons.matching(NSPredicate(format: "label == %@", "Zen mode")).firstMatch
        workspaceActivate(zen)
        XCTAssertTrue(brushSource.waitForNonExistence(timeout: 10), "Zen hides the header source")
        XCTAssertGreaterThan(faces().count, 0, "Bubbles remain accessible in Zen")
        attachEditor(in: app, name: "bubble-zen-\(theme)")
        XCTAssertFalse(app.staticTexts["Canvas error"].exists)
    }
}
