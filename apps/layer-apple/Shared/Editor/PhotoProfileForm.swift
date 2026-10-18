import SwiftUI

struct PhotoProfileForm: View {
    @Environment(\.capyProfileCopy) private var copy
    @Environment(\.capyInterfaceLanguage) private var language
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
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(copy["interpret_title"].string).font(.headline)
            Text(copy["interpret_help"].string)
            SourceProfilePicker(preferences: preferences, spaces: spaces, selection: $space, imported: $imported, busy: $readingProfile)
            Text(sourceCaption)
                .foregroundStyle(.secondary)
            if let error { Text(error).foregroundStyle(.red) }
            HStack {
                Button(copy["common"]["cancel"].string) { choose(nil) }.keyboardShortcut(.cancelAction)
                Spacer()
                Button(copy["use_profile"].string) { choose(space == "imported" ? imported["profile"] : JSON(["Builtin": space])) }.keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("photo-profile-use")
                    .disabled(readingProfile)
            }
        }.padding(24).frame(minWidth: 320, idealWidth: 420, maxWidth: 520).disabled(busy)
            .onChange(of: [language, interpretation["channels"].string, interpretation["depth"].string], initial: true) { _, _ in
                sourceCaption = NativeTextContext.caption(["type": "photo_source", "channels": interpretation["channels"].raw,
                    "depth": interpretation["depth"].raw], language: language)
            }
    }
}
