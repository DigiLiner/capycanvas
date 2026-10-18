import SwiftUI
import CoreGraphics

final class NativePaperTask: @unchecked Sendable {
    let handle: OpaquePointer
    init(_ handle: OpaquePointer) { self.handle = handle }
    deinit { let handle = handle; NativeProjectTask.io.async { capy_paper_free(handle) } }
    func cancel() { capy_paper_cancel(handle) }
    func work(_ url: URL?) {
        do {
            if let url {
                try ProjectFileIO.coordinate(url, writing: false) { source in
                    let file = try FileHandle(forReadingFrom: source)
                    defer { try? file.close() }
                    source.deletingPathExtension().lastPathComponent.withCString { capy_paper_work(self.handle, file.fileDescriptor, $0) }
                }
            } else { "".withCString { capy_paper_work(handle, -1, $0) } }
        } catch { error.localizedDescription.withCString { capy_paper_failed(handle, $0) } }
    }
}

final class NativePaperPreviews: @unchecked Sendable {
    private let handle: OpaquePointer
    init(_ handle: OpaquePointer) { self.handle = handle }
    deinit { let handle = handle; NativeProjectTask.io.async { capy_paper_previews_free(handle) } }
    struct Decoded: @unchecked Sendable { let images: [Int: CGImage] }
    func decode() -> Decoded {
        var images: [Int:CGImage] = [:]
        for index in 0..<capy_paper_previews_count(handle) {
            var bytes: UnsafePointer<UInt8>?, count = 0
            let side = Int(capy_paper_previews_read(handle,index,&bytes,&count))
            guard let bytes, side > 0, count == side * side else { continue }
            let data = Data(bytes:bytes,count:count)
            guard let provider = CGDataProvider(data:data as CFData),
                let image = CGImage(width:side,height:side,bitsPerComponent:8,bitsPerPixel:8,bytesPerRow:side,
                    space:CGColorSpaceCreateDeviceGray(),bitmapInfo:CGBitmapInfo(rawValue:CGImageAlphaInfo.none.rawValue),
                    provider:provider,decode:nil,shouldInterpolate:true,intent:.relativeColorimetric) else { continue }
            images[index] = image
        }
        return Decoded(images:images)
    }
}

struct PaperRename: Identifiable {
    let id = UUID()
    let action: JSON
}

@MainActor final class PaperController: ObservableObject {
    private weak var store: EditorStore?
    private let directory: URL?
    private var started = false
    private var handled = Set<UInt64>()
    private var importRequest: JSON?
    private var importEpoch: UInt64 = 0
    private var jobs: [UInt64:NativePaperTask] = [:]
    private var active = Set<UInt64>()
    private var cancelled = Set<UInt64>()
    private var reloadPending = false
    @Published var rename: PaperRename?
    @Published private(set) var generation: UInt64 = 0
    @Published private(set) var busy = false
    init(store: EditorStore, directory: URL?) { self.store = store; self.directory = directory }
    var common: JSON { store?.bootstrap["common"] ?? JSON() }
    var copy: JSON { store?.catalog["native_copy"]["paper"] ?? JSON() }
    func receive() {
        guard let store, directory != nil else { return }
        if !started { started = true; reload() }
        let requests = store.state["requests"].array.filter { $0["kind"]["type"].string == "paper" }
        let current = Set(requests.map { $0["id"].uint })
        handled.formIntersection(current)
        for request in requests where !handled.contains(request["id"].uint) {
            let id = request["id"].uint
            if request["kind"]["request"]["type"].string == "import" {
                guard importRequest == nil else { continue }
                handled.insert(id); importRequest = request; importEpoch = store.state["document_file"]["epoch"].uint
                store.projectFiles.choosePaper { [weak self] url in self?.imported(url) }
            } else { handled.insert(id); run(request) }
        }
    }
    func reload() { run(nil) }
    private func run(_ request: JSON?, url: URL? = nil, epoch: UInt64? = nil) {
        guard let store, let native = store.native, let directory else { return }
        let id = request?["id"].uint ?? 0
        if active.contains(id) { if id == 0 { reloadPending = true }; return }
        active.insert(id); cancelled.remove(id); busy = true
        let complete: @Sendable (String?) -> Void = { [weak self, weak store] error in
            DispatchQueue.main.async {
                guard let self else { return }
                self.jobs.removeValue(forKey:id); self.active.remove(id); self.cancelled.remove(id)
                self.busy = !self.active.isEmpty; self.generation &+= 1
                if let error { store?.failure = error }
                if let request, ["import","add","rename","delete"].contains(request["kind"]["request"]["type"].string), error == nil {
                    store?.paperLibraryChanged()
                }
                if id == 0 && self.reloadPending { self.reloadPending = false; self.reload() }
            }
        }
        native.paperTask(id:UInt32(id),epoch:epoch ?? store.state["document_file"]["epoch"].uint,directory:directory) { [weak self] job,error in
            DispatchQueue.main.async {
                guard let job else { complete(error); return }
                if let self {
                    self.jobs[id] = job
                    if self.cancelled.contains(id) { job.cancel() }
                } else { job.cancel() }
                native.runPaperTask(job,url:url,completion:complete)
            }
        }
    }

    private func imported(_ url: URL?) {
        guard let request = importRequest else { return }
        importRequest = nil
        if let url { run(request,url:url,epoch:importEpoch) }
        else { store?.dispatch(["type":"complete_request","id":request["id"].raw,"error":NSNull()]) }
    }
    func cancelWork() {
        cancelled.formUnion(active)
        for job in jobs.values { job.cancel() }
    }
    func action(_ item: JSON) {
        guard let store else { return }
        if item["action"]["op"].string == "rename_paper" {
            rename = PaperRename(action:item["action"])
        } else { store.dispatch(["type":"effect","action":item["action"].raw]) }
    }
    func renamed(_ name: String, completion: @escaping @MainActor (String?) -> Void) {
        guard let rename, let store else { return }
        var action = rename.action.object; action["name"] = name
        store.edit(["type":"effect","action":action]) { [weak self] error in
            if error == nil { self?.rename = nil }
            completion(error)
        }
    }
}

struct PaperFiles: ViewModifier {
    @ObservedObject var controller: PaperController
    func body(content: Content) -> some View {
        content.sheet(item:$controller.rename) { draft in PaperNameForm(controller:controller,draft:draft).modifier(EditorPopupPresentation()) }
            .overlay(alignment:.top) {
                if controller.busy {
                    HStack { ProgressView(); Button(controller.common["cancel"].string) { controller.cancelWork() } }
                        .padding(8).background(.regularMaterial)
                        .accessibilityElement(children:.contain).accessibilityIdentifier("paper-library-progress")
                }
            }
    }
}

private struct PaperNameForm: View {
    @Environment(\.capyNativeCopy) private var copy
    @ObservedObject var controller: PaperController
    let draft: PaperRename
    @State private var name: String
    @State private var error: String?
    @State private var saving = false
    init(controller:PaperController,draft:PaperRename) {
        self.controller = controller; self.draft = draft; _name = State(initialValue:draft.action["name"].string)
    }
    var body: some View {
        VStack(alignment:.leading,spacing:12) {
            Text(copy["paper"]["rename_title"].string).font(.headline)
            TextField(controller.common["name"].string,text:$name).textFieldStyle(.roundedBorder)
                .accessibilityIdentifier("paper-library-name").onSubmit(save)
            if let error { Text(error).foregroundStyle(.red).accessibilityIdentifier("paper-name-error") }
            HStack {
                Spacer()
                Button(controller.common["cancel"].string,role:.cancel) { controller.rename = nil }.keyboardShortcut(.cancelAction)
                Button(controller.copy["rename"].string,action:save).disabled(saving).keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("paper-name-save")
            }
        }.padding(20).frame(minWidth:320)
            .accessibilityElement(children:.contain).accessibilityIdentifier("paper-name-dialog")
    }
    private func save() {
        guard !saving else { return }
        saving = true
        controller.renamed(name) { error = $0; saving = false }
    }
}

struct PaperPicker: View {
    @ObservedObject var store: EditorStore
    @ObservedObject var controller: PaperController
    let layer: UInt64
    let epoch: UInt64
    let control: JSON
    let select: (Int) -> Void
    @State private var open = false
    @State private var images: [Int:CGImage] = [:]
    private var tiles: [JSON] { control["paper"]["tiles"].array }
    private var selected: Int { Int(control["value"]["value"].uint) }
    private var key: String { "\(epoch):\(layer):\(controller.generation):\(control.stableKey)" }
    @ViewBuilder private func thumbnail(_ index:Int,_ side:CGFloat) -> some View {
        if let image = images[index] { Image(decorative:image,scale:1).resizable().frame(width:side,height:side) }
        else { SharedIcon(name:"layer-fill-symbolic").frame(width:side,height:side) }
    }
    var body: some View {
        Button { open.toggle() } label: {
            HStack { thumbnail(selected,28); Text(tiles.indices.contains(selected) ? tiles[selected]["label"].string : control["label"].string); Spacer(); SharedIcon(name:"chevron-down") }.contentShape(Rectangle())
        }.buttonStyle(EditorControlButtonStyle()).accessibilityLabel(control["label"].string)
            .accessibilityValue(tiles.indices.contains(selected) ? tiles[selected]["label"].string : "")
            .accessibilityIdentifier("property-" + control["key"].string)
            .editorPopover(isPresented:$open) {
                VStack(alignment:.leading,spacing:8) {
                    ScrollView {
                        LazyVGrid(columns:[GridItem(.adaptive(minimum:72),spacing:4)],spacing:4) {
                            ForEach(tiles.indices,id:\.self) { index in
                                Button { select(index); open = false } label: {
                                    VStack { thumbnail(index,64); Text(tiles[index]["label"].string).font(.caption).lineLimit(2) }
                                        .frame(width:72).frame(minHeight:88).contentShape(Rectangle())
                                }.buttonStyle(EditorControlButtonStyle(selected:index == selected))
                                    .accessibilityLabel(tiles[index]["label"].string)
                                    .accessibilityAddTraits(index == selected ? .isSelected : [])
                                    .accessibilityIdentifier("paper-tile-\(index)")
                                    .contextMenu {
                                        ForEach(tiles[index]["actions"].array.indices,id:\.self) { action in
                                            let item = tiles[index]["actions"][action]
                                            Button(item["label"].string) { open = false; controller.action(item) }
                                        }
                                    }
                            }
                        }
                    }.frame(maxHeight:360)
                    if !control["paper"]["import"].isNull {
                        Button(control["paper"]["import"]["label"].string) { open = false; controller.action(control["paper"]["import"]) }
                            .accessibilityIdentifier("paper-import")
                    }
                }.padding(8).frame(width:336)
                    .accessibilityElement(children:.contain).accessibilityIdentifier("paper-picker")
            }
            .task(id:key) {
                guard let native = store.native else { return }
                let previews: NativePaperPreviews? = await withCheckedContinuation { done in
                    native.paperPreviews(layer:layer,epoch:epoch) { done.resume(returning:$0) }
                }
                guard let previews else { images = [:]; return }
                let decoded = await Task.detached(priority:.utility) { previews.decode() }.value
                guard !Task.isCancelled else { return }
                images = decoded.images
            }
    }
}
