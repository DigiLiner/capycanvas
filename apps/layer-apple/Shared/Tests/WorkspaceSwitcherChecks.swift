import XCTest

extension XCTestCase {
    @MainActor func checkWorkspaceSwitcher(in app: XCUIApplication) {
        app.launchEnvironment["CAPY_INITIAL_ACTIONS"] = #"[{"type":"workspace_manager","command":{"type":"manage"}}]"#
        app.launch()
        let painter = app.descendants(matching: .any)["workspace-select-builtin:workspace:painter"].firstMatch
        let illustrator = app.descendants(matching: .any)["workspace-select-builtin:workspace:illustrator"].firstMatch
        let photographer = app.descendants(matching: .any)["workspace-select-builtin:workspace:photographer"].firstMatch
        let grip = app.buttons["workspace-grip-builtin:workspace:painter"]
        let menu = app.descendants(matching: .any)["workspace-row-menu"].firstMatch
        let pin = app.buttons["workspace-pin-builtin:workspace:painter"]
        func activateMenu(_ control: XCUIElement) {
            XCTAssertTrue(control.waitForExistence(timeout: 10))
            if control.isHittable { workspaceActivate(control); return }
            // XCTest can omit a hit point for offset SwiftUI overlays even
            // though their visible bounds are correct. Deliver a native pointer
            // inside the measured menu and verify the resulting editor state.
            let bounds = control.frame, viewport = workspaceViewport(in: app)
            guard bounds.width > 0, bounds.height > 0, viewport.frame.contains(bounds), menu.frame.contains(bounds) else {
                XCTFail("The menu action must be inside its visible menu and app viewport"); return
            }
            let point = viewport.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(
                dx: bounds.midX - viewport.frame.minX, dy: bounds.midY - viewport.frame.minY))
            point.clickOrTap()
        }
        XCTAssertTrue(painter.waitForExistence(timeout: 30))
        let create = app.buttons["workspace-action-new"], remove = app.buttons["workspace-action-delete"]
        let rows = app.descendants(matching: .any)["workspace-manager-rows"].firstMatch
        XCTAssertTrue(create.waitForExistence(timeout: 10)); XCTAssertTrue(remove.exists)
        XCTAssertFalse(remove.isEnabled, "Included workspaces cannot be deleted")
        XCTAssertGreaterThanOrEqual(create.frame.minY, rows.frame.maxY - 1)
        XCTAssertGreaterThanOrEqual(remove.frame.minX, create.frame.maxX - 1)
        XCTAssertGreaterThanOrEqual(grip.frame.minX, pin.frame.maxX - 1)
        XCTAssertFalse(app.textFields["workspace-manager-search"].exists)
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "workspace-options-")).firstMatch.exists)
        workspaceActivate(illustrator)
        XCTAssertTrue(illustrator.isSelected)
        XCTAssertTrue(pin.waitForExistence(timeout: 10)); XCTAssertTrue(pin.isSelected)
        workspaceActivate(pin)
        XCTAssertTrue(illustrator.isSelected)
        XCTAssertTrue(menu.waitForNonExistence(timeout: 10))

        // Native touch hold retains its menu after release. A fresh held contact
        // then continues into a drag; explicit grips require no hold.
        #if os(iOS)
        painter.press(forDuration: 0.8)
        XCTAssertTrue(menu.waitForExistence(timeout: 10))
        XCTAssertFalse(pin.isSelected)
        activateMenu(app.buttons["menu-action-Move Down"])
        workspaceActivate(grip); activateMenu(app.buttons["menu-action-Move Up"])
        workspaceActivate(pin)
        #else
        workspaceActivate(pin)
        #endif
        XCTAssertTrue(menu.waitForNonExistence(timeout: 10))
        func drag(_ source: XCUIElement, to target: XCUICoordinate, held: Bool) {
            let start = source.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            #if os(macOS)
            start.click(forDuration: 0.05, thenDragTo: target)
            #else
            start.press(forDuration: held ? 0.8 : 0.05, thenDragTo: target)
            #endif
        }
        #if os(iOS)
        drag(painter, to: photographer.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95)), held: false)
        XCTAssertLessThan(painter.frame.minY, illustrator.frame.minY, "Early row motion must not reorder before the native hold")
        XCTAssertTrue(illustrator.isSelected)
        #endif
        drag(painter, to: photographer.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95)), held: true)
        expectation(for: NSPredicate { _, _ in painter.frame.minY > photographer.frame.minY }, evaluatedWith: painter)
        waitForExpectations(timeout: 10)
        XCTAssertFalse(menu.exists, "Dragging with the held contact must close its menu")
        XCTAssertTrue(illustrator.isSelected, "A completed reorder must not select or preview the dragged row")
        drag(grip, to: illustrator.coordinate(withNormalizedOffset: CGVector(dx: 0.05, dy: 0.05)), held: false)
        expectation(for: NSPredicate { _, _ in painter.frame.minY < illustrator.frame.minY }, evaluatedWith: painter)
        waitForExpectations(timeout: 10)
        XCTAssertTrue(illustrator.isSelected)

        workspaceActivate(grip); activateMenu(app.buttons["menu-action-Move Down"])
        expectation(for: NSPredicate { _, _ in painter.frame.minY > illustrator.frame.minY && painter.frame.minY < photographer.frame.minY }, evaluatedWith: painter)
        waitForExpectations(timeout: 10)
        workspaceActivate(pin)
        XCTAssertTrue(menu.waitForNonExistence(timeout: 10))
        app.terminate(); app.launch()
        XCTAssertTrue(painter.waitForExistence(timeout: 30))
        XCTAssertGreaterThan(painter.frame.minY, illustrator.frame.minY)
        XCTAssertLessThan(painter.frame.minY, photographer.frame.minY)
        XCTAssertTrue(pin.waitForExistence(timeout: 10)); XCTAssertFalse(pin.isSelected)
        XCTAssertFalse(app.staticTexts["Canvas error"].exists)
        attachEditor(in: app, name: "workspace-switcher-restarted")
    }
}
