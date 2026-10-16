# Extensibility lessons for the frozen `.capy` package and vector items

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-16

Checked against the vector design at `726cb6cdc`, or the draft before it in which recipes were separate records keyed by item ID. The design this audit produced is in [vector layers §7.2–7.4](../vector-layers-research.md#72-layer-model).

Date: 2026-10-16. Scope: how long-lived document and asset formats stay extensible, and what that means for `capy.vector-object/1` and its binary item stream before the package format is frozen.

Claims carry a source URL. `[K]` marks general knowledge I did not re-verify for this report. `[RE]` marks reverse-engineered, unofficial sources. Capy text is quoted from `docs/reference/capy-package.md`, `docs/reference/authored-model.md`, `docs/history/vector-layers-research.md` §7.4 and `docs/history/object-content-extensions-research.md`, and from code in `crates/layer-core/src/package/manifest.rs`, `crates/layer-core/src/package/json.rs` and `crates/layer-core/src/authored/extensions.rs`.

## 0. Findings in one page

1. **Every format that lasted kept framing separate from meaning.** A reader can find where every unknown thing ends without knowing what it is: PNG chunks, IFF/RIFF chunks, Xara records, PSD tagged blocks, protobuf wire types and EBML elements all work this way. The two documented failures are both framing leaks. PSD's PSB format makes a block's length field 4 or 8 bytes depending on its key. Rive needs a per-file type table to skip properties, and its runtime source carries fixes for desync bugs.
2. **Must-understand is decided per item by its writer or by its name, never by the reader's guess.** Examples: PNG's ancillary bit, glTF's `extensionsRequired`, OOXML `MustUnderstand`/`Ignorable`, Xara's essential and atomic tag lists, and PDF's MH/BE ("must honour"/"best effort") dictionaries. Formats without this rule degrade silently: ODF falls back to defaults for unknown values, Avro substitutes enum defaults, and Lottie only warns.
3. **Unknown data has to be cheap to keep and must not reference anything that moves.** Two examples: EXIF MakerNotes break on rewrite because they hold absolute offsets, and protobuf proto3 dropped unknown fields at first, reversing that in 3.5. Capy already forbids IDs and offsets in payloads. The item stream needs the same rule for item IDs and slot or style indices.
4. **Preservation rules that nobody exercises rot.** OOXML removed `PreserveElements`/`PreserveAttributes` because no one implemented them. The PNG spec says ordinary image editors "usually discard all unrecognized information". RFC 8701/9170 ("GREASE") exist because extension points "rust shut". Every bit in the design must be used by V0 data and by fixtures.
5. **Result plus source works only if exactly one side is authoritative and staleness is detectable.** PDF lets `/AP` take precedence over source fields, and real tools still ship stale or aliased appearances. Inkscape treats the source (`original-d`) as authoritative and regenerates `d`. Illustrator plugin groups regenerate result art from edit art on every update. Office `AlternateContent` keeps both, and an older editor's output document carries only the branch it selected. Capy's plan makes the result authoritative and the recipe advisory. That is the right orientation, but the recipe needs a verification step at the point of use (§R6).
6. **Stable IDs must be allocated without reuse and must never be positions.** Two cautionary cases: SQLite ROWIDs reuse deleted values unless the high-water mark is stored, and Rive's index-based IDs force null placeholders for unknown objects. Other cautionary cases: IDML `Self` IDs change on round trip, and pasted SVG IDs clash.

The concrete rules are in **§R (Rules and checks for Capy)**. The most important problems in the current plan:

- **An unknown container kind is rejected as Invalid, not Preserved.** Groups use End markers whose pairing depends on knowing the kind. A V0 reader seeing a future container kind reads its children as siblings, finds an unbalanced End and reports **Invalid**. The fix is a container bit in every kind number (§R1).
- **Ancillary records that reference other ancillary records make the document Invalid.** A future writer that breaks this rule gets its whole document rejected, instead of only its own ancillary data being ignored (§R9).
- **The JSON item-keyed ancillary recipe record in the plan is weaker than an in-stream property.** Old readers cannot see item IDs in JSON, so they cannot drop the record when its item dies. They do not clone it on duplicate. They keep it after the item changes. Use an in-stream ignorable, not copy-safe property instead, plus a fingerprint the reader that knows the recipe checks (§R6).

---

## 1. PNG (W3C PNG Third Edition)

Source: https://www.w3.org/TR/png-3/ (§5.4, §13.5, §14.2, §14.3, §15.3).

- **Four property bits in the chunk name** (§5.4):
  - **Ancillary** (first letter). For an unknown critical chunk, a decoder "shall indicate to the user that the image contains information it cannot safely interpret". An unknown ancillary chunk "can safely ignore the chunk and proceed to display the image".
  - **Private** (second letter). The bit "has no functional significance" for decoders (§13.5).
  - **Reserved** (third letter). It is 0 in this version. §13.5 says "Decoders should not flag an error if the reserved bit is set to 1, however, as some future version of the PNG specification could define a meaning for this bit. It is sufficient to treat a chunk with this bit set in the same way as any other unknown chunk type." §15.3.3 repeats this: "A chunk type in which the reserved bit is set is treated as an unknown chunk type."
  - **Safe-to-copy** (fourth letter). It "is not of interest to pure decoders, but it is needed by PNG editors."
- **Editor rules** (§14.2):
  - A safe-to-copy chunk "may be copied to a modified PNG datastream whether or not the PNG editor recognizes the chunk type, and regardless of the extent of the datastream modifications."
  - An unsafe chunk "depends on the image data. If the program has made any changes to critical chunks, including addition, modification, deletion, or reordering of critical chunks, then unrecognized unsafe chunks shall not be copied."
  - "A PNG editor is always allowed to copy all unrecognized ancillary chunks if it has only added, deleted, modified, or reordered ancillary chunks. This implies that it is not permissible for ancillary chunks to depend on other ancillary chunks."
  - Editors "shall terminate on encountering an unrecognized critical chunk type… (Simply discarding the chunk is not good enough, because it might have unknown implications for the interpretation of other chunks.)" The safe-to-copy bit "will always be 0 for critical chunks".
  - **Ordering:** an unknown unsafe chunk must not move relative to critical chunks. An unknown safe chunk must not cross IDAT. Ancillary chunks are forbidden to have stricter ordering needs, so editors can follow these rules blindly (§14.3.2).
- **What actually happens:** the spec itself says "Ordinary image editors are not PNG editors because they usually discard all unrecognized information while reading in an image" (§14.2). The bits are well designed, but most software does not implement editing preservation.
- **Lessons for Capy:**
  - Put the classification in the number, tested numerically ("Decoders should test the properties of an unknown chunk type by numerically testing the specified bits", §13.5).
  - A reserved bit set means *unknown*, never *error*.
  - "Unsafe" is scoped to what the data depends on. PNG keeps the scope coarse (any critical change in the file).

## 2. glTF 2.0

Sources: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html (§3.12, §5.16, §5.17, §4), https://github.com/KhronosGroup/glTF/blob/main/extensions/README.md, https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_draco_mesh_compression/README.md.

- **Two lists at the root.** "All extensions used in a glTF asset MUST be listed in the top-level extensionsUsed array". "All glTF extensions required to load and/or render an asset MUST be listed in the top-level extensionsRequired array". "extensionsRequired is a subset of extensionsUsed" (§3.12).
- **Required is the writer's decision.** The registry README says an extension "is considered required if a typical glTF loader would fail to load the asset in the absence of support… any mesh compression extension must be listed as required unless an uncompressed fallback mesh is provided… PBR or other kinds of material extensions should not be listed in required, because the core glTF material can be considered a valid fallback".
- **Draco** spells out both sides. "If KHR_draco_mesh_compression is in extensionsRequired and the loader does not support the Draco extension, then the loader must fail loading". Otherwise the loader ignores the extension and uses the fallback. Fallbacks must be consistent with the compressed data: "generators should create uncompressed attributes and indices using data that has been decompressed from the Draco buffer, rather than the original source data." A derived fallback must be derived from the stored form, not from higher-precision memory.
- **Container-level unknowns:** "Client implementations MUST ignore chunks with unknown types" in GLB (§4).
- **`extras`** is "Application-specific data", with no rules for preservation or for references held inside it (§5.16). glTF's internal references are array indices ("indices are used for internal glTF references"). A tool that reorders arrays leaves indices stored in `extras` dangling `[K]`.
- **Evolution and lifecycle:** the registry lists stages from proposal to ratified. **Archived** extensions such as `KHR_materials_pbrSpecularGlossiness`, `KHR_techniques_webgl` and `KHR_xmp` "may be useful for reading older glTF files, but they are no longer recommended for creating new files". Vendor prefixes (`KHR_`, `EXT_`, vendor) avoid name collisions.
- **Optional is not preserved.** glTF Transform throws "Missing required extension" and only warns "Missing optional extension". It reads only registered extensions, and its writer warns "Some extensions were not registered for I/O, and will not be written". A rewrite therefore drops unknown optional data (https://github.com/donmccurdy/glTF-Transform/blob/3797aad585021e68cf5fbf3f2fba37783596ffac/packages/core/src/io/reader.ts#L556-L573 , https://github.com/donmccurdy/glTF-Transform/blob/3797aad585021e68cf5fbf3f2fba37783596ffac/packages/core/src/io/writer.ts#L70-L72). "Ignorable" says nothing about "kept on edit"; that must be specified separately, which PNG and Capy both do.

## 3. OOXML Markup Compatibility and Extensibility (ECMA-376 Part 3)

Source: ECMA-376 Part 3, 5th ed. (2015), https://ecma-international.org/wp-content/uploads/ECMA-376-3_5th_edition_december_2015.zip (§6–§9).

- **Ignorable:** elements and attributes in an ignorable namespace "can be disregarded by consuming applications as if they were not present in the input document" (§6). An ignored element is removed with "its attributes and contents" (§9.4).
- **ProcessContent** marks ignorable elements to be **unwrapped** (children processed) instead of dropped (§7.3, §9.2). This is how a new container whose children still matter can degrade gracefully.
- **MustUnderstand:** "Namespaces can be declared that must be understood by consuming applications in order to process the document" (§6). If one is missing, the processor signals a mismatch (§9.4).
- **AlternateContent/Choice/Fallback:** a Choice is selected when every namespace in its `Requires` is understood. Otherwise the Fallback is selected. The `AlternateContent` is replaced by the selected branch's content (§9.3–9.4). Unknown ignorable children are allowed inside AlternateContent "to allow for future extensions to this construct" (§7.5 note).
- **Application-defined extension elements** suspend MCE processing; "Consuming applications might preserve application-defined extension elements even if they do not process them" (§6, §8). Office's `extLst`/`ext uri` uses this for Excel 2010+ features `[K]`.
- **PreserveElements and PreserveAttributes were removed.** The 2008 edition had them, but the 2015 text has neither. The SC34 WG4 convenor's revision note reads: "Japan believes that PreserveElements and PreserveAttributes are underspecified and cannot achieve interoperability… They are explicitly disallowed in Parts 1 and 4… there are no implementations" (https://mailman.vse.cz/pipermail/sc34wg4/2010-October/001812.html). MCE therefore has *no* preservation guarantee. The output of the reference processing model contains only the selected branch, so an older editor that saves its processed document loses the newer Choice (inference from §9.4).
- **How Office uses Fallback:**
  - Word 2010 shapes are a `Choice` holding `wps` DrawingML, with a VML `pict` Fallback for Word 2007 (https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/1ba79d37-c640-41dc-ba4e-9180ed31651e).
  - PowerPoint ink is a `Choice` holding `contentPart` (namespaces p14 and inkAction), with a `pic` picture as Fallback (https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/0a7df81a-b34d-4846-8e1f-db55b34f696b).
  - The fallback is a pre-rendered or legacy-vocabulary result. It is exactly Capy's "result plus recipe", with the roles reversed: the new form is authoritative and the fallback is a cache that older apps edit at their peril.
- **Lessons for Capy:**
  - MCE has three distinct reader behaviours for an unknown wrapper: drop the subtree (Ignorable), unwrap it (ProcessContent), or fail (MustUnderstand). Capy's item stream needs the same distinction for unknown container kinds.
  - Preservation hints that nobody tests become dead text.

## 4. PDF and ODF

**PDF.** Source: ISO 32000-1:2008, https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf.

- **Unknown keys:**
  - "Earlier versions of conforming readers do not notice the existence of such entries and behave as if they were not there."
  - "If a PDF file undergoes editing by an earlier version of a conforming product that does not understand some of the features that the file uses, the occurrences of those features may or may not survive" (Annex I.3).
  - PDF gives no preservation rule at all.
- **Features that cannot be ignored:** "In some cases, a new feature is impossible to ignore… There is little choice but to give an error" (Annex I.3).
- **Must honour vs best effort, per entry:**
  - In multimedia dictionaries, an unknown entry in the MH dictionary makes the object non-viable. An unknown entry in the BE dictionary "shall be ignored and viability shall be unaffected".
  - The point is to say "how entries that are added in the future… shall be interpreted by old conforming readers" (§13.2.2 Viability).
  - This is a per-property critical/ancillary split chosen by the writer.
- **`BX`/`EX`** bracket "a compatibility section… within which unrecognized operators shall be ignored without error" (§7.8.2, Table 32).
- **Developer extensions:** the catalog `/Extensions` dictionary maps registered prefixes to `BaseVersion` and an `ExtensionLevel` that "shall be a monotonically increasing sequence over time" (§7.12).
- **Optional content:** "A non-conforming reader that does not support optional content… will draw and process all content" (§8.11.3.1 Note 5). The extension was designed so the old behaviour is a sensible degradation.
- **Appearance streams are stored results** (§12.5.5):
  - For free text, "The annotation dictionary's AP entry, if present, shall take precedence over the DA entry" (§12.5.6.6), so the result wins.
  - Widget `NeedAppearances` asks the viewer to rebuild results (§12.7.2). PDF 2.0 deprecates it and requires appearance streams; see https://www.websupergoo.com/helppdfnet/source/5-abcpdf/xform/2-properties/needappearances.htm and https://www.pdf-tools.com/pdf-knowledge/annotations-interactive-form-fields/ ("the appearance depends on the implementation unless the appearance stream is present").
  - Poppler notes the standard is unclear when `NeedAppearances` is true but appearances exist (https://gitlab.freedesktop.org/poppler/poppler/-/merge_requests/1440).
- **Stale and aliased results in practice:**
  - Tools that change `/V` without regenerating `/AP` leave fields blank or stale in viewers that paint only `/AP` (https://www.useanvil.com/ask-anvil/pdfs/filled-pdf-fields-blank-in-adobe-reader/ ; https://github.com/endigo/flutter_pdfview/pull/354).
  - PDFBox bug PDFBOX-4471, "Different fields look alike if they have the same /AP/N before value change": two fields shared one appearance object, so editing one changed both (https://issues.apache.org/jira/browse/PDFBOX-4471). This is the deduplication-by-sharing hazard.

**ODF.** Source: OpenDocument 1.3 Part 3 §3.17, https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html.

- **Foreign elements:** "conforming consumers should not interpret but may preserve the element's content". This is a weak, optional preservation rule.
- **Unknown values are replaced by defaults:** "When a conforming consumer encounters an OpenDocument defined attribute that has a value that is not defined by OpenDocument, it should: 1) If the attribute has a specified default value, use its default value, or 2) … ignore the attribute." This silent substitution is what Capy's rules forbid, and Capy is right to.

## 5. Tagged binary formats: Xara, TIFF/EXIF, PSD, IFF/RIFF, OpenRaster, Krita

**Xara (XAR).** Source: http://web.archive.org/web/20251001015712/http://site.xara.com/support/docs/webformat/spec/XARFormatDocument.pdf.

- **Records:** every record has "a 32-bit 'Tag' field and a 32-bit size field… The Size field also allows the record to be skipped if the reader doesn't understand the Tag."
- **Tree structure is explicit and kind-independent.** `TAG_DOWN`/`TAG_UP` navigation records mark children: "The tree structure is essential to the correct interpretation of the image and so cannot be ignored."
- **Atomic records (`TAG_ATOMICTAGS`):**
  - The writer lists them per file. The list "prevents a Reader trying to understand the children of a record it doesn't recognise… If it finds a match the record and all its child records should be ignored."
  - Examples are "Text Stories, Moulds, and Blends", which are exactly the live groups Capy plans.
  - "Note: This implies that no references should be made to tags within atomic record subtrees if the referencing record is deemed essential."
- **Essential records (`TAG_ESSENTIALTAGS`):** for an unknown tag in this list, "the Reading process should be aborted and the user should be informed."
- **Tag descriptions (`TAG_TAGDESCRIPTION`)** give human-readable names of new tags, so an old reader can tell the user what it cannot show.
- **Unknown records that are neither atomic nor essential** are skipped, and their children are processed. This is MCE's ProcessContent behaviour.

**TIFF/EXIF MakerNotes.** Sources: https://exiftool.org/idiosyncracies.html , https://dev.exiv2.org/issues/543 .

- **Absolute offsets:** values longer than 4 bytes are referenced by "an offset from an absolute position in the file". In an unknown maker note these "must be recalculated when a file is rewritten, but in general this is not possible".
- **Per-vendor damage:** Leica S2 offsets break "if other software simply adds a leading JFIF segment", and Sony ARW maker notes "are not self-contained, so some information is lost".
- **Microsoft's OffsetSchema (0xea1d) workaround** records where WIC moved the maker note. It is unreliable: one sample had it set to 0, and "Phil recommends to ignore the tag" (exiv2 #543).
- **Lesson:** opaque data must never contain positions, offsets or identities that a rewriter changes.

**PSD/PSB.** Source: https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/ .

- **Tagged blocks:** additional layer information is a sequence of tagged blocks ("Signature: '8BIM' or '8B64'", 4-char key, length, data). This is skippable TLV.
- **Except in PSB:** "the following keys have a length count of 8 bytes: LMsk, Lr16, Lr32, Layr, Mt16, Mt32, Mtrn, Alph, FMsk, lnk2, FEid, FXid, PxSD". The width of the length depends on knowing the key, so a reader cannot generally skip an unknown key in PSB.
- **Rendered fallback:** "If maximize compatibility is unchecked then the merged/composite is not created and the layer data must be read to reproduce the final image." The file also has a `hasRealMergedData` flag.
- **Regenerating stored results changes them.** Photoshop shows rendered text pixels until the user accepts "Some text layers might need to be updated…", and users report that updating changes fonts (https://community.adobe.com/t5/photoshop-ecosystem-discussions/updating-all-text-layers/m-p/11811579 ; https://community.adobe.com/questions-438/when-updating-text-layers-the-font-changes-photoshop-23-2-2-on-mac-os-12-2-1-738110). Regenerating a stored result in a newer engine changes the artwork.

**IFF 85 / RIFF.** Source: https://www.martinreddy.net/gfx/2d/IFF.txt .

- **Skip and rename:** a reader "should skip over all unrecognized chunks. The ckID also serves as a format version number as long as we pick new IDs to identify new formats of ckData". This is Capy's `/2`-is-a-new-type rule.
- **Do not edit what you cannot create:** "Do not try to edit a file that you don't know how to create… don't copy structural parts you don't understand." This is Capy's Preserved outcome.
- **No copy-safety concept:** "we don't need to make programs preserve information that they don't understand". RIFF's 32-bit sizes later forced RF64 `[K]`.

**OpenRaster.** Sources: https://www.openraster.org/baseline/file-layout-spec.html , https://www.openraster.org/baseline/layer-stack-spec.html .

- **Mandatory composite:** `mergedimage.png` is required ("Changed in version 0.0.2… it is now mandatory") so viewers can show the final image.
- **Underspecified extensions:** the stack spec only says extensions "should be individually specified". It states no rule for unknown elements, attributes or values. Underspecification means each app behaves differently.

**Krita .kra.** Source: https://docs.krita.org/en/general_concepts/file_formats/file_kra.html .

- **Composite for other apps:** it contains `mergedimage.png`, and "Other applications mostly cannot open .kra files… Some applications, like Scribus, can use the mergedimage.png file."

## 6. Schema-evolution systems

**Protocol Buffers.**

- **Wire types make unknown fields skippable.** A tag is "(field_number << 3) | wire_type"; wire types "allow old parsers to skip over new fields". Groups (wire types 3 and 4) are deprecated (https://protobuf.dev/programming-guides/encoding/).
- **Unknown-field preservation was reversed:** proto3 at first dropped unknown fields. 3.4 added opt-in retention, and 3.5.0 made preservation the default (https://fuchsia.googlesource.com/third_party/protobuf/+/v3.5.2/CHANGES.txt). The current guide says "Proto3 messages preserve unknown fields", but serializing to JSON loses them (https://protobuf.dev/programming-guides/proto3/).
- **Never reuse field numbers:** "Reusing a field number makes decoding wire-format messages ambiguous… Field numbers should never be reused… Never take a field number out of the reserved list" (proto3 guide).
- **Unknown enum values** survive re-serialization (proto3 guide).
- **Serialization is not canonical:** "Protobuf serialization is not (and cannot be) canonical… hashes of serialized protos are fragile… we recommend writing out the fingerprinting function yourself naming all fields" (https://protobuf.dev/programming-guides/serialization-not-canonical/). This bears directly on recipe fingerprints.

**FlatBuffers.** Source: https://flatbuffers.dev/evolution/ .

- **Adding fields:** "New fields MUST be added to the end of the table definition", or use explicit `id` attributes.
- **Removing fields:** "You MUST not remove a field… mark the field deprecated".
- **Defaults are frozen:** changing defaults is "NOT OK" because absent values rely on generated code. This matches Capy's "frozen wire defaults".
- **Unions:** members may be added only at the end, or with explicit discriminants.

**Cap'n Proto.** Sources: https://capnproto.org/language.html , https://capnproto.org/encoding.html .

- **Fields:** new fields need numbers "larger than all previous members". You cannot change a field's number, type or default.
- **Unions:** "Adding new fields to an existing union is safe, since existing programs should already check the union's tag."
- **Copying without the schema:** struct pointers carry the data-section and pointer-section sizes. "This split allows structs to be traversed (e.g., copied) without knowing their type". References are discoverable without the schema, and this is the property Capy needs for references inside unknown properties.

**CBOR (RFC 8949).** Source: https://www.rfc-editor.org/rfc/rfc8949.txt .

- **Unknown tags are not errors:** a decoder that rejects unknown tags or simple values is allowed, but "treating this case as an error can cause ossification and is thus not encouraged. This error is not a validity error, per se" (§5.4). This is the Capy distinction between Unsupported and Invalid.
- **Unknown framing is fatal:** "An implementation receiving an unknown additional information value has no way to continue decoding" (§7.1). Framing codes cannot be extended later; only tags can.
- **Number ranges:** tags are allocated by range and cost. 0–23 need Standards Action, 24–32767 Specification Required, and above that First Come First Served (§9.2).

**Apache Avro.** Source: https://avro.apache.org/docs/1.11.1/specification/ .

- **Not self-describing:** "Binary encoded Avro data does not include type information or field names", so "Files or systems that store Avro data should always include the writer's schema".
- **Unknown data is dropped or replaced:**
  - A writer field the reader lacks is ignored, so it is dropped on rewrite.
  - An unknown enum symbol uses the reader's default if one exists. That is silent substitution.

**tldraw.**

- **Migrations:** snapshots record per-sequence schema versions, and records migrate up on load.
- **Newer documents do not load:** a persisted version the reader does not know returns `Result.err('Incompatible schema?')`, so a newer document cannot be loaded at all (https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/store/src/lib/StoreSchema.ts#L483-L508 ; failure reasons include `TargetVersionTooNew` and `UnknownType`: https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/store/src/lib/migrate.ts#L534-L541).
- **Why it does not transfer:** migration-first designs suit a hosted app whose readers update together. They do not suit files read by old installed apps. Capy's no-migration rule is the right stance, but it means every extension must be additive.

**Figma kiwi.**

- **Messages evolve with an embedded schema:** kiwi messages have optional, numbered fields. Forward compatibility works when "a copy of the new schema is bundled with the data", which "lets the decoder skip over unknown fields" (https://github.com/evanw/kiwi).
- **The `.fig` file embeds its schema** `[RE]`: "Each file contains its own schema definition in chunk 1" (https://grida.co/docs/wg/feat-fig/).
- **Lesson:** an embedded schema makes structure self-describing, but not meaning.

**Rive runtime format.** Sources: https://rive.app/docs/runtimes/advanced-topic/format ; https://github.com/rive-app/rive-runtime/blob/8f06a91c7dad29a8cb1345535c0855de3ace4411/src/file.cpp .

- **Header and type table:** "RIVE", then major and minor varuints, a file ID and a **Table of Contents**. The ToC lists every property key used, then a bit array with **2 bits per key** naming its backing type: 0 Uint/Bool, 1 String, 2 Float, 3 Color.
- **Objects:** a varuint type key, then varuint property key/value pairs, with key 0 as terminator.
- **Unknown properties:** the reader checks the ToC for an unknown property's backing type and reads past the value. Unknown objects become no-ops. Runtimes support exactly one major version.
- **The source records what goes wrong when a value has no explicit length:**
  - A key "that neither the runtime nor the file's ToC can type leaves the stream unreadable: nothing after it parses as intended, so malformed is set and the caller fails the import".
  - Two fixes are noted: "Uint64 shares the uint type id; skip the full range so an unknown 64 bit value never aborts the read", and a bool that was "Skipping nothing here read the value byte as the next key and desynchronized everything after it" (file.cpp L245–L310).
- **Positional IDs need placeholders:** "Ids are indices into the artboard's object list… Claim this object's index before import() can fail, so a failed object still holds its place" (file.cpp ~L563). Unknown objects therefore insert null placeholders (`ArtboardImporter::readNullObject` → `addComponent(nullptr)`, https://github.com/rive-app/rive-runtime/blob/8f06a91c7dad29a8cb1345535c0855de3ace4411/src/importers/artboard_importer.cpp#L49-L53).
- **Scope:** this is read-only forward compatibility for a runtime, not preservation through editing.

**Lottie.** Source: https://lottie.github.io/lottie-spec/latest/ (and `/specs/composition/`).

- **Version is advisory:** `ver` is an `MMmmpp` integer. Players "SHOULD issue a warning if" the major version is unsupported or the minor version is newer.
- **Free-form extension:** "Implementation MAY store additional data in the JSON objects."
- **Result:** there is no required-features list, so an unsupported feature is silently not rendered.

**Systems that separate "can read" from "can write".**

- **SQLite:** "If… the read version is 1 or 2 but the write version is greater than 2, then the database file must be treated as read-only. If a database file with a read version greater than 2 is encountered, then that database cannot be read or written" (https://www.sqlite.org/fileformat2.html §1.3.3).
- **ext4** has three feature masks (https://www.kernel.org/doc/html/latest/filesystems/ext4/super.html):
  - compat: the kernel "can still read/write this fs even if it doesn't understand a flag; fsck should not do that";
  - ro_compat: it "can still mount read-only";
  - incompat: it "should stop".
  - The structural rewriter (fsck) is stricter than the reader. That is PNG's unsafe-to-copy idea.
- **EBML (RFC 8794 §11.2.8):** `DocTypeReadVersion` is "The minimum DocType version an EBML Reader has to support to read this EBML Document" (https://www.rfc-editor.org/rfc/rfc8794.txt).

**Ossification (GREASE).** RFC 8701 reserves values that implementations send at random because "bugs may cause an implementation to reject unknown values… the metaphorical joint in the protocol has rusted shut" (https://www.rfc-editor.org/rfc/rfc8701.txt). RFC 9170: "an implementation that is regularly exposed to unknown values is less likely to be intolerant of new values" (https://www.rfc-editor.org/rfc/rfc9170.txt).

## 7. Stable identity and references across edits

- **Figma:**
  - IDs embed the client ID, so clients "generate unique IDs without asking the server".
  - The document is "Map<ObjectID, Map<Property, Value>>".
  - The parent link is a property on the child, stored atomically with a fractional-index position.
  - Deleted objects' properties live in the deleting client's undo buffer (https://www.figma.com/blog/how-figmas-multiplayer-technology-works/).
  - Instance overrides `[RE]` are `symbolOverrides`, each with a `guidPath`, "An ordered path of overrideKey GUIDs from the instance root to the target node", which "supports arbitrary nesting depth". `overrideKey` is distinct from the node `guid`. "Usage-site overrides WIN". `derivedSymbolData` caches derived results indexed by `guidPath` (https://canary.grida.co/docs/fr/wg/feat-fig/glossary/fig.kiwi).
  - Nested instance IDs appear as `I<id>;<id>` path strings in the API, and some REST endpoints do not resolve them (https://forum.figma.com/t/cant-get-node-with-prefix-i/58062).
- **Sketch:** an override's path "is formed by the symbolId of the nested symbols separated by a /", and its ID is `${path}_${property}` (https://developer.sketch.com/reference/api/symbol-override.txt). A community symbol-replacer plugin matches by name and warns of conflicts (https://github.com/uxdiogenes/symbol-replacer/blob/master/README.md). Name matching is the fragile fallback when IDs are not stable.
- **InDesign IDML:** `Self` IDs link objects, for example a frame's `ParentStory`. An Adobe community expert reports that after exporting to IDML and reopening, "the IDs for the page items are not the same anymore, they are reordered" (https://community.adobe.com/t5/indesign/reference-between-indd-and-idml/td-p/4530080). IDs that are not preserved through a format round trip cannot anchor external references.
- **Illustrator** added `PageItem.uuid` and `getPageItemFromUuid()` only in CC 2020 (24.0) (https://ai-scripting.docsforadobe.dev/introduction/changelog).
- **SVG and Inkscape:**
  - Pasting or importing objects whose gradient IDs clashed with the host document silently re-bound them to the host's gradient (https://bugs.launchpad.net/bugs/168729).
  - Inkscape 0.47 added `id-clash.cpp`, which renames clashing IDs and rewrites their users, but skips renaming when the existing gradient is "equivalent", so it merges by equivalence (https://bazaar.launchpad.net/~inkscape.dev/inkscape/trunk/view/15731/src/id-clash.cpp).
- **SQLite ROWID reuse:** without AUTOINCREMENT the new ROWID is "one larger than the largest ROWID in the table… ROWIDs from previously deleted rows might be reused". AUTOINCREMENT stores the largest ever used in `sqlite_sequence` "to prevent the reuse of ROWIDs from previously deleted rows" (https://www.sqlite.org/autoinc.html). A max+1 allocator recomputed on load is the standard way IDs get reused.
- **Rive** uses positional IDs, with the placeholder workaround in §6.

## 8. Result plus source patterns, and how they go stale

| System | Authoritative | Derived | When the derived side is rebuilt | Failure seen |
|---|---|---|---|---|
| Inkscape LPE | `inkscape:original-d` + effect parameters | `d` | On every edit of the source | A user reports: "when it is present, then it is no longer possible to edit the d attribute". The developer's reply: "The original-d is the path as drawn in the node editor and then the d is the path after it's gone through the live path effect" (https://lists.inkscape.org/hyperkitty/list/inkscape-user@lists.inkscape.org/message/D6622GAETM3CMI3EJF5FABCBEP66ROTZ/). A foreign edit to `d` is overwritten by the next rebuild (inference from that design) |
| Illustrator plugin groups | Edit group plus private data | Result group | On every update notification: it "is sent whenever any of the edit-group objects are modified… respond by regenerating the result-group objects". Edit art is "hidden, but… editable"; result art is "visible, but… not selectable". Live Blend keeps its two source objects and spine as edit art and the intermediates as result art (https://github.com/WestonThayer/Bloks/blob/master/BloksAIPlugin/Vendor/illustratorapi/illustrator/AIPluginGroup.h). Recolour and style changes reach private data through `adjustColorsCallback` and `adjustArtStyleCallback` | Only consistent while the plug-in is loaded |
| PDF annotations | `/AP` for display; `/V`/`/DA` as source | `/AP` | Writer regenerates, or viewer with `NeedAppearances` | Stale or blank fields, and shared `/AP` aliasing (§4) |
| OOXML AlternateContent | Choice (newer) | Fallback | Written by the newer app at save | An older editor's output keeps only the Fallback (§3) |
| PSD text layers | Text engine data | Layer pixels | When the user accepts "update text layers" | Updating changes fonts (§5) |
| glTF Draco | Draco buffer | Uncompressed accessors | At export, from decompressed data | Inconsistency if derived from source instead of stored data |

- **Two coherent designs:**
  - *Source authoritative, result cache* (Inkscape, Illustrator, PSD text). It needs every editor to understand the source, or foreign result edits are lost.
  - *Result authoritative, source advisory* (PDF `/AP` precedence, Capy's plan). It needs a staleness check before the source is used again.
- **Incoherent cases** have no defined winner. PDF's `NeedAppearances` with existing appearances is one.

---

# R. Rules and checks for Capy

These rules target `capy.vector-object/1`, `capy.vector-items/1` and the record level. "Unsupported" means the document opens Preserved. "Invalid" means malformed.

### R0. One principle

**For unknown data, Invalid must be decidable from framing alone.** Lengths, nesting, canonical integers, ordering and reference classes must all be checkable without knowing any kind or tag. Semantic validation applies only to understood data. Any rule that needs the meaning of an unknown kind or tag to decide Invalid will someday reject a well-formed future file. This is the CBOR §5.4/§7.1 lesson, and it matches the package's own rule that Invalid is "data that can never become valid".

### R1. Framing (freeze now; it can never change without a new resource type)

- **Item:** `K:varuint` then, unless `K == 0`, `id:u64le` and `len:varuint` followed by `len` bytes of properties.
  - `K == 0` is the **End** marker and has no ID or length. Kinds 1–15 (number 0 with any flag) are reserved and treated as Unsupported.
- **Kind word `K` bits:**
  - bit0 **I** ignorable;
  - bit1 **C** copy-safe;
  - bit2 **P** parent: the item is followed by its children and one End marker;
  - bit3 **R** reserved, 0;
  - `K >> 4` is the kind number.
  - **P makes subtree extent kind-independent.** Today's design ("Group… followed by its children and an End marker") ties pairing to knowing the kind. A V0 reader that meets a future container kind would see an unbalanced End and report **Invalid**. Xara solves this with explicit DOWN/UP records; the P bit keeps the End-marker layout and chunk locality.
- **Property:** `T:varuint`, `len:varuint`, then `len` bytes.
  - `T` bits: bit0 **I**, bit1 **C**, bits2–3 **V** (value class, R3), bit4 **R** reserved; `T >> 5` is the tag number (≥ 1).
  - **Every** value is length-prefixed, including fixed-size ones. One byte per small property buys unconditional skipping and avoids Rive's ToC desync class and PSB's key-dependent lengths.
- **Use varuint, not `u16`, for kind and tag words.** With five flag bits a `u16` leaves 2,048 tag numbers for decades, and the flags would sit in the high bits. Low-bit flags in a varuint follow protobuf's tag layout and have no ceiling.
- **Canonical encoding** (needed for byte-stable chunks, deduplication and fingerprints):
  - minimal varuints;
  - properties strictly ascending by full `T` within an item and within nested lists;
  - no repeated `T`;
  - a known value consumes exactly `len`;
  - F32/F64 bit patterns preserved, finite only.
  - A violation is **Invalid**, just as non-canonical decimal U64s and repeated JSON keys are today.
- **Values are never extended by appending fields.** A known tag with trailing bytes is Invalid. New fields are new tags, which is protobuf, FlatBuffers and Cap'n Proto practice and Capy's "new meaning gets a new name".
- **Chunks:** an item never spans chunks, and chunk boundaries may fall inside a group. An empty chunk should be accepted rather than Invalid, because it is a writer policy, not a mathematical domain.

### R2. Number assignment

- **A checked-in registry is the source of truth.** It lists number, flags, value class, grammar, the kinds each tag applies to, and the release that introduced it.
- **The registry is append-only, with a CI test.** No entry may change or disappear. Retired numbers stay reserved forever (protobuf `reserved`; glTF "archived").
- **Flags are part of the identity.** Changing I, C, P or V means a new number.
- **Experimental range:** reserve one range, for example tag numbers ≥ 2²⁰, for development builds. Portable save refuses it. Readers treat it like any other unknown number by its bits; no special-casing, per RFC 9170.
- **Third-party record types** in JSON should use reverse-DNS type names (glTF vendor prefixes; PNG private bit). Reserve `capy.` in the package contract.

### R3. Value classes (decide now: they can't be added later)

| V | Class | Old reader's duty for unknown properties |
|---|---|---|
| 0 | `data` | Opaque bytes. Must contain no item IDs, slot or style indices, offsets or Capy IDs |
| 1 | `items` | `n × u64le` item IDs, each nonzero and resolving in this vector object. Dangling is **Invalid**, as with JSON refs |
| 2 | `slots` | `n × (list:varuint, index:varuint)` into the record's slot lists. An unknown list number is Unsupported; an out-of-range index is Invalid |
| 3 | `nested` | A property list with the same framing, sort and flag rules, recursively |

- **Why classes are needed:** without them an old reader cannot keep a referenced slot alive, renumber it on compaction, remap an item ID on paste, or notice that a referenced item was deleted. That is the MakerNote failure. Cap'n Proto separates pointers so data can be copied without the schema; Capy's JSON already finds `{"ref"}` recursively inside unknown records.
- **What the existing alternative costs:** the current design avoids classes by declaring that "ignorable data… may depend only on its own item and that item's subtree, never on slots or other items". That is coherent, but it permanently forces every future feature that touches another item or a resource to be critical. Its own exceptions list (connectors, text on a separate path, Image Trace) would make whole documents Preserved in older readers. Two bits avoid that.
- **Minimum if classes are rejected:** freeze the restriction explicitly, and include style-table indices in it.

### R4. Reader behaviour for unknown kinds and tags

| Bits | Render | Edit (identity-preserving) | Duplicate or paste | Referenced item or slot removed |
|---|---|---|---|---|
| I=0 (any C) | Unsupported: the document opens Preserved | — | — | — |
| R=1, or I=0 with C=1 | Treat as unknown critical: Preserved, **never Invalid** (PNG §13.5) | — | — | — |
| I=1, C=1 | Ignore | Keep the bytes with the item; remap class 1/2 references | Clone with the item; remap references into the copied set; drop if a reference would leave the layer | Drop the property, and never write a dangling reference |
| I=1, C=0 | Ignore | Keep only while the **dependency content** is unchanged; otherwise drop | Keep if the copy is untransformed and its references stay valid after remapping; otherwise drop | Drop |

- **Dependency content** is defined once and reused by the fingerprint in R6:
  - It covers the owning item, its subtree, and every item its own class 1 references name.
  - Each item contributes its kind and its **I=0 properties only**, in canonical order, recursively.
  - Item IDs are excluded; references are hashed as in R6.
  - A rename (an I=1 name tag) therefore does not invalidate a recipe; a move, restyle, reorder of children or edit of a referenced item does.
- **Unknown data follows identity:**
  - Operations that make new items (combine, boolean, outline stroke, split-off pieces) carry no unknown properties.
  - The piece that keeps the original ID keeps them, by the rules above.
- **Unknown parent kinds (P=1) are atomic** (Xara atomic; MCE Ignorable without ProcessContent):
  - Their children are never interpreted.
  - An ignorable unknown parent hides its whole subtree from rendering, and the subtree is kept or dropped as a unit by the parent's C bit.
  - New container behaviour whose children should still render (links, export slices, tags) must be an ignorable **property on Group**, not a new kind.
  - This avoids needing an "unwrap" bit.
- **Unknown ignorable leaf kinds** keep their position among siblings. An unsafe one depends on its parent's subtree, so a change to that subtree drops it.
- **A known ignorable tag whose value the reader does not accept** (a new enum choice, out of range) is handled as an unknown property with the same bits, not as Preserved. A known critical tag with an unaccepted value is Unsupported, as today.
- **No edit to a vector object keeps its chunks byte-identical.** A save without edits to the layer must reuse its resources.

### R5. Item identity

- **Keep random nonzero 64-bit IDs.**
  - Check them against every live ID in the vector object: definitions, unknown ignorable items, and IDs named by any class 1 value, including inert override paths.
  - Store them as fixed `u64le`; a random value is 9–10 bytes as a varint.
- **Don't switch to a counter recomputed from live items.** That is SQLite's ROWID reuse.
- **A stored high-water mark has its own trap.** It lives in the record, so Undo of a creation would restore the lower mark and hand the undone item's ID to the next new item, while redo branches, the clipboard and stale ancillary keys still name it. Random IDs need no state, so neither problem arises. If a counter is ever used, it must be a never-decreasing allocator outside undo, saved as the maximum.
- **IDs are never positions, indices, names or hashes**, and are never renumbered on save (Rive placeholders, IDML round trip, glTF indices in `extras`).
- **One ID space per vector object, including local definitions.** Duplicate and paste renew colliding IDs and rewrite every class 1 reference in the copied set, critical or not, in the same edit.
- **Layer duplication can keep item IDs,** because they are layer-local. Only the record ID changes, so overrides and recipes inside need no rewrite.

### R6. Results with recipes

- **Store the recipe as an in-stream I=1, C=0 property** on the owning Path or Group, as the extension research proposes. Do not use a JSON ancillary record keyed by item ID. The comparison:

  | | JSON ancillary record keyed by item ID | In-stream I=1, C=0 property |
  |---|---|---|
  | Old reader drops it when the item changes | No: it sees only portable refs, and keeps copy-safe records "regardless of the extent" of change, as PNG does | Yes (R4) |
  | Old reader drops it when the item is deleted | No: an item key is not a `{"ref"}` | Yes |
  | Survives duplicate in old readers | No: unknown records are not cloned | Yes, when the copy is untransformed |
  | Item-ID reuse can rebind it to another item | Yes, unless IDs are never reused | No: it travels with the item |
  | Manifest budget | JSON nodes per live item against 4,194,304 | Binary in chunks |

- **Add a fingerprint anyway.** The C=0 rule is enforced by readers that *don't* understand the recipe, which is where PNG's history says enforcement fails. Other enforcers include third-party scripts and buggy Capy versions. The unsafe failure is Inkscape's: a later edit of recipe parameters regenerates from a stale recipe and silently overwrites someone else's geometry edit. Only the reader that knows the recipe can verify it.
  - `fp = SHA-256("capy.recipe/1\0" ‖ canonical dependency content)` truncated to 16 bytes. `sha2` is already in `Cargo.lock`. The fingerprint protects against accidental change, not attack.
  - Canonical dependency content is R4's definition. Item references to items inside the owner's subtree are hashed as their pre-order index within it. References outside are hashed by the referenced item's dependency content. Slot references are hashed by the bound resource's portable ID; a foreign re-ID detaches the recipe, which is acceptable.
  - It excludes the recipe itself, every I=1 property, item IDs, chunking and the item's position among its siblings. Per protobuf, name exactly what is covered, and never hash serialized messages.
  - **Verify on load in the worker,** against the stored result. Never verify by regenerating and comparing, because algorithm changes would detach every live item.
  - **A mismatch detaches** the item: it becomes ordinary geometry, the recipe is dropped on the next save, and the geometry is untouched. It is never Invalid and never regenerated.
- **Never regenerate on open** (PSD text-layer lesson). Regenerate only when the user edits the recipe. Transforms and recolouring in a reader that knows the recipe must update the recipe and refresh the fingerprint, as Illustrator's adjust callbacks do.
- **The stored result must be generated from the stored recipe values** (glTF Draco), not from in-memory higher-precision state.
- **Fingerprint parameters are not a freeze item.** Only readers that know a recipe check it, so each recipe tag fixes its own fingerprint version. The drop rules in R4 are the freeze item.
- **Decide per feature with one test:** can the result be expressed in kinds that already shipped? If yes, store result plus an ignorable recipe; older versions render it and degrade to geometry when edited. If not, the feature is critical and those older versions open the document Preserved.

### R7. Instances and overrides

- **Overrides are critical data on the instance item.** Each is `(path: items class, tag, value)`.
  - The path is the chain of definition item IDs from the instance's definition down through nested instances to the target. Every element is an item in the same vector-object ID space, so class 1 validation and remapping apply.
  - The overriding value uses the target tag's own I/C/V bits. An unknown critical override tag means Preserved.
  - Never key an override by name, index or depth-first position (Sketch and Figma name-matching fallbacks; IDML IDs reordered).
- **Precedence:** the outermost instance's override wins (Figma "usage-site overrides WIN").
- **An override whose path no longer reaches a target is inert, not Invalid.**
  - That happens when the definition lost the item, or a nested instance was swapped to another definition.
  - A reader that understands instances drops inert overrides when it next edits that instance.
  - Rejecting it as Invalid would break the planned shared, document-wide definitions, which can be edited apart from their instances. Old readers would then reject future files as malformed.
  - Swapping a definition does not migrate overrides by name.
- **Shared definitions later** live in another record. Their override paths start in another ID namespace, so they need a new critical tag, not a reinterpretation of this one.

### R8. Styles

- **Prefer styles inline in items, interned in memory** (the extension research's choice) over an indexed style table.
  - An index renumbered by compaction is a hidden reference, the MakerNote class of failure.
  - Shared objects alias edits (PDFBOX-4471).
  - LZ4 recovers most of the size.
- **If a table is kept:**
  - indices are class 2 references;
  - deduplication compares full canonical bytes, unknown properties included;
  - an edit is copy-on-write, so an entry is "deduplicated only, not linked";
  - deriving a new style from an old one copies its unknown C=1 properties.

### R9. Record-level rules that conflict with these lessons

1. **Ancillary→ancillary references are Invalid.** See capy-package.md, "Ancillary records may reference artwork/resources, not other ancillary records", the fixture table row "Ancillary record refers to another ancillary record… Invalid", and `manifest.rs:310` "Reference to ancillary record".
   - PNG makes the same *design* rule ("not permissible for ancillary chunks to depend on other ancillary chunks"), but never makes the file invalid.
   - Here a future writer that breaks the rule gets its whole document rejected.
   - Make such a record **not retainable**: it is ignored, and dropped on edited save like a non-copy-safe one. That keeps the closure rule simple.
   - Keep Invalid only for *required* artwork referencing ancillary records, which really can never be valid.
2. **`copy_safe:true` without `ancillary:true` is Invalid** (`manifest.rs:86`). For unknown types, a future version could give this combination a meaning, for example a required record that may be cloned with its subject. Treat it as an unknown *required* record (Preserved).
3. **"Known artwork types reject either flag"** (`manifest.rs:86`; `RecordRole` has only `Object`/`Resource`).
   - When Capy ships its own ancillary types, such as notes or caches of third-party data, the registry needs a classification, and the rule becomes "a known type's flags must equal its registration".
   - Otherwise the reader that defines an ancillary type rejects its own records.
4. **Copy-safe ancillary records survive any change to what they reference.** `edited_retained` (`extensions.rs:119-134`) checks only that referenced IDs still exist. That matches PNG, but it means a copy-safe record may depend only on its referents' *identity*. Anything content-dependent must be in-stream with C=0 (R6), or carry its own fingerprint. Say so in the contract.
5. **The `ref` key is reserved globally.** A JSON object containing `ref` must have exactly that field, or it is Invalid (`json.rs:72`, `manifest.rs:37`). That permanently forecloses qualified references such as `{"ref":…,"item":…}`. If any JSON record will name items, freeze its spelling now as `{"object":{"ref":…},"item":"<u64>"}`, mirroring endpoint ports. Readers must treat an unresolvable item as stale, because old readers can't see it.
6. **The envelope is closed.** Unknown envelope fields make the manifest uninterpretable. That is fine only as a stated freeze rule: never add envelope fields; extend through records.
7. **Asymmetry to document:** unknown ancillary *records* are not cloned on duplicate, but unknown copy-safe in-stream *properties* are. That is safe because in-stream references are typed and remappable, while JSON ancillary data may reference anything.
8. **Corrections to the current vector design in vector-layers-research.md §7.4:**
   - **Kind width:** the `u16` kind and tag words have no container bit (R1).
   - **Unsafe properties are dropped on duplicate:** "Ignorable, not copy-safe… Drops it as soon as the reader changes either, or duplicates or pastes the item" loses recipes on every untransformed duplicate. Under R4 they survive.
   - **"Changes" is undefined.** Use the dependency content in R4, so a rename doesn't drop a recipe.
   - **Slot renumbering** ("Writers keep slot lists dense and renumber the items they know… ignorable data never uses slots") is safe only under the no-class alternative to R3. With class 2, renumbering must rewrite unknown slot references too.
9. **Preserved stays whole-document.** That is the MCE MustUnderstand and glTF required model. Per-layer fallbacks in Office's style would let old apps edit other layers but bring back stale-cache problems. I recommend no change. Expect a feature that ships critical to make every document using it read-only in older versions, and choose result plus recipe where the test in R6 allows.

### R10. Checks: fixed files and tests

All checks must be in place for the **first** release. V0 data must use all three classes: critical geometry; an ignorable copy-safe item name; an ignorable unsafe live-shape recipe. Otherwise the paths rust shut (RFC 8701/9170).

| Fixture | Expected |
|---|---|
| Unknown critical leaf kind; unknown critical **parent** kind with children and End | Preserved, not Invalid |
| Unknown ignorable C=1 parent kind containing known Paths | Renders without the subtree; editing a sibling keeps its bytes identical |
| Unknown ignorable unsafe leaf kind in a group | Kept until a child of that group changes, then dropped |
| Reserved bit set on a kind or tag; I=0 with C=1 | Preserved |
| Unknown I=1, C=1 tag of each class on a Path | Move, recolour or rename keeps it. Duplicate clones it with remapped class 1 IDs. Deleting the referenced item drops it. Slot compaction renumbers its class 2 references |
| Unknown I=1, C=0 tag | Rename keeps it; move drops it; untransformed duplicate keeps it on both copies; paste with offset drops it on the copy |
| Known ignorable tag with an unknown enum value | Editable; behaves as an unknown property with those bits |
| Non-minimal varuint; unsorted or repeated tags; trailing bytes in a known value; length overrun; dangling class 1 or out-of-range class 2 reference; unbalanced End | Invalid |
| Empty chunk; depth or count beyond admission | Accepted; or Unsupported (Preserved), never Invalid |
| Recipe whose result geometry was changed by a foreign writer that kept the recipe | Opens Editable as plain geometry; recipe gone after save; geometry bit-identical |
| Recipe fingerprint across re-chunking, a rename, duplicate with ID renewal, and save in another chunk layout | Unchanged |
| Override path dangling after its definition item is removed | Editable; inert; dropped when that instance is next edited |
| Ancillary record referencing another ancillary record (after R9.1) | Editable; record dropped on edited save, kept by Copy Original |
| No-edit save of a layer with unknown ignorable data | The layer's chunk resources are byte-identical |
| Seeded property test: create, delete, undo and redo 10⁵ items | No live, deleted or referenced ID is ever reissued |
| Registry test | Every assigned number, flag set and value class matches the frozen list; new entries append only |

Fuzz test: for each fixture, inject random unknown properties of every bit combination at every nesting level, apply random edit scripts, and assert three things: framing stays valid; copy-safe bytes survive identity-preserving edits; unsafe ones disappear when their dependency content changes.

### R11. Decide before the freeze (cannot be added later)

1. The kind and tag word layouts, including P, R and varuint encoding (R1).
2. Value classes, or the explicit "ignorable data references nothing" rule (R3).
3. Canonical encoding rules (R1).
4. The R4 drop, keep and clone table, including "dependency content".
5. Whether override paths are class 1 references, and that dangling overrides are inert (R7).
6. R9.1 (ancillary→ancillary), R9.2 (`copy_safe` without `ancillary`), R9.5 (the item-endpoint spelling, if any JSON names items).

Not freeze items: each recipe's fingerprint algorithm, recipe value grammars, and new kinds and tags. Those only need readers that understand them.

## Source index

- PNG 3rd ed.: https://www.w3.org/TR/png-3/
- glTF 2.0: https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html ; https://github.com/KhronosGroup/glTF/blob/main/extensions/README.md ; https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_draco_mesh_compression/README.md ; glTF Transform: https://github.com/donmccurdy/glTF-Transform/blob/3797aad585021e68cf5fbf3f2fba37783596ffac/packages/core/src/io/reader.ts
- ECMA-376 Part 3: https://ecma-international.org/wp-content/uploads/ECMA-376-3_5th_edition_december_2015.zip ; https://mailman.vse.cz/pipermail/sc34wg4/2010-October/001812.html ; https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/1ba79d37-c640-41dc-ba4e-9180ed31651e ; https://learn.microsoft.com/en-us/openspecs/office_standards/ms-pptx/0a7df81a-b34d-4846-8e1f-db55b34f696b
- PDF: https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf ; https://www.websupergoo.com/helppdfnet/source/5-abcpdf/xform/2-properties/needappearances.htm ; https://www.pdf-tools.com/pdf-knowledge/annotations-interactive-form-fields/ ; https://gitlab.freedesktop.org/poppler/poppler/-/merge_requests/1440 ; https://www.useanvil.com/ask-anvil/pdfs/filled-pdf-fields-blank-in-adobe-reader/ ; https://github.com/endigo/flutter_pdfview/pull/354 ; https://issues.apache.org/jira/browse/PDFBOX-4471
- ODF 1.3 Part 3: https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part3-schema/OpenDocument-v1.3-os-part3-schema.html
- Xara: http://web.archive.org/web/20251001015712/http://site.xara.com/support/docs/webformat/spec/XARFormatDocument.pdf
- EXIF: https://exiftool.org/idiosyncracies.html ; https://dev.exiv2.org/issues/543
- PSD: https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/ ; https://community.adobe.com/t5/photoshop-ecosystem-discussions/updating-all-text-layers/m-p/11811579 ; https://community.adobe.com/questions-438/when-updating-text-layers-the-font-changes-photoshop-23-2-2-on-mac-os-12-2-1-738110
- IFF: https://www.martinreddy.net/gfx/2d/IFF.txt ; OpenRaster: https://www.openraster.org/baseline/file-layout-spec.html , https://www.openraster.org/baseline/layer-stack-spec.html ; Krita: https://docs.krita.org/en/general_concepts/file_formats/file_kra.html
- Protobuf: https://protobuf.dev/programming-guides/encoding/ , https://protobuf.dev/programming-guides/proto3/ , https://protobuf.dev/programming-guides/serialization-not-canonical/ , https://fuchsia.googlesource.com/third_party/protobuf/+/v3.5.2/CHANGES.txt
- FlatBuffers: https://flatbuffers.dev/evolution/ ; Cap'n Proto: https://capnproto.org/language.html , https://capnproto.org/encoding.html ; CBOR: https://www.rfc-editor.org/rfc/rfc8949.txt ; Avro: https://avro.apache.org/docs/1.11.1/specification/
- tldraw: https://github.com/tldraw/tldraw/blob/dd116535b138520d560c9881ad095ab782dc74c7/packages/store/src/lib/StoreSchema.ts ; kiwi: https://github.com/evanw/kiwi ; .fig [RE]: https://grida.co/docs/wg/feat-fig/ , https://canary.grida.co/docs/fr/wg/feat-fig/glossary/fig.kiwi
- Rive: https://rive.app/docs/runtimes/advanced-topic/format ; https://github.com/rive-app/rive-runtime/blob/8f06a91c7dad29a8cb1345535c0855de3ace4411/src/file.cpp ; Lottie: https://lottie.github.io/lottie-spec/latest/
- SQLite: https://www.sqlite.org/fileformat2.html , https://www.sqlite.org/autoinc.html ; ext4: https://www.kernel.org/doc/html/latest/filesystems/ext4/super.html ; EBML: https://www.rfc-editor.org/rfc/rfc8794.txt ; GREASE: https://www.rfc-editor.org/rfc/rfc8701.txt , https://www.rfc-editor.org/rfc/rfc9170.txt
- Identity: https://www.figma.com/blog/how-figmas-multiplayer-technology-works/ ; https://forum.figma.com/t/cant-get-node-with-prefix-i/58062 ; https://developer.sketch.com/reference/api/symbol-override.txt ; https://github.com/uxdiogenes/symbol-replacer/blob/master/README.md ; https://community.adobe.com/t5/indesign/reference-between-indd-and-idml/td-p/4530080 ; https://ai-scripting.docsforadobe.dev/introduction/changelog ; https://bugs.launchpad.net/bugs/168729 ; https://bazaar.launchpad.net/~inkscape.dev/inkscape/trunk/view/15731/src/id-clash.cpp
- Result/source: https://lists.inkscape.org/hyperkitty/list/inkscape-user@lists.inkscape.org/message/D6622GAETM3CMI3EJF5FABCBEP66ROTZ/ ; Illustrator SDK `AIPluginGroup.h` (vendored copy): https://github.com/WestonThayer/Bloks/blob/master/BloksAIPlugin/Vendor/illustratorapi/illustrator/AIPluginGroup.h
