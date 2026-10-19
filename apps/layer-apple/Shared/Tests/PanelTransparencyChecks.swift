import XCTest

extension XCTestCase {
    @MainActor func checkPanelTransparency(in app: XCUIApplication) {
        app.launchEnvironment["CAPY_INITIAL_ACTIONS"] = #"[{"type":"set_theme","theme":"light"},{"type":"invoke","command":"settings"},{"type":"preferences","action":{"type":"reveal","id":"transparency"}}]"#
        app.launch()
        let lowCircle = app.buttons["preference-transparency-1"]
        XCTAssertTrue(lowCircle.waitForExistence(timeout: 20), "Apple offers Panel transparency")
        XCTAssertTrue(lowCircle.isSelected, "Low is the shared default")
        XCTAssertEqual((0...3).filter { app.buttons["preference-transparency-\($0)"].exists }.count, 4)
        let highCircle = app.buttons["preference-transparency-3"]
        workspaceActivate(highCircle)
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: highCircle)
        waitForExpectations(timeout: 5)
        attachEditor(in: app, name: "settings-transparency-circles")
        #if os(macOS)
        highCircle.rightClick()
        let reset = app.menuItems["Reset to Default"]
        #else
        highCircle.press(forDuration: 0.7)
        let reset = app.buttons["Reset to Default"]
        #endif
        XCTAssertTrue(reset.waitForExistence(timeout: 5)); workspaceActivate(reset)
        expectation(for: NSPredicate(format: "selected == YES"), evaluatedWith: lowCircle)
        waitForExpectations(timeout: 5)
        app.terminate()

        func panelSample(level: Int, theme: String = "light") -> [Double] {
            app.launchEnvironment["CAPY_INITIAL_ACTIONS"] = "[{\"type\":\"set_theme\",\"theme\":\"\(theme)\"},"
                + "{\"type\":\"preferences\",\"action\":{\"type\":\"edit\",\"id\":\"transparency\",\"value\":\(level)}}]"
            app.launch(); capturePaintEditor(in: app, theme: theme)
            let zoom = app.buttons["navigator-zoom_in"]
            XCTAssertTrue(zoom.waitForExistence(timeout: 10))
            for _ in 0..<4 { workspaceActivate(zoom) }
            let group = app.descendants(matching: .any)["workspace-group-15"].firstMatch
            XCTAssertTrue(group.waitForExistence(timeout: 10))
            let tab = group.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@ AND selected == YES", "panel-tab-")).firstMatch
            XCTAssertTrue(tab.waitForExistence(timeout: 10))
            #if os(macOS)
            let frame = app.windows.firstMatch.frame
            #else
            let frame = app.descendants(matching: .any)["canvas"].firstMatch.frame
            #endif
            let point = { (x: CGFloat, y: CGFloat) in CGPoint(x: (x - frame.minX) / frame.width, y: (y - frame.minY) / frame.height) }
            expectation(for: NSPredicate { _, _ in true }, evaluatedWith: app)
            waitForExpectations(timeout: 2)
            let samples = editorPixelSamples(in: app, at: [point(group.frame.minX + 24, group.frame.maxY - 30),
                point(tab.frame.minX + 4, tab.frame.maxY - 4), point(tab.frame.minX + 4, tab.frame.maxY + 3)], size: 4).map { (data: Data) -> [Double] in
                [0, 1, 2].map { channel in stride(from: channel, to: data.count, by: 4).map { Double(data[$0]) }.reduce(0, +) / Double(data.count / 4) }
            }
            let name = "panel-transparency-\(theme)-\(level)"
            attachEditor(in: app, name: name)
            app.terminate()
            XCTAssertTrue(zip(samples[1], samples[2]).allSatisfy { abs($0 - $1) <= 2 },
                "The active tab matches its panel body in \(name): tab \(samples[1]) panel \(samples[2])")
            return samples[0]
        }
        let light = (0...3).map { panelSample(level: $0) }
        for level in 0...3 { _ = panelSample(level: level, theme: "dark") }
        let opaque = light[0], low = light[1], high = light[3]
        let brightness = { (rgb: [Double]) in rgb.reduce(0, +) / 3 }
        XCTAssertGreaterThan(brightness(low), brightness(opaque) + 2, "White paper shows through Low glass: \(opaque) \(low)")
        XCTAssertGreaterThan(brightness(high), brightness(low) + 2, "High shows more of the artwork: \(low) \(high)")
        XCTAssertFalse(app.staticTexts["Canvas error"].exists)
    }
}
