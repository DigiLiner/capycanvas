import SwiftUI

struct PhotoProfileForm: View {
    @Environment(\.capyProfileCopy) private var copy
    @Environment(\.capyInterfaceLanguage) private var language
    @Environment(\.editorDialogPage) private var page
    let preferences: ColorPreferencesStore
    let interpretation: JSON
    let spaces: [JSON]
    let error: String?
    let busy: Bool
    let choose: (JSON?) -> Void
    @State private var space = "Srgb"
    @State private var imported = JSON()
    @State private var readingProfile = false
    @State private var sourceCaption = ""
    private func useProfile() { choose(space == "imported" ? imported["profile"] : JSON(["Builtin": space])) }
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if page {
                EditorPageHeader(intent: copy["dialog"], title: copy["interpret_title"].string, identifier: "photo-profile",
                    primaryLabel: copy["use_profile"].string, primaryEnabled: !readingProfile, primaryIdentifier: "photo-profile-use",
                    cancel: { choose(nil) }, primary: useProfile)
            }
            if page { EditorScrollView { formBody } }
            else { formBody }
        }.modifier(EditorDialogFrame(minWidth: 320, idealWidth: 420, maxWidth: 520)).disabled(busy)
            .onChange(of: [language, interpretation["channels"].string, interpretation["depth"].string], initial: true) { _, _ in
                sourceCaption = NativeTextContext.caption(["type": "photo_source", "channels": interpretation["channels"].raw,
                    "depth": interpretation["depth"].raw], language: language)
            }
    }
    private var formBody: some View {
        VStack(alignment: .leading, spacing: 16) {
            if !page { Text(copy["interpret_title"].string).font(.headline) }
            Text(copy["interpret_help"].string)
            SourceProfilePicker(preferences: preferences, spaces: spaces, selection: $space, imported: $imported, busy: $readingProfile)
            Text(sourceCaption)
                .foregroundStyle(.secondary)
            if let error { Text(error).foregroundStyle(.red) }
            if !page { HStack {
                Button(copy["common"]["cancel"].string) { choose(nil) }.keyboardShortcut(.cancelAction)
                Spacer()
                Button(copy["use_profile"].string, action: useProfile).keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("photo-profile-use")
                    .disabled(readingProfile)
            } }
        }.padding(24)
    }
}
