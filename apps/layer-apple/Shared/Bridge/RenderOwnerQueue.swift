import Foundation

final class RenderOwnerQueue: @unchecked Sendable {
    typealias Work = @Sendable () -> Void
    private final class State: @unchecked Sendable {
        let condition = NSCondition()
        var jobs: [Work?] = []
        var head = 0
        var suspended = 0
        var closed = false

        func append(_ work: @escaping Work) {
            condition.lock(); defer { condition.unlock() }
            guard !closed else { return }
            jobs.append(work); condition.signal()
        }
        func finish(_ work: @escaping Work) {
            condition.lock(); defer { condition.unlock() }
            guard !closed else { return }
            jobs.append(work); closed = true; suspended = 0; condition.signal()
        }
        func next() -> Work? {
            condition.lock(); defer { condition.unlock() }
            while suspended > 0 || head == jobs.count {
                if closed && head == jobs.count { return nil }
                condition.wait()
            }
            let work = jobs[head]; jobs[head] = nil; head += 1
            if head == jobs.count { jobs.removeAll(keepingCapacity: true); head = 0 }
            else if head >= 64 && head >= jobs.count / 2 { jobs.removeFirst(head); head = 0 }
            return work
        }
        func run() {
            while let work = next() { autoreleasepool(invoking: work) }
        }
    }
    private let state = State()

    init() {
        let state = state
        let thread = Thread { state.run() }
        thread.name = "art.capycanvas.render"
        thread.qualityOfService = .userInteractive
        thread.stackSize = 8 * 1024 * 1024
        thread.start()
    }
    func async(execute work: @escaping Work) { state.append(work) }
    func asyncAfter(deadline: DispatchTime, execute work: @escaping Work) {
        DispatchQueue.global(qos: .userInteractive).asyncAfter(deadline: deadline) { [weak self] in self?.async(execute: work) }
    }
    func suspend() {
        state.condition.lock(); defer { state.condition.unlock() }
        if !state.closed { state.suspended += 1 }
    }
    func resume() {
        state.condition.lock(); defer { state.condition.unlock() }
        if state.suspended > 0 { state.suspended -= 1; state.condition.signal() }
    }
    func finish(execute work: @escaping Work) { state.finish(work) }
    deinit { state.finish({}) }
}
