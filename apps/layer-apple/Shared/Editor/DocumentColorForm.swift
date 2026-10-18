import SwiftUI
import UniformTypeIdentifiers
#if os(macOS)
import AppKit
#endif

struct DocumentColorForm: View {
    @ObservedObject var editor: DocumentColorController
    @Environment(\.capyInterfaceLanguage) private var language
    let spaces: [JSON]
    @State private var space = "Srgb"
    @State private var depth = "U16"
    @State private var intent = "RelativeColorimetric"
    @State private var dither = "None"
    @State private var copy = false
    @State private var imported = JSON()
    @State private var readingProfile = false
    @State private var filename = ""
    @State private var choosingFolder = false
    private var text: JSON { editor.text }
    private var exportCopy: JSON { editor.catalog["export_copy"] }
    private var delivery: JSON { editor.catalog["document_delivery_copy"] }
    private var help: String {
        let keys = ["rasterize_source": "rasterize_help", "repair_source_profile": "repair_help", "assign": "assign_help", "depth": "depth_help"]
        return text[keys[editor.operation] ?? "convert_help"].string
    }
    private var progress: String {
        if editor.publishing { return text["applying"].string }
        if !editor.loaded { return editor.bootstrap["preparing_document"].string }
        if editor.ready { return delivery["writing_converted_copy"].string }
        if editor.source { return text[editor.operation == "rasterize_source" ? "rasterize_comparison" : "preparing_comparison"].string }
        return editor.copy ? delivery["preparing_converted_copy"].string : text["preparing"].string
    }
    private var applyLabel: String {
        if editor.copy { return text["save_copy"].string }
        if editor.operation == "rasterize_source" { return text["rasterize"].string }
        if editor.sourceInfo["adds_layer"].bool { return text["add_source"].string }
        return editor.source ? text["apply_profile"].string : text["common"]["apply"].string
    }
    private var choice: JSON? {
        switch editor.operation {
        case "repair_source_profile": return space == "imported" ? imported["profile"] : JSON(["Builtin": space])
        case "rasterize_source": return nil
        case "assign": return JSON(["Assign": space])
        case "depth": return JSON(["Depth": ["depth": depth, "dither": depth == "U8" ? dither : "None"]])
        default: return JSON(["Convert": ["space": space, "options": ["intent": intent, "black_point_compensation": false]]])
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(editor.title).font(.headline)
            EditorScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if editor.properties {
                        ForEach(Array(editor.rows.enumerated()), id: \.offset) { _, row in
                            Text(row[0].string).font(.headline)
                            Text(row[1].string).textSelection(.enabled)
                        }
                        if !editor.propertiesView["sources"].array.isEmpty {
                            Text(editor.propertiesView["source_images"].string).font(.headline)
                            ForEach(Array(editor.propertiesView["sources"].array.enumerated()), id: \.offset) { _, row in
                                Text(row[0].string).font(.headline)
                                Text(row[1].string).textSelection(.enabled)
                            }
                        }
                    } else if !editor.history {
                        Text(help)
                        choices.disabled(editor.busy || !editor.loaded)
                        Button(text["preview"].string) { editor.prepare(choice, copy: copy) }
                            .disabled(editor.busy || !editor.loaded || readingProfile).accessibilityIdentifier("document-color-preview")
                    }
                    if editor.busy { ProgressView(progress) }
                    if editor.clipped > 0 { Text(text[editor.source ? "source_clipped" : "outside_gamut"].string) }
                    if editor.source {
                        Text(text["current_source"].string).font(.headline)
                        Text(editor.sourceInfo["source_profile"].string).textSelection(.enabled)
                        if editor.sourceInfo["adds_layer"].bool { Text(text["adds_layer"].string) }
                    }
                    ForEach(Array(editor.previews.enumerated()), id: \.offset) { index, image in
                        Text(text[index == 0 ? "before" : "after"].string).font(.headline)
                        Image(decorative: image, scale: 1).resizable().aspectRatio(contentMode: .fit)
                            .frame(maxWidth: .infinity, maxHeight: 180)
                            .accessibilityLabel(text[index == 0 ? "original_composition" : "prepared_composition"].string)
                    }
                    if let error = editor.error { Text(error).foregroundStyle(.red) }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack {
                Button(editor.properties ? editor.propertiesDone : text["common"]["cancel"].string) { editor.cancel() }
                    .keyboardShortcut(.cancelAction).disabled(editor.publishing)
                Spacer()
                if !editor.properties && !editor.history {
                    Button(applyLabel) {
                        if editor.copy { saveCopy() } else { editor.apply() }
                    }.keyboardShortcut(.defaultAction).disabled(!editor.ready || editor.busy || readingProfile)
                        .accessibilityIdentifier("document-color-apply")
                }
            }
        }.padding(24).frame(minWidth: 340, idealWidth: 520, maxWidth: 620, minHeight: 260, idealHeight: 640, maxHeight: 720)
            .onChange(of: language, initial: true) { _, _ in editor.refreshCopy() }
            .onChange(of: editor.color.stableKey, initial: true) { _, _ in
                if editor.loaded {
                    space = editor.source ? editor.sourceInfo["profile_builtin"].string : editor.color["space"].string
                    if space.isEmpty { space = "Srgb" }
                    depth = editor.color["depth"].string
                }
            }
            // Imported ICC payloads are large. Their explicit import callback
            // invalidates previews without serializing those bytes during layout.
            .onChange(of: editor.source ? space : (choice ?? JSON()).stableKey) { _, _ in editor.invalidate() }
            .onChange(of: copy) { _, next in
                editor.invalidate()
                if next && filename.isEmpty { editor.convertedName { if filename.isEmpty { filename = $0 } } }
            }
            #if os(iOS)
            .fileImporter(isPresented: $choosingFolder, allowedContentTypes: [.folder]) { result in
                switch result {
                case .success(let folder): editor.saveCopy(to: folder.appendingPathComponent(filename), access: folder)
                case .failure(let error):
                    let native = error as NSError
                    if native.domain != NSCocoaErrorDomain || native.code != NSUserCancelledError {
                        editor.error = error.localizedDescription
                    }
                }
            }
            #endif
    }
    @ViewBuilder private var choices: some View {
        if editor.operation == "repair_source_profile" {
            SourceProfilePicker(preferences: editor.preferences, spaces: spaces, selection: $space, imported: $imported, busy: $readingProfile, onImport: editor.invalidate)
        } else if editor.operation != "depth" && editor.operation != "rasterize_source" {
            FormPicker(text["space"].string, selection: $space) {
                ForEach(spaces, id: \.stableKey) { Text($0[1].string).tag($0[0].string) }
            }.accessibilityIdentifier("document-color-space")
        }
        if editor.operation == "depth" {
            FormPicker(text["depth"].string, selection: $depth) {
                Text(text["depth_8"].string).tag("U8"); Text(text["depth_16"].string).tag("U16")
                Text(text["depth_float16"].string).tag("F16"); Text(text["depth_float32"].string).tag("F32")
            }
                .accessibilityIdentifier("document-color-depth")
            if depth == "U8" {
                FormPicker(text["dither"].string, selection: $dither) {
                    Text(exportCopy["dither_none"].string).tag("None"); Text(text["dither_stochastic"].string).tag("Stochastic8")
                }
            }
        }
        if editor.operation == "convert" {
            FormPicker(text["result"].string, selection: $copy) {
                Text(text["editable_layers"].string).tag(false); Text(text["flattened_copy"].string).tag(true)
            }
            FormPicker(exportCopy["intent"].string, selection: $intent) {
                Text(exportCopy["relative"].string).tag("RelativeColorimetric"); Text(exportCopy["perceptual"].string).tag("Perceptual")
                Text(exportCopy["saturation"].string).tag("Saturation"); Text(exportCopy["absolute"].string).tag("AbsoluteColorimetric")
            }
            #if os(iOS)
            if copy { TextField(text["common"]["name"].string, text: $filename).textFieldStyle(.roundedBorder) }
            #endif
        }
    }
    private func saveCopy() {
        #if os(macOS)
        let panel = NSSavePanel(); panel.allowedContentTypes = [.capyProject]
        panel.nameFieldStringValue = filename; panel.canCreateDirectories = true
        panel.begin { response in if response == .OK, let url = panel.url { editor.saveCopy(to: url) } }
        #else
        filename = filename.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !filename.isEmpty, !filename.contains("/"), filename != ".", filename != ".." else {
            editor.error = text["choose_filename"].string; return
        }
        if !filename.lowercased().hasSuffix(".capy") { filename += ".capy" }
        choosingFolder = true
        #endif
    }
}
