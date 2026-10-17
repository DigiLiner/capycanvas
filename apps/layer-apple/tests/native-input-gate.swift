import Foundation
import os

@main struct NativeInputGateChecks {
    static func main() {
        let gate = NativeInputGate()
        for _ in 0..<8 {
            let winners = OSAllocatedUnfairLock(initialState: 0)
            DispatchQueue.concurrentPerform(iterations: 1000) { _ in
                if gate.claim() { winners.withLock { $0 += 1 } }
            }
            precondition(winners.withLock { $0 } == 1, "A held gate must admit exactly one concurrent caller")
            precondition(!gate.claim(), "Admission must stay closed until release")
            gate.release()
            precondition(gate.claim(), "Release must reopen admission")
            gate.release()
        }
        print("PASS: native input gate concurrent admission, release and repeated cycles")
    }
}
