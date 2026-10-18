import Foundation

@main struct ReorderContactChecks {
    @MainActor static func main() {
        let origin = CGPoint(x: 10, y: 20), destination = CGPoint(x: 80, y: 90)
        for surface: ReorderSurface in [.tile, .row, .handle, .headerEditor, .control, .swatch] {
            for device: ReorderDevice in [.mouse, .touch, .pen] {
                var events: [String] = [], valid = true
                let contact = ReorderContact()
                let target = ReorderTarget(id: "source", surface: surface, valid: { _ in valid },
                    openContext: { events.append("menu") }, closeContext: { events.append("close") },
                    begin: { precondition($0 == origin); events.append("begin") },
                    move: { precondition($0 == destination); events.append("move") },
                    finish: { precondition($0 == destination); events.append("finish") }, cancel: { events.append("cancel") })
                let heldRequired = surface == .tile || (surface == .row && device != .mouse)
                contact.prepare(target, device: device, origin: origin)
                contact.release(at: origin)
                precondition(events.isEmpty, "Short clicks must not open a menu or edit history")
                contact.prepare(target, device: device, origin: origin)
                precondition(contact.move(to: destination) != heldRequired, "Reject early tile/touch/pen-row pickup; admit handles and mouse rows")
                if heldRequired { precondition(events.isEmpty) }
                else { precondition(events == ["close", "begin", "move"]) }
                contact.cancel(); events.removeAll()
                contact.prepare(target, device: device, origin: origin)
                contact.recognizeHold(); contact.recognizeHold()
                precondition(events == (device == .mouse ? [] : ["menu"]), "Only touch/pen holds open menus; holds never begin history")
                precondition(contact.consumeClick() && !contact.consumeClick())
                contact.release(at: origin)
                precondition(events == (device == .mouse ? [] : ["menu"]), "Stationary hold release keeps touch/pen menus")
                events.removeAll()
                contact.prepare(target, device: device, origin: origin)
                contact.recognizeHold(); precondition(contact.move(to: destination))
                contact.move(to: destination); contact.release(at: destination); contact.release(at: destination)
                precondition(events == (device == .mouse ? [] : ["menu"]) + ["close", "begin", "move", "move", "finish"], "Held contact closes the menu and commits once")
                events.removeAll()
                contact.prepare(target, device: device, origin: origin)
                contact.recognizeHold(); contact.move(to: destination); contact.cancel(); contact.cancel()
                precondition(events == (device == .mouse ? [] : ["menu"]) + ["close", "begin", "move", "close", "cancel"], "Capture loss rolls back once")
                events.removeAll()
                contact.prepare(target, device: device, origin: origin)
                valid = false; contact.recognizeHold(); contact.move(to: destination); contact.release(at: destination)
                precondition(events.isEmpty && contact.target == nil, "Removed sources cannot arm or commit")
            }
        }
        for device: ReorderDevice in [.mouse, .touch, .pen] {
            var menus = 0, starts = 0, finishes = 0
            let contact = ReorderContact()
            let target = ReorderTarget(id: "layer", surface: .row, dragDevices: [.mouse], valid: { _ in true },
                openContext: { menus += 1 }, begin: { _ in starts += 1 }, move: { _ in },
                finish: { _ in finishes += 1 }, cancel: {})
            contact.prepare(target, device: device, origin: origin)
            contact.recognizeHold()
            precondition(contact.move(to: destination) == (device == .mouse), "Layer body pickup retains the admitted device after a hold")
            contact.release(at: destination)
            precondition(menus == (device == .mouse ? 0 : 1) && starts == (device == .mouse ? 1 : 0) && finishes == starts,
                "Direct-contact layer bodies retain context without an artwork transaction")
        }
        print("PASS: eighteen generic surface/device combinations and layer body device gates preserve context, cancellation and one transaction")
    }
}
