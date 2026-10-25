import XCTest

extension EditorLaunchTests {
    @MainActor func testNativeZenContextAction() {
        XCUIDevice.shared.orientation = .landscapeLeft
        for theme in ["light", "dark"] {
            let app = editorTestApplication()
            app.launchEnvironment["CAPY_INITIAL_ACTIONS"] = #"[{"type":"set_theme","theme":"\#(theme)"},{"type":"customize","action":{"type":"set_panel_visible","panel":"layers","visible":true}}]"#
            app.launch()
            let zen = app.descendants(matching: .any)["zen-button"].firstMatch.buttons.firstMatch
            XCTAssertTrue(zen.waitForExistence(timeout: 20))
            zen.press(forDuration: 0.8)
            let icon = app.buttons["Change icon…"]
            XCTAssertTrue(icon.waitForExistence(timeout: 5))
            XCTAssertTrue(app.buttons["layer-New layer"].exists, "Opening the native menu must suppress the Zen button tap")
            icon.tap()
            let choices = (0..<4).map { app.buttons["preference-zen_icon-\($0)"] }
            for choice in choices { XCTAssertTrue(choice.waitForExistence(timeout: 10)) }
            XCTAssertEqual(choices.filter { $0.isSelected }.count, 1)
            XCTAssertTrue(choices[0].isSelected)
            XCTAssertFalse(app.staticTexts["Canvas error"].exists)
            app.terminate()
        }
    }
}

extension XCTestCase {
    @MainActor func checkNativeWorkspaceContextAction() {
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = editorCaptureApplication()
        app.launchEnvironment["CAPY_INITIAL_ACTIONS"] = #"[{"type":"set_theme","theme":"dark"},{"type":"workspace_manager","command":{"type":"manage"}}]"#
        app.launch()
        let painter = app.descendants(matching: .any)["workspace-select-builtin:workspace:painter"].firstMatch
        let illustrator = app.descendants(matching: .any)["workspace-select-builtin:workspace:illustrator"].firstMatch
        let photographer = app.descendants(matching: .any)["workspace-select-builtin:workspace:photographer"].firstMatch
        XCTAssertTrue(painter.waitForExistence(timeout: 30))
        workspaceActivate(illustrator)
        XCTAssertTrue(illustrator.isSelected)
        painter.press(forDuration: 0.8)
        let pin = app.buttons["Show in top bar"]
        let down = app.buttons["Move Down"]
        XCTAssertTrue(pin.waitForExistence(timeout: 5))
        XCTAssertTrue(down.waitForExistence(timeout: 5))
        XCTAssertLessThan(pin.frame.maxY, down.frame.minY, "Row menus must retain a vertical layout")
        XCTAssertTrue(illustrator.isSelected, "A hold must suppress the row's ordinary selection")
        let capture = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        capture.name = "native-workspace-context-dark"; capture.lifetime = .keepAlways; add(capture)
        down.tap()
        XCTAssertTrue(down.waitForNonExistence(timeout: 5))
        expectation(for: NSPredicate { _, _ in painter.frame.minY > illustrator.frame.minY && painter.frame.minY < photographer.frame.minY }, evaluatedWith: painter)
        waitForExpectations(timeout: 10)
        XCTAssertTrue(illustrator.isSelected)
        app.terminate(); app.launch()
        XCTAssertTrue(painter.waitForExistence(timeout: 30))
        XCTAssertGreaterThan(painter.frame.minY, illustrator.frame.minY, "The native action must persist the shared order")
        XCTAssertLessThan(painter.frame.minY, photographer.frame.minY)
    }
}
