# Text objects and font storage: what the Capy format must leave room for

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-16

Checked against the vector design at `726cb6cdc`, or the draft before it in which recipes were separate records keyed by item ID. The design this audit produced is in [vector layers §7.2–7.4](../vector-layers-research.md#72-layer-model).

Research date 2026-10-16. Scope: the text object model, how the main formats store text, font identity and embedding, the Rust text stack, and the decisions the vector format must make now so text can arrive later without a redesign.

Evidence marks: a URL after a claim is where I checked it. **[K]** is general knowledge I did not re-verify for this report. **[unverified]** means I looked and could not confirm it. **[M]** is my own measurement (method in §4.3 and §3.4).

---

## 0. Summary

1. **Store the laid-out result.** Figma stores glyph outlines inside every text node. Its reverse-engineered schema has `Glyph{commandsBlob, position, fontSize, firstCharacter, advance, rotation, emojiImageSet}` plus `fontDigest` and layout-version fields. PSD stores rendered pixels next to EngineData. PDF stores glyph runs. Office stores the computed autofit `fontScale`. Inkscape writes a positioned SVG 1.1 fallback. A stored result gives three things: identical rendering without the font, no reflow when the layout engine, ICU or fonts change, and a correct render of later typographic features by readers that cannot lay them out. The cost is measured in §3.4: about 1.3 KB raw (0.4 KB after LZ4) per CJK glyph as Capy-style F64 path items. That is 3 to 10 times the size of a font subset.
2. **The result's form is the central structural choice (§5.1).** If text results are ordinary Path items with an ignorable text recipe, readers that predate text can still edit the document. But under the current rule a recipe cannot use font slots, so it cannot embed fonts, and text on another path or wrap around other items cannot be ignorable either. I recommend a required **Text** kind that holds the story, the layout inputs and a compact glyph-run result inside the item, with no child items. Keep "outlines + text recipe" only for Convert to Outlines and for SVG import before text ships.
3. **Freeze the text unit and paragraph rules when the first text data ships.** That could be SVG import's text recipe, before Text itself. Use UTF-8 storage, with run lengths in UTF-8 bytes on scalar boundaries. Rust, parley's `StyleRunBuilder`, harfrust clusters and cosmic-text all use bytes. Never use grapheme units, because the segmentation rules change between Unicode versions. Do not normalize on save. LF is the paragraph separator and U+2028 is a forced line break. SVG 2 itself uses two units: code points for positioning attributes and UTF-16 for the DOM.
4. **Keep two range structures.** Coalescible *style runs* point into a style table, as in PSD StyleRun, Figma `styleOverrideTable` and parley `StyleRunBuilder`. Separate non-coalescing *spans* carry identity: ruby, tate-chu-yoko, warichu, links and inline objects. Two adjacent tate-chu-yoko units, or two ruby groups with the same reading, must not merge.
5. **Font identity is a descriptor, not just a name.** The probe in §3.3 shows why. The Noto Sans CJK variable font's default instance is *Thin* and its file-level PostScript name is `NotoSansCJKjp-Thin`, so "Regular" needs explicit axis values. Store family, style, PostScript name, version, axis values, face index and a content digest in the item. Use a record-JSON `fonts` slot only for embedded bytes.
6. **Embedding policy:** follow OS/2 `fsType` (2 never, 4 preview & print, 8 editable, bit 8 no subsetting, bit 9 bitmap only) and require explicit user opt-in, as Sketch does. On the Fedora 44 workstation used for this research, 35 of 251 system font files are fsType 4 and 18 are fsType 8 [M], so a policy that only embeds fsType 0 would exclude common free fonts.
7. **Rust stack.** harfrust shapes vertical text. parley 0.12 has no vertical writing mode, ruby, hyphenation, tab stops or span-level bidi overrides. skrifa paints COLRv0/v1 and reads sbix, CBDT and EBDT bitmaps, but not SVG-in-OT. Vertical layout, ruby, tate-chu-yoko, kinsoku and mojikumi are Capy's own work on top of harfrust and ICU4X. Measured wasm32 sizes: parley+skrifa is 1.23 MB raw (439 KB gzip), and complex-script segmentation raises it to 5.0 MB (2.9 MB gzip) [M].
8. **Before the v1 vector freeze**, resolve the shared rules text depends on. Readers must know whether an unknown kind opens a subtree. Decide whether slot entries may be descriptors. Decide whether a recipe with unsupported content degrades to an opaque ignorable property, and whether pre-text SVG import stores the original SVG fragment rather than a Capy text model. See the gaps section.

---

## 1. Feature inventory: what artists expect and where it lives

The **Lives on** column is the level each feature naturally attaches to. *Item* means the whole text object or story, *frame* is a container, *paragraph*, *run* is a coalescible character style, and *span* is a range with identity.

### 1.1 Containers and flow

| Feature | Evidence | Lives on |
|---|---|---|
| Point text: an anchor and alignment (start, middle or end), no wrapping | Illustrator `TextType` point and `anchor` "start of the base line for point text" (https://ai-scripting.docsforadobe.dev/jsobjref/TextFrameItem/); PSD `ShapeType` 0 with `PointBase` (https://github.com/Agamnentzar/ag-psd/blob/master/src/text.ts) | item |
| Area text: rectangle or arbitrary shape, columns and rows, gutter, insets, vertical alignment | Illustrator `columnCount`, `rowCount`, `spacing` (TextFrameItem above). Office `bodyPr` `numCol`, `spcCol`, `rtlCol`, `lIns`/`tIns`/`rIns`/`bIns`, `anchor` (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.bodyproperties?view=openxml-3.0.1). IDML frames are arbitrary `PathGeometry` (sample spread, §2.4) | frame |
| Text in a shape built from added and subtracted shapes, plus padding | Krita 5.3: "the text flow area can be composed of multiple shapes, with some adding and others subtracting" (https://krita.org/en/release-notes/krita-5-3-release-notes); Krita docs "Shape Padding" (https://docs.krita.org/en/reference_manual/dockers/text_properties.html) | frame |
| Auto-size: fixed, auto height, auto width and height; shrink to fit | Figma `textAutoResize` NONE / HEIGHT / WIDTH_AND_HEIGHT, `textTruncation`, `maxLines` (https://developers.figma.com/docs/rest-api/file-property-types/). Sketch `textBehaviour` Flexible / Fixed / FixedWidthAndHeight (https://github.com/sketch-hq/sketch-file-format). Office `normAutofit fontScale lnSpcReduction` stores the computed shrink (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.normalautofit?view=openxml-3.0.1). CSP "Wrap text at frame … overflow gets cut off" (https://help.clip-studio.com/en-us/manual_en/810_subtools/T.htm) | frame |
| Threaded or linked frames | Illustrator `story`, `nextFrame`, `previousFrame`, `flowLinksHorizontally` (TextFrameItem above). IDML `TextFrame ParentStory PreviousTextFrame NextTextFrame` (§2.4). Krita "text can flow across a chain of multiple shapes" (https://docs.krita.org/en/reference_manual/tools/text.html) | story → frames |
| Text on a path: start and end, side or flip, align to ascender, descender, center or baseline, effects (Rainbow, Skew, 3D Ribbon, Stair Step, Gravity), spacing | Illustrator `startTValue`, `endTValue` (TextFrameItem above). Path options (https://learn.adobe.com/illustrator/using/creating-type-path.html, https://creativepro.com/set-type-circle-square-s-curve-whatever/). SVG `startOffset`, `method`, `spacing`, `side`, `path`, `textLength`, `lengthAdjust` (https://www.w3.org/TR/SVG/text.html). Figma beta `textPathStartData{segment, position}` (https://developers.figma.com/docs/rest-api/file-property-types/) | frame (path) |
| Text wrap around other objects | Set on the *obstacle*: IDML `TextWrapPreference` sits on the wrapping item (sample spread, §2.4). Illustrator Object › Text Wrap works the same way [K] | the obstacle item |
| Warp and envelope | PSD `TySh` has a warp descriptor; ag-psd `WarpStyle` arc … twist, custom, cylinder (https://github.com/Agamnentzar/ag-psd/blob/master/src/psd.ts, https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/). Office `prstTxWarp` (bodyPr above). Illustrator Envelope Distort applies to any art [K] | item (generic deformer) |
| Text converted to outlines | Illustrator `createOutline()` returns a GroupItem (TextFrameItem above). Repository extension research: "a group of paths, with the text in the recipe" | separate result group |

### 1.2 East Asian and complex-script layout

| Feature | Evidence | Lives on |
|---|---|---|
| Vertical writing (tategaki): vertical-rl and vertical-lr | CSS Writing Modes 4 `writing-mode: horizontal-tb \| vertical-rl \| vertical-lr \| sideways-rl \| sideways-lr` (https://www.w3.org/TR/css-writing-modes-4/). Krita lists Writing Mode under paragraph properties that "can only be applied over a whole text shape" (https://docs.krita.org/en/reference_manual/dockers/text_properties.html). IDML `StoryPreference StoryOrientation` on the story (§2.4). PSD `Rendered.Shapes.WritingDirection` per layer (ag-psd above). Office `vert` horz / vert / vert270 / wordArtVert / eaVert / mongolianVert / wordArtVertRtl on the body (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.textverticalvalues?view=openxml-3.0.1). CSP Text Direction on the layer (https://help.clip-studio.com/en-us/manual_en/480_text/Vertical_text_and_readings.htm) | **item or story**, in every format checked |
| Upright vs rotated Latin in vertical text | `text-orientation: mixed \| upright \| sideways`; mixed uses UTR #50 `Vertical_Orientation` (CSS Writing Modes 4 above, https://www.unicode.org/reports/tr50/). CSP: "vertical text will be rotated 90 degrees". Krita does *not* support text-orientation yet: "planned in the future" (Krita docs above). Krita's Full-width text-transform is a workaround | run |
| Tate-chu-yoko (horizontal in vertical) | InDesign `tatechuyoko` plus X/Y offsets (https://developer.adobe.com/indesign/dom/api/c/CharacterStyle/). CSP Auto TateChuYoko for 1 to 4 half-width characters, or set by hand (CSP manuals above). CSS `text-combine-upright` (Writing Modes 4 above). JLREQ handling (https://www.w3.org/TR/jlreq/) | **span** (identity), plus an auto rule on the paragraph or item |
| Ruby (furigana): mono, group and jukugo; position; size; overhang | JLREQ "Positioning of Mono-ruby / Group-ruby / Jukugo-ruby" (https://www.w3.org/TR/jlreq/). InDesign `rubyFlag` and `rubyString` on Text, `rubyType`, `rubyAlignment`, `rubyPosition`, `rubyOverhang` (https://developer.adobe.com/indesign/dom/api/t/Text/). CSP Reading string, size, position and spacing (CSP manual above). CSS Ruby Layout (https://www.w3.org/TR/css-ruby-1/) [K] | **span** with a string payload |
| Emphasis marks (kenten, bōten) | InDesign `kentenKind`, `kentenPosition`, `kentenAlignment` (CharacterStyle above). CSS `text-emphasis` (https://www.w3.org/TR/css-text-decor-3/) [K]. I did not find it in the CSP manual | run |
| Warichu (inline two-line note) | InDesign `warichu`, `warichuLines`, `warichuSize` (CharacterStyle above). JLREQ "Inline Cutting Note (Warichu)" | **span** |
| Kinsoku (line-break prohibition), burasagari (hanging punctuation) | PSD `KinsokuSet` (PhotoshopKinsokuHard and Soft), `Burasagari`, `KinsokuOrder` (ag-psd above). Krita East-Asian hanging punctuation (Krita docs above). CSS `line-break` loose / normal / strict / anywhere in ICU4X (§4) | paragraph |
| Mojikumi (JIS X 4051 spacing between character classes), tsume (character compression) | PSD `MojiKumiSet`, `Tsume` (ag-psd above). JLREQ character classes and line adjustment (JLREQ above). InDesign `tsume`, `jidori` (CharacterStyle above) | paragraph (set ID) plus run (tsume) |
| Composite fonts (合成フォント): kana, kanji and Latin from different fonts, with per-class scale and baseline | InDesign and Illustrator composite fonts; mixing needs size and baseline adjustment (https://cssnite.jp/kobe/vol02/takano/CSSNiteKobe2-takano.pdf, https://www.itmedia.co.jp/news/article/2102/17/1210217084/2, https://community.adobe.com/質問-225/合成フォント設定でグレーアウトしている項目-421318). Manga "Antique" lettering pairs antique kana with gothic kanji, usually as pre-merged fonts (https://forest.watch.impress.co.jp/docs/news/1111761.html; summary level) | font slot kind |
| Bidi and RTL; per-paragraph base direction; span overrides | UAX #9 (https://www.unicode.org/reports/tr9/). Figma `TextLineData.directionality`, `directionalityIntent` per line (§2.6). Krita "Use Visual Cursor" (Krita tool docs) | paragraph, plus spans for overrides |
| Complex scripts: Arabic joining, Indic reordering, Thai and Khmer dictionary breaking | harfrust shaping; ICU4X segmentation (§4). Krita dominant and alignment baseline for Devanagari "hanging" (Krita docs) | engine; language on runs |
| Language tag (BCP 47) | Krita: "Language affects … glyph shape, upper- and lowercase and line breaking" (Krita docs). PSD `Language` per style (ag-psd). Han unification makes `locl` glyph choice depend on it [K] | run |

### 1.3 Line breaking, hyphenation, justification

- UAX #14 line breaking (https://www.unicode.org/reports/tr14/) with CSS `line-break` strictness and `word-break` normal / break-all / keep-all. ICU4X implements these (§4). Krita exposes Line Break and Word Break: "requires Language being set … Krita at this moment does not support Loose" (Krita docs).
- Hyphenation: PSD `AutoHyphenate`, `HyphenatedWordSize`, `PreHyphen`, `PostHyphen`, `ConsecutiveHyphens`, `Zone` (ag-psd).
- Justification: PSD `WordSpacing`, `LetterSpacing` and `GlyphSpacing` as [min, desired, max] arrays, and `EveryLineComposer` (Adobe's paragraph composer vs the single-line composer) (ag-psd). Office `algn` [K]. Krita "Align Last". Japanese line adjustment compresses and expands by character class (JLREQ).
- Optical margin alignment: IDML `StoryPreference OpticalMarginAlignment` (§2.4); Illustrator `opticalAlignment`.

### 1.4 Character-level typography

| Feature | Evidence |
|---|---|
| Tracking; kerning (metrics, optical or manual pair) | PSD `Tracking`, `AutoKerning`, `Kerning`. InDesign `kerningMethod` "type of pair kerning". Office `spc`, `kern` (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.runproperties?view=openxml-3.0.1). Adobe's optical kerning derives spacing from outlines (https://helpx.adobe.com/illustrator/using/line-character-spacing.html, from search; I could not open the page) |
| Leading: auto or absolute; line-height modes | PSD `AutoLeading`, `Leading`. Figma `lineHeightUnit` PIXELS / FONT_SIZE_% / INTRINSIC_%; the .fig "RAW" multiplier (https://grida.co/docs/wg/feat-fig/glossary/fig.kiwi) |
| Baseline shift; super- and subscript (synthesized or OpenType `sups`/`subs`); small caps (synthesized or `smcp`/`c2sc`) | PSD `BaselineShift`, `FontBaseline`, `FontCaps`, and document-level `SuperscriptSize`/`Position`, `SmallCapSize`. Figma `textCase` SMALL_CAPS / SMALL_CAPS_FORCED. Krita Small, All Small and Petite Caps. parley `VerticalAlign` with sub and super (§4) |
| Horizontal and vertical scale, skew, faux bold and italic | PSD `HorizontalScale`, `VerticalScale`, `FauxBold`, `FauxItalic`. InDesign `skew`. Krita "Synthesize Bold / Slant" |
| OpenType features with *integer values* (`liga`, `dlig`, `ss01`–`ss20`, `cv01`–`cv99`, `frac`, `tnum`, `salt` index …) | Krita feature list with search and "glyph palette" alternates. Figma stores only booleans: `openTypeFlags` map to 1 or 0. InDesign `otfStylisticSets` is a bitmask. Feature registry (https://learn.microsoft.com/en-us/typography/opentype/spec/featurelist) [K]. A stored value must be a u32, not a bool, because `cvXX` and `salt` take an index [K] |
| Variable axes per run | Figma `FontVariation{axisTag, axisName, value}` (kiwi schema, §2.6). Sketch `fontDescriptor.variation` map (§2.7). Krita "all axes included" |
| Color fonts and emoji: COLRv0/v1, SVG-in-OT, sbix, CBDT | Figma stores `emojiCodePoints` and `emojiImageSet` per glyph (kiwi). skrifa handles COLRv0/v1 and sbix/CBDT/EBDT; it has no SVG-in-OT painting (§4) |
| Decorations: underline and strikethrough with offset, size and colour; overline | parley `Underline*`/`Strikethrough*` (§4). Krita underline position per writing mode |
| Text transform (case, full-width), language-sensitive | Krita Text Transform, including Turkish dotted i (Krita docs) |
| Fill and stroke per character; several fills and strokes; stroke behind fill | PSD `FillColor`, `StrokeColor`, `FillFirst`, `OutlineWidth` (ag-psd). Krita 5.3 `paint-order` "outlines to be drawn behind the text" (release notes). CSP "Edge and Edge color" border (CSP sub-tool manual). Illustrator appearance stacks fills above or below a "Characters" entry [K] |
| Per-glyph transforms | SVG `x`/`y`/`dx`/`dy`/`rotate` lists indexed by code point (§2.1). Illustrator Touch Type rotates, scales and moves "one character at a time" while the text stays editable (https://creativepro.com/?p=14743). Krita: for text that is not auto-wrapped, "you can edit the position of each glyph" (release notes) |

### 1.5 Paragraph, structure and styles

- Paragraph: alignment and justify-last, first-line, start and end indents, space before and after (PSD `ParagraphProperties`; Krita Text Indent, including hanging and "indent after hard breaks").
- Lists: Figma `lineTypes` ORDERED / UNORDERED / NONE and `lineIndentations` per line (https://developers.figma.com/docs/rest-api/file-node-types/).
- Tabs: Krita Tab Size in `Sp` units. Tab stops with leaders [K].
- Character and paragraph styles: IDML `AppliedParagraphStyle` and `AppliedCharacterStyle` with local overrides (§2.4). PSD `StyleSheetSet` and `ParagraphSheetSet` with `TheNormalStyleSheet`. Krita Style Presets "apply a selection of properties", which is copy-on-apply (Krita docs). Figma text styles with `boundVariables`.

---

## 2. How the formats store text

### 2.1 SVG 2 (Candidate Recommendation Snapshot of 2026-10-06)

- Status: CR Snapshot 06 October 2026, Recommendation no earlier than 01 Dec 2026. At risk are several `title`/`desc` elements, nested links and `vector-effect` values (https://www.w3.org/TR/SVG/latest/).
- **`shape-inside` and `shape-subtract` are removed.** The WG resolved on 2026-03-05 to "Drop text wrapping in CSS shapes for SVG2"; PR #1097 merged on 2026-04-23 (https://github.com/w3c/svgwg/issues/1067, https://lists.w3.org/Archives/Public/public-svg-issues/2026Apr/). The TR Text chapter no longer has them (https://www.w3.org/TR/SVG/text.html).
- **`inline-size` is kept but at risk.** The WG resolved "Keep inline-size as it is the simplest text wrapping suggestion for SVG2". Every browser fails the `text-inline-size` tests. Krita and Inkscape implement it, and Krita's developer asked why Krita is not counted as an implementation (https://github.com/w3c/svgwg/issues/1087).
- **`textPath` `side` is kept** (resolution 2026-03-12; Firefox has shipped it since 61) (https://github.com/w3c/svgwg/issues/1068). The attributes are `path`, `href`, `startOffset`, `method`, `spacing`, `side`, `textLength` and `lengthAdjust`. There is no end offset (TR Text chapter).
- Model: nested `text` › `tspan` › `textPath`, with properties inherited through the element tree. Text layout is pre-formatted (`white-space` pre and pre-line), auto-wrapped in a content area, or on a path.
- **Index units are split.** The spec says positioning attributes index "Unicode code points (thus a 'u' followed by the combining diaeresis … is counted as two characters)", while "SVG DOM text methods … [use] UTF-16 code units". The WG asked for feedback on grapheme clusters (https://www.w3.org/TR/SVG/text.html, "addressable character").
- Writing mode lives on `text`. `text-orientation` defers to CSS Writing Modes (TR Text chapter; https://www.w3.org/TR/css-writing-modes-4/).
- Consequence for Capy: export of area or shape text must write positioned SVG 1.1 text or outlines. Inkscape and Krita can convert flowed text to positioned 1.1 text (#1087 thread; Inkscape "SVG 2: Insert SVG 1.1 fallback in text", https://wiki.inkscape.org/wiki/Translations:Release_notes/1.0/134/en; Krita "Convert to Pre-positioned", https://docs.krita.org/en/reference_manual/tools/text.html).

### 2.2 PDF

- Text is shown as strings of character codes mapped through a font to glyphs, positioned by text-state operators. There are no paragraphs or styles [K, ISO 32000-1 ch. 9].
- Fonts: embedded programs, or subsets named with a six-letter tag plus a plus sign (`EOODIA+Poetica`). Type 3 fonts define glyphs as "streams of PDF graphics operators", so outlines become a font. `ToUnicode` maps codes back to text. `ActualText` gives exact replacement text for a structure element or marked-content sequence, so outlined glyphs can stay searchable (ISO 32000-1:2008 §9.6.4, §9.6.5, §9.9, §14.9.4, https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).
- Licensing in the spec: "A font program may allow embedding for the sole purpose of viewing and printing the document but not for creating new or modified text that uses the font" (§9.9).

### 2.3 Illustrator

- Text frames are POINT, AREA or PATH (`kind`). Each frame belongs to a `story` and links through `nextFrame` and `previousFrame`. Path text has `startTValue` and `endTValue`. Area text has rows, columns and spacing. `matrix` is read-only, and `createOutline()` returns a GroupItem (https://ai-scripting.docsforadobe.dev/jsobjref/TextFrameItem/).
- File: "Create PDF Compatible File … stores two formats". Fonts in the PDF part are subset by default when less than 100% of the characters are used; `fontSubsetThreshold` defaults to 100 (https://helpx.adobe.com/illustrator/kb/optimize-native-pdf-file-sizes.html, https://illustrator-scripting-guide.readthedocs.io/jsobjref/IllustratorSaveOptions.html; both from search snippets, because helpx pages return 403).
- Missing fonts: a Missing Fonts dialog lists fonts available from Adobe Fonts; others are "Preview only". There is an Auto-activate Adobe Fonts preference. Failed text is "substituted with the default font and highlighted" (https://learn.adobe.com/illustrator/using/find-missing-fonts.html, from the search snippet).
- The native text engine is shared with Photoshop's EngineData syntax [K, unverified for current versions].

### 2.4 IDML (InDesign)

- **Stories are separate from frames.** `Stories/Story_uc9.xml` holds `<Story Self="uc9">` with `StoryPreference … StoryOrientation="Horizontal" StoryDirection="LeftToRightDirection" OpticalMarginAlignment=…` and nested `ParagraphStyleRange AppliedParagraphStyle=…` › `CharacterStyleRange AppliedCharacterStyle=…` › `<Content>`. The spread holds `<TextFrame Self="udb" ParentStory="uc9" PreviousTextFrame="n" NextTextFrame="n" ItemTransform="1 0 0 1 300.75 -0.75">`, a `PathGeometry`, a `TextFramePreference` and a `TextWrapPreference` (a real file from https://github.com/sillsdev/pathway/blob/master/DistFiles/InDesignFiles/Scripture/Stories/Story_uc9.xml and `…/Spreads/Spread_ub9.xml`).
- Threading identity: frames point to the story by ID and to their neighbours. Writing orientation is a *story* preference.
- Ruby is a character attribute with a string value (`rubyFlag`, `rubyString`), so ruby text is plain text with ruby-level settings. Kenten, tatechuyoko and warichu are also character attributes (https://developer.adobe.com/indesign/dom/api/t/Text/, https://developer.adobe.com/indesign/dom/api/c/CharacterStyle/).
- The official IDML specification PDF is no longer hosted. The last complete public version is CS5 (2010); Adobe moved the documentation into the SDK HTML (https://community.adobe.com/t5/indesign-discussions/where-is-the-idml-specification/m-p/13170191).

### 2.5 PSD (`TySh` and EngineData)

- `TySh` contains the version, a 6×F64 transform, a text descriptor, a warp descriptor and bounds (https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/). The descriptor's EngineData is a PostScript-like dictionary that the spec does not document.
- From ag-psd's typed decoder (https://github.com/Agamnentzar/ag-psd/blob/master/src/text.ts): `EngineDict.Editor.Text` is one string with `\r` paragraph breaks and a trailing `\r`. `ParagraphRun{DefaultRunData, RunArray, RunLengthArray}` and `StyleRun{…, RunLengthArray}` are run-length arrays in string units. `ResourceDict` holds `FontSet[{Name (PostScript), Script, FontType, Synthetic}]` (styles reference fonts *by index*, which is a slot table), `StyleSheetSet`, `ParagraphSheetSet`, `KinsokuSet`, `MojiKumiSet` and sub/superscript and small-cap sizes. `Rendered.Shapes` holds `WritingDirection` and `ShapeType` (point or box). Lengths are counted on a JavaScript string, so they are UTF-16 code units [K, inferred].
- The layer's rendered pixels are stored as normal channel data, so readers without the font show the stored rendering. psd-tools exposes `TypeLayer.text` and `engine_dict` read-only (https://psd-tools.readthedocs.io/en/latest/reference/psd_tools.api.layers.html). The pixel-fallback behaviour is [K]. Photoshop keeps the old rendering until edit and then warns that the layout will change (2011 thread: https://community.adobe.com/t5/photoshop-ecosystem-discussions/how-to-tell-what-fonts-are-missing-in-a-photosohp-cs5-document/m-p/3473659/highlight/true).

### 2.6 Figma

- REST: `characters`; `style`; `characterStyleOverrides[]`, one entry per character, an index into `styleOverrideTable`, where 0 means the default and characters past the array's end use the default; `lineTypes[]` and `lineIndentations[]` per *line*. TEXT_PATH is beta (https://developers.figma.com/docs/rest-api/file-node-types/). `TypeStyle` identifies fonts by `fontFamily`, `fontPostScriptName`, `fontStyle`, `fontWeight` and `italic` (https://developers.figma.com/docs/rest-api/file-property-types/). The index unit is not documented; JavaScript strings suggest UTF-16 [unverified] (https://developers.figma.com/docs/plugins/api/properties/TextNode-setrangefontname/).
- .fig kiwi schema, reverse engineered and unofficial (https://github.com/gridaco/grida/blob/main/.ref/figma/fig.kiwi):
  - `TextData{characters, characterStyleIDs[], styleOverrideTable, lines[TextLineData{lineType, indentationLevel, directionality, …}], layoutVersion, fallbackFonts[FontName], layoutSize, baselines, glyphs, decorations, fontMetaData, logicalIndexToCharacterOffsetMap, …}`.
  - `Glyph{commandsBlob, position, styleID, fontSize, firstCharacter, advance, emojiCodePoints, emojiImageSet, rotation}`, which is *stored outlines per glyph*.
  - `FontMetaData{key: FontName{family, style, postscript}, fontLineHeight, fontDigest: bytes, fontStyle, fontWeight}`.
  - The node also has `fontVersion`, `textUserLayoutVersion`, `textBidiVersion` and `fontVariations[{axisTag, axisName, value}]`.
  - Grida notes that `fontMetaData` "is the authoritative source" for weight and style (https://grida.co/docs/wg/feat-fig/glossary/fig.kiwi).
- Fonts: Google Fonts are built in, while local fonts need the desktop app or a font installer (https://help.figma.com/hc/en-us/articles/360039956994; forum threads cited in search). I found no embedding.

### 2.7 Sketch

- `text.attributedString{string, attributes[stringAttribute{location, length, attributes{MSAttributedStringFontAttribute: fontDescriptor{name, size, variation}, MSAttributedStringColorAttribute, kerning, paragraphStyle{…}, textStyleVerticalAlignmentKey}}]}`, plus `textBehaviour` and `glyphBounds` (schema YAML in https://github.com/sketch-hq/sketch-file-format, `packages/file-format/schema/{layers/text,objects/attributed-string,objects/string-attribute,objects/font-descriptor}.schema.yaml`). Locations are NSString offsets, which are UTF-16 [K].
- Embedded fonts sit at document level as `fontReferences[fontReference{fontData, fontFamilyName, fontFileName, options, postscriptNames}]` (`objects/font-ref.schema.yaml`). Embedding is an explicit checkbox for non-system fonts, "requires … the correct license", and is automatic on the first save to a Workspace. Missing fonts can be replaced by family and weight (https://www.sketch.com/docs/interface-and-settings/document-settings/fonts/, from search).

### 2.8 Office DrawingML (`txBody`)

- `bodyPr` attributes are `rot`, `vert`, `wrap`, insets, `numCol`, `spcCol`, `rtlCol`, `anchor`, `anchorCtr`, `upright` and `vertOverflow`/`horzOverflow`. Its children are `prstTxWarp` and one of `noAutofit`, `normAutofit` or `spAutoFit` (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.bodyproperties?view=openxml-3.0.1). `normAutofit fontScale lnSpcReduction` stores the *computed* shrink, and absent means 100% (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.normalautofit?view=openxml-3.0.1).
- Runs are nested elements (`a:p` › `a:r` › `a:rPr` + `a:t`). There are no offsets.
- `rPr` has **separate font slots per script class**: `latin`, `ea` (East Asian), `cs` (complex script) and `sym` (https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.runproperties?view=openxml-3.0.1).
- Embedding offers "Embed only the characters used", which "limits editing", or all characters. It respects the font's Non-embeddable, Preview/Print, Editable or Installable setting (https://support.microsoft.com/en-us/office/fonts/benefits-of-embedding-custom-fonts; platform limits in https://www.microsoft.com/en-us/microsoft-365/blog/2015/07/06/document-font-embedding-demystified/).

### 2.9 Krita 5.3 / 6.0 (released 2026-02-05)

- Text is SVG inside `.kra`, with SVG 2 text types: Preformatted, Pre-positioned, Text on Path, Inline Wrapped and Text in Shape (with subtract shapes and chains). An SVG source editor is included (https://docs.krita.org/en/reference_manual/tools/text.html, https://krita.org/en/release-notes/krita-5-3-release-notes).
- More than 50 properties; writing mode is a whole-shape property; text-orientation is not supported yet; language and script per range; the CSS font-family list is a fallback chain; fonts are "resources [that] can be tagged, searched" (https://docs.krita.org/en/reference_manual/dockers/text_properties.html).
- Fonts are not embedded. A forum answer says embedding "would give a lot of copyright issues"; versions before 5.3 also omitted the default family (https://krita-artists.org/t/i-need-help-in-fonts/15732 [unverified attribution, from a search summary], https://krita-artists.org/t/the-text-editor-changes-the-default-font-with-each-krita-update/98109).
- Krita's area text relies on `shape-inside`, which SVG 2 has now removed (§2.1).

### 2.10 Clip Studio Paint

- Text layers have Text Direction (vertical or horizontal), Auto TateChuYoko (0 to 4 characters) or manual TCY, half-width punctuation, Readings (ruby string with size, position and spacing), "Wrap text at frame", Edge and edge colour, and a background colour (https://help.clip-studio.com/en-us/manual_en/480_text/Vertical_text_and_readings.htm, https://help.clip-studio.com/en-us/manual_en/810_subtools/T.htm).
- The `.clip` storage of text is undocumented [unverified].

### 2.11 Common patterns

| Pattern | Who | Notes |
|---|---|---|
| One plain string plus *full-coverage run lengths* into a style table | PSD (StyleRun / ParagraphRun), Figma (per-character index array), parley `StyleRunBuilder` ("contiguous, non-overlapping spans that cover 0..text.len()") | Easy to validate: lengths sum to the text length |
| One plain string plus (location, length) attribute ranges | Sketch, parley `RangedBuilder` | Ranges can overlap; needs resolution rules |
| Nested elements | SVG, IDML, DrawingML | Inheritance, so a cascade is needed; Capy's design stores specified values, never a cascade (repository extension research) |
| Paragraph attributes keyed by paragraph or line | PSD ParagraphRun (lengths must align to paragraphs), Figma per line, IDML ParagraphStyleRange | |
| Fonts by index into a per-object or per-document table | PSD FontSet, Figma fontMetaData, Office per script class | This is a slot table |
| Font identity = PostScript name (+ family/style) | PSD, Sketch, Figma, Illustrator SVG export | Figma adds a font *digest* and version |
| Stored layout result | Figma glyph outlines, PSD pixels, PDF glyph runs, Office `fontScale`, Inkscape 1.1 fallback | Layout versioning: Figma `layoutVersion` and `textUserLayoutVersion` |
| Offsets | UTF-16: PSD, Sketch, likely Figma, SVG DOM. Code points: SVG positioning. UTF-8: Rust libraries | No common unit; converting at import is unavoidable |

---

## 3. Fonts

### 3.1 Embedding permissions (OS/2 `fsType`)

From https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fstype:

- **0, Installable:** may be embedded and permanently installed.
- **2, Restricted License:** "must not be modified, embedded or exchanged".
- **4, Preview & Print:** "Documents containing Preview & Print fonts must be opened 'read-only'".
- **8, Editable:** editing is permitted, including formatting new text with the font.
- **Bit 8, No subsetting.** **Bit 9, Bitmap embedding only.**
- Bits 0–3 must be mutually exclusive in OS/2 version 3 and later. In versions 0–2 the least-restrictive bit wins.
- Applications "must not modify the embedding permissions". Applications that load fonts temporarily "**must** delete the fonts when the document … is closed".
- On the Fedora 44 workstation used for this research, 251 font files: 198 are fsType 0, 35 are 4 (URW base35 such as NimbusSans and C059) and 18 are 8 (Droid). None are 2 [M].
- What applications do:
  - PDF: honours embedding conditions (§2.2).
  - Office: offers subset or full embedding by permission (§2.8).
  - Sketch: explicit, licence-gated embedding (§2.7).
  - Figma: no embedding; digest only.
  - Krita: no embedding (§2.9).
  - Illustrator: embeds subsets in the PDF part (§2.3).
- **Outlines vs embedding:** whether stored glyph outlines count as "embedding" under a font licence is [unverified]. PDF Type 3 makes outlines a font. Figma and Illustrator store or write outlines for any font. I found no authoritative statement.

### 3.2 Subsetting

- Subsets change glyph IDs unless the subsetter retains them; HarfBuzz has `--retain-gids` [K]. Stored glyph IDs must name their glyph space: the original font, or a subset with retained IDs.
- "Embed only the characters used" limits editing (Office, §2.8).
- Rust subsetters:
  - typst `subsetter` 0.2.6, MIT/Apache: "most likely unusable in any other contexts than PDF writing" (https://github.com/typst/subsetter).
  - fontations `skera` 0.8.0 (2026-10-03), MIT/Apache, a Rust subsetter (https://github.com/googlefonts/fontations/tree/main/skera).
  - `allsorts` 0.17, Apache-2.0 (https://github.com/yeslogic/allsorts).

### 3.3 Identity and variable instances

- PostScript name (name ID 6) is ASCII, at most 63 characters. Variable fonts may carry ID 25, the "Variations PostScript Name Prefix", used with Adobe Tech Note #5902 to generate instance names (https://learn.microsoft.com/en-us/typography/opentype/spec/name, https://adobe-type-tools.github.io/font-tech-notes/pdfs/5902.AdobePSNameGeneration.pdf [K for the TN]).
- **Measured pitfall [M]:** `/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc` face 0 has `wght 100..100..900`, so its *default instance is Thin*. Its PostScript name reads `NotoSansCJKjp-Thin`, and named instances list Thin, Light, DemiLight and so on. "Noto Sans CJK JP Regular" from this file therefore needs `wght=400` stored explicitly. The PostScript name in the file does not identify the instance. Store **user-space axis values by tag**, as Figma and Sketch do, not normalized values and not only instance names.
- The same family can differ by version, foundry or packaging (TTF, OTF or WOFF2). Figma's `fontDigest` and `fontVersion` show the fix: a content digest decides "exactly the same font" and the names decide "a compatible substitute".

### 3.4 Size: stored outlines vs subsets [M]

Sample text: 204 Japanese manga-dialogue characters (89 unique) and 258 English characters (39 unique). Fonts: Noto Sans CJK JP VF at its default (Thin) location, and Noto Sans [wght]. Counts come from skrifa `OutlinePen`, and the encoding mimics Capy's planned path format. Each anchor has a u8 kind, an F64 point and two F64 handle offsets. Each segment has a u8 kind. There is one item per glyph, positioned in layer pixels at 24 px.

| Payload | Japanese (203 glyph items) | English (215 glyph items) |
|---|---|---|
| Capy-style F64 path items, raw | 267,193 B | 247,395 B |
| … LZ4 block (`lz4_flex`) | 85,279 B | 77,196 B |
| … zstd -19 | 38,160 B | 31,025 B |
| Unique glyphs only (F32 cubic estimate) | about 44 KB (89 glyphs) | about 15 KB (39 glyphs) |
| `hb-subset` 14.1 subset, instanced at wght 400 | 32,372 B (LZ4 29,152) | 5,932 B (LZ4 4,851) |
| Same subset, still variable | 61,124 B | 9,736 B |

Per glyph, outlines cost about 1.3 KB raw and 0.4 KB after LZ4 for CJK. A dense manga page of about 1,000 glyphs is therefore about 1.3 MB raw and 0.4 MB LZ4. That is small next to Capy's raster tiles, but 3 to 10 times a subset. Deduplicating repeated glyphs saves about 2× for Japanese and 6× for English.

Reproduce:
- `probe/outl/src/main.rs` and `probe/outl/src/bin/enc.rs`, with sample texts `probe/ja.txt` and `probe/en.txt`.
- `hb-subset --face-index=0 --text-file=ja.txt --instance="wght=400"`.

### 3.5 Missing-font handling

- **Illustrator:** dialog with activation from Adobe Fonts or "Preview only"; auto-activate preference; substitute font plus highlight (§2.3).
- **InDesign:** pink highlight and a Missing section in the Font menu (from the search summary of https://helpx.adobe.com/si/indesign/desktop/troubleshoot/font-and-plugin-issues/missing-font-error.html).
- **Photoshop:** a warning triangle on the layer; the old rendering is kept until edit; editing warns that the layout will change (2011 thread, §2.5).
- **Sketch:** replace with family and weight.
- **Figma:** missing-font icon; replacing a font "will drop all of the applied styles" (forum, from search).
- **Krita:** a "same family or similar fallback" substitute, and in older versions the original name was lost (https://bugs.kde.org/show_bug.cgi?id=381560).

Lesson: keep the original descriptor through substitution until the user explicitly replaces it. Render the stored result until the text is edited.

### 3.6 Platform font access

| Host | Access | Evidence |
|---|---|---|
| Android | NDK `ASystemFontIterator_*` and `AFont_getFontFilePath` (API 29), with weight, italic, locale (usable from API 30), collection index and axes. Downloadable Fonts through the Google Play services provider (`com.google.android.gms.fonts`, FontsContract) | https://developer.android.com/ndk/reference/group/font, https://android.googlesource.com/platform/frameworks/native/+/android-16.0.0_r2/include/android/font.h, https://developer.android.com/develop/ui/views/text-and-emoji/downloadable-fonts. fontique reads `$ANDROID_ROOT/etc/fonts.xml` instead (crate source) |
| iOS / iPadOS | Core Text. Fonts installed by "font provider" apps need the `com.apple.developer.user-fonts` entitlement (`app-usage` to use them, `system-installation` to install) | https://developer.apple.com/videos/play/wwdc2019/227/ and forum threads from search. fontique has a CoreText backend (crate source) |
| Windows | DirectWrite system collection | fontique `dwrite` backend (crate source); per-user fonts [K] |
| Linux | fontconfig | fontique `fontconfig` backend, optionally dlopen (crate source) |
| Web | `window.queryLocalFonts()` works only in Chromium from version 103, behind a permission prompt. Firefox and Safari do not support it, and Mozilla has no position yet. Otherwise only fonts the app ships, the user imports or fetches, or the document embeds | https://developer.chrome.com/docs/capabilities/web-apis/local-fonts, https://developer.mozilla.org/en-US/docs/Web/API/Local_Font_Access_API, https://github.com/mozilla/standards-positions/issues/401 |

**Consequence:** on the web, "the font is present" is the exception. Editing a document from another device on the web needs embedded or library fonts. Google Fonts (OFL or Apache) can be re-fetched by family. Adobe Fonts activate only inside Creative Cloud apps.

### 3.7 What a portable `.capy` should store

1. **Source story and layout inputs**, so the text stays editable whenever a matching font exists.
2. **Laid-out result.** Glyph runs: font, glyph ID, position, advance, cluster offset and optional per-glyph transform. Add decorations and the outline geometry, including colour layers or bitmap references for colour glyphs. The document then renders identically everywhere, including where the font is missing or the layout engine differs.
3. **Font descriptors for every font used**, including system fallback fonts chosen during layout: family, style, PostScript name, weight, width and slope, axis values, face index, version string and content digest.
4. **Optional embedded font program** (full, or a subset with retained glyph IDs). Only with user opt-in and when `fsType` is 0 or 8, never 2. Skip 4, because outlines already give display. With bit 8 set, embed full or not at all; with bit 9, never.
5. **A layout version**: engine, Unicode and ICU data, and hyphenation patterns. On open, use the stored result; re-lay out only on edit or an explicit "Update text". This mirrors the repository rule that "artwork never changes because the app was updated".

---

## 4. Rust text stack (crates.io metadata fetched 2026-10-16)

### 4.1 Crates

| Crate | Version (date) | License | Role and status | Notes checked in source |
|---|---|---|---|---|
| parley | 0.12.0 (2026-10-09) | Apache-2.0 OR MIT | Rich text layout (linebender) | No writing-mode or vertical layout in `StyleProperty`. Justify is inter-word. No hyphenation or tab-size. `RangedBuilder`, `TreeBuilder` and `StyleRunBuilder` all use UTF-8 byte ranges. Depends on harfrust **0.12** and skrifa **0.44**, behind the latest. Blitz tracking issue: span-level bidi overrides, `tab-size` and small caps are unchecked (https://github.com/linebender/parley/issues/208). Open issue "East Asian line breaking rules" (https://github.com/linebender/parley/issues/301) |
| parley_engine / parlance | 0.12.0 / 0.1.1 (2026-10-09) | Apache-2.0 OR MIT | Low-level analysis and shaping; property vocabulary (`Tag`, `Language`) | `complex-scripts` feature switches ICU4X segmenters |
| fontique | 0.12.0 (2026-10-09) | Apache-2.0 OR MIT | Font enumeration and fallback | Backends for fontconfig, CoreText, DirectWrite and Android `fonts.xml`; `register_fonts(Blob)` for in-memory fonts (wasm) |
| harfrust | 0.14.0 (2026-10-04) | MIT | HarfBuzz port on read-fonts, matching HarfBuzz 14.5.1; "less than 25% slower" | Vertical shaping: `Direction::TopToBottom`, `vert` substitution, `glyph_v_advance` and vertical origin; clusters are UTF-8 byte indices (`push_str`). No graphite; malformed fonts error (https://github.com/harfbuzz/harfrust) |
| rustybuzz | 0.20.1 (2024-11) | MIT | **Archived**: "recommend that all users switch to HarfRust" (https://github.com/harfbuzz/rustybuzz) | Do not adopt |
| skrifa / read-fonts | 0.48.0 / 0.45.1 (Oct 2026) | MIT OR Apache-2.0 | Font metadata, outlines, hinting, variations | `ColorGlyphFormat::{ColrV0, ColrV1}` painting; bitmaps sbix, CBDT and EBDT; no SVG-in-OT painting found |
| swash | 0.2.10 (2026-07) | Apache-2.0 OR MIT | Shaping, scaling, rendering; no layout | Depends on skrifa ≤ 0.44 |
| cosmic-text | 0.19.0 (2026-04) | MIT OR Apache-2.0 | Editor-oriented layout | No vertical support found; pinned to harfrust 0.5 and skrifa 0.40 |
| icu_segmenter (ICU4X) | 2.3.0 (2026-08) | Unicode-3.0 (permissive) | UAX #14 and #29 | `LineBreakStrictness` Loose / Normal / Strict / Anywhere; `LineBreakWordOption` Normal / BreakAll / KeepAll. LSTM and dictionary models for Southeast Asian scripts. 4.2 MB crate, mostly data |
| unicode-bidi | 0.3.18 (2024-12) | MIT OR Apache-2.0 | UAX #9 | parley has its own bidi |
| hypher | 0.1.8 (2026-09) | MIT OR Apache-2.0 (code) | Hyphenation, 48 languages, about 1.1 MiB all-in (English 27 KiB) | **Pattern licences are per language: "LPPL, MPL, MIT, BSD-3"**, which needs review against Capy's publication rules (https://github.com/typst/hypher) |
| hyphenation | 0.8.4 (2021-08) | Apache-2.0/MIT | Knuth-Liang | Stale; pattern licences [unverified] |
| unicode-linebreak | 0.1.5 (2023) | Apache-2.0 | Plain UAX #14 | No CSS strictness [K] |
| skera / subsetter / allsorts | see §3.2 | permissive | Subsetting | |

All licences above are permissive. hypher's patterns include LPPL and MPL (file-level copyleft) and need a decision.

### 4.2 Gaps Capy would fill itself

- Vertical line layout: column stacking, vertical metrics fallback, `text-orientation`. Noto Sans has no `vmtx` or `vhea` [M], so synthesized vertical metrics are required.
- Ruby, tate-chu-yoko, warichu and emphasis marks.
- JIS X 4051 kinsoku with burasagari, oikomi and oidashi, and mojikumi spacing tables.
- Inter-character (CJK) justification.
- Hyphenation integration, tab stops, span bidi overrides and text on path.
- Composite fonts.

harfrust and ICU4X give the primitives; parley covers horizontal paragraphs. None of this changes the file format, but it explains why the result must be stored: Capy's own layout will change between versions.

### 4.3 Measured wasm32 sizes [M]

Setup:
- Toolchain: cargo 1.99.0 (2026-08-27), `wasm32-unknown-unknown`.
- Profile: `opt-level="s"`, `lto=true`, `codegen-units=1`, `panic="abort"`, `strip=true`. No wasm-opt.
- Each probe is a cdylib that exports one function. It registers a font from bytes, lays out and draws outlines into a counting pen.

| Variant | Raw | gzip -9 | brotli 11 |
|---|---|---|---|
| skrifa 0.44 outlines only | 327 KB | 109 KB | 81 KB |
| harfrust 0.12 shaping + skrifa outlines | 831 KB | 287 KB | 222 KB |
| parley 0.12 (`std`, no system fonts) + skrifa outlines | 1,226 KB | 439 KB | 343 KB |
| … + `complex-scripts` (ICU4X LSTM and dictionary data) | 5,020 KB | 2,865 KB | 2,409 KB |

The repository already measured usvg with text at 1.51 MB vs 385 KB without (`docs/history/vector-layers-research/07-rust-feasibility.md`). Southeast Asian word breaking should be a lazily loaded module on the web.

---

## 5. Design recommendation

### 5.1 The central decision: where the stored result lives

**Option A: result items plus a text recipe** (repository pattern "Text converted to outlines"). The text is a Group of ordinary Path items, one or more per glyph or paint run. The story is an ignorable, not copy-safe recipe property on the group.

Strengths:
- Readers without text support open the document **editable**, and editing turns the text into outlines (Illustrator Expand).
- The renderer is reused.

Problems:
- **The current rule forbids ignorable data from using slots,** so the recipe cannot reference an embedded font resource. Text on another path and text wrap depend on other items, so they cannot be ignorable either. The repository already lists "text on a separate path" as an exception that needs required data.
- **History churn.** Typing one character re-positions every later glyph item. With about 1.3 KB per glyph, each keystroke rewrites most chunks of a long story, unless the in-memory model treats the children as derived.
- **Size.** About 1.3 KB per glyph, with no deduplication (§3.4).
- **Glyph IDs and clusters are lost.** PDF and SVG export of real (selectable) text needs the recipe and the font. ActualText over outlines is the fallback.

**Option B, recommended: a required Text kind with an inline result.**
- Text is a required kind, unknown to older readers, so they preserve the file.
- The item holds:
  - the story;
  - the layout inputs: mode, frames or path, writing mode;
  - a `fonts` descriptor table;
  - a **result** section: glyph runs plus a deduplicated outline table keyed by (font, glyph ID, axis location). Colour layers and bitmap references go in the same table.
- **No child items.** That avoids the unknown-container problem in §6(a1).
- Readers that know Text but lack the font draw the result.
- Later text features can be *ignorable* sub-tags. An older text-aware reader renders the stored result, which already includes ruby or vertical glyphs, and drops the feature only if it edits that text. Or they can be required, and the document is preserved.

This matches Figma: per-node glyph outlines plus a digest and layout version. PDF and PSD use the same idea of a result stored next to the source. Keep option A for Convert to Outlines and for SVG import before Text exists.

Cost of B: documents with text open as Preserved in versions without text, and those readers can only show the stored preview. For a pre-release product, that transition cost is lower than A's permanent constraints. If pre-text editability matters more, choose A *and* add a slot rule for ignorable data (§6 a1.3).

### 5.2 Minimal Text item (tagged-property sketch)

All values are specified, never cascaded. Absent optional tags mean frozen wire defaults that reproduce the previous version's layout. U64 values follow the package conventions.

```text
Text item  (kind: text, required)
  affine            F64 2×3 (already planned)
  story             required
    chars           UTF-8 bytes; LF = paragraph separator, U+2028 = forced line break,
                    U+0009 tab, U+00AD soft hyphen, U+FFFC inline object; no normalization
    styles[]        each a tagged property set (font index, size in layer px, paint style,
                    language BCP-47, features [(tag,u32)], axes [(tag,f32 user value)],
                    tracking ‰em, baseline shift, …); writers deduplicate
    style_runs[]    (len_bytes u32, style u32); full coverage, no zero length; coalescible
    para_styles[]   tagged property sets (align, indents, spacing, direction auto|ltr|rtl, …)
    paras[]         para_style u32 per paragraph (count = number of LF + 1)
    spans[]         later: (start u32, len u32, kind, payload); never coalesced; well nested
  fonts[]           required; descriptors (family, style, postscript, weight, width, slope,
                    axes, face index, version, digest sha-256, source hint), optional
                    embedded = index into the record's `fonts` slot list
  layout            required
    mode            point | area | path
    point:          origin (0,0) in local space, anchor start|middle|end
    area:           frames[] (owned geometry + rule; list length 1 now, >1 = threading),
                    insets, columns+gutter, v_align, sizing fixed|auto_h|auto_wh, overflow
    path:           owned path geometry, start, end, side, align_to, effect, spacing
    writing_mode    horizontal-tb | vertical-rl | vertical-lr (item level; default horizontal)
  result            required to render without the font; regenerated on edit
    layout_version  engine + Unicode/ICU + pattern data identifiers
    glyph_runs[]    (font index, size, axis location, paint style, cluster base) →
                    glyphs[] (gid u32, x,y F64 or F32, advance, cluster byte offset,
                    optional per-glyph affine for sideways or path text)
    outlines[]      keyed (font index, gid, location): cubic geometry in em units;
                    colour layers or bitmap image refs (image slot) for colour glyphs
    decorations[]   rectangles or paths with paint
```

Points that need care:
- **Coalescing.** Writers may merge equal adjacent style runs. Anything with identity (ruby groups, tate-chu-yoko units, links, warichu) must be a span. CSP and InDesign treat each TCY or ruby group as one unit.
- **Paragraph table.** One entry per paragraph is simpler to validate than PSD-style paragraph run lengths. PSD has to trim trailing `\r` counts (ag-psd `removedCharacters`).
- **Units.**
  - Font size is in layer pixels before the item affine, as path geometry is. The UI converts to pt or Japanese Q/H through the composition `resolution`.
  - Tracking and kerning are in 1/1000 em, the Adobe convention, so they survive size changes [K].
  - Axis values are in user space.
- **Language** should be on runs from day one. An absent value is a frozen default `und`. New text should store the UI or input language explicitly, because Han `locl` choice and CJK line breaking depend on it.
- **Fonts used by fallback** are added to `fonts[]` and referenced by result runs, so the result names its real source fonts. Layout on another device can then try the same fallback first.

### 5.3 Font resource and slot

- Add a `capy.font/1` resource: immutable sfnt bytes, raw or `capy.lz4-bytes/1`.
- Its data JSON holds: face count or index; PostScript name, family and version; the original font's digest; `fs_type` copied unchanged; whether it is a subset, with `retain_gids`; and the coverage summary when subset.
- The record JSON `fonts` slot list binds slot index → `{"ref"}`. Only embedded fonts need a slot. Descriptors stay in the item (§5.2).
- Sharing one resource across text items and layers follows the existing rule that "resource sharing never implies shared editing".
- Load embedded fonts only into Capy's in-process collection (fontique `register_fonts`). Never install them into the OS or the user's library, as the fsType rules require. Unload them when the document closes.

### 5.4 Fingerprint, freshness and editing

- A text-aware reader with the exact fonts (digest match) and the same `layout_version` may re-lay out silently on edit.
- With a different font version, a substitute, or a newer layout version: show the stored result. On the first edit, warn that the layout may change, as Photoshop does. Keep the original descriptor until the user replaces it.
- With option A, an older reader that edits the outline group drops the recipe automatically (not copy-safe), so no fingerprint is needed. With the ancillary-record variant of an earlier draft, the fingerprint must cover the result geometry.

---

## 6. Gaps vs the planned Capy model

### (a) Structural: decide now, or a redesign follows later

**a1. Shared vector-format rules, before the v1 vector freeze.**

1. **Container-ness of unknown kinds.** The stream is pre-order with End markers. A reader that meets an unknown kind cannot tell whether children and an End follow. An ignorable unknown container would then make the stream look unbalanced, which is *Invalid*, not *Preserved*. Encode "opens a subtree" in the kind number (a third bit) or forbid ignorable container kinds. The recommended Text kind (§5.1 B) has no children, which avoids this for text. Group and later containers still need the rule.
2. **Slot entries.** Decide that a slot-list entry is a JSON object binding a resource (`{"ref"}` plus metadata), not a bare reference, and that font *identity* lives in the item, not in slots. Otherwise system-font text would need "descriptor-only resources".
3. **Ignorable data and slots.** Current rule: ignorable data never uses slots, because writers renumber slots. With that rule, option A text cannot embed fonts. Either accept that (embedding makes the text required data), or add *stable-key* slots that ignorable data may reference, kept while their referrer is kept. Decide which before recipes ship.
4. **Unsupported content inside a known recipe.** Decide whether a recipe with an unknown required sub-tag degrades to an opaque, unknown ignorable property. The result would still render, and the recipe would drop on edit, instead of the whole document becoming Preserved. Without this rule, every new text attribute in a text recipe forces Preserved on older text-aware readers.
5. **Pre-text SVG import.** The repository says that until text exists, "import outlines it and keeps the source text as a recipe". Whatever that recipe stores is frozen when import ships, before Text. Store the **original SVG `<text>` fragment as UTF-8** (ignorable, not copy-safe) rather than an early Capy text model. Otherwise the offset unit, paragraph and font-descriptor choices in a2 are fixed prematurely.
6. **Property value types.** Text needs UTF-8 strings, byte blobs, nested tagged property sets (style tables), arrays of fixed records (runs, glyph runs) and 4-byte OpenType tags. Confirm that the tagged-property encoding allows nested tagged lists everywhere, not just in paint entries, and that admission bounds (bytes per property, entries per array) scale to stories of about 1 MB.
7. **Item-local preservation (policy).** "Preserved is whole-document" means one balloon using a newer text feature makes a whole manga page read-only in an older text-aware version. If that is unacceptable, an "item locked, render stored result" outcome needs to exist in the reader model before freeze.

**a2. Text vocabulary, frozen the first time any text data ships (Text kind or a text recipe). Cannot change afterwards.**

1. **Encoding and offset unit:** UTF-8 storage, run lengths in UTF-8 bytes on scalar boundaries, never graphemes, no normalization on save. Imports convert from UTF-16 (PSD, Sketch, Figma) and code points (SVG positioning).
2. **Separators:** LF = paragraph; U+2028 = forced line break; CR, CRLF and U+2029 normalized on input.
3. **Run model:** a coalescible full-coverage style-run array into a style table, plus a separate non-coalescing span list. Ruby, tate-chu-yoko and warichu are spans.
4. **Paragraph attributes keyed by paragraph index.**
5. **Result storage** (§5.1). Decide A vs B and the result representation: glyph runs, a deduplicated outline table, colour-glyph handling. Decide that results are used on open and regenerated only on edit (`layout_version`).
6. **Story and frame topology:**
   - Text item = story.
   - Frames and the text path are owned geometry inside the item.
   - Threading = more frames in the same item, with no cross-item identity.
   - Limitation: threaded frames share one z-position.
   - Linking to another item's path is a later required tag.
7. **Font identity:** an inline descriptor table with digest and user-space axes; embedded bytes via the `fonts` slot.
8. **Writing mode at item level.** Every format checked puts it on the story, frame or layer. `text-orientation` and combine-upright go on runs or spans.
9. **Specified values only,** with frozen defaults whose absence reproduces the previous layout. Named styles cannot be a cascade later; they arrive as a reference plus local overrides (b).

### (b) Additive later (named slot)

| Feature | Slot |
|---|---|
| Ruby, tate-chu-yoko, warichu, links, inline objects (U+FFFC) | `story.spans[]` kinds |
| Emphasis marks, text-orientation, combine-upright auto rule | style-table tags; paragraph or item tag for auto TCY |
| Kinsoku set, mojikumi set, burasagari, hanging punctuation, optical margins | para-style tags (built-in set IDs) |
| Hyphenation (on, min word, before/after, limit), justification ranges, composer | para-style tags; pattern-data ID inside `layout_version` |
| Tab stops (position, alignment, leader), lists (type, level, format, start) | para-style tags |
| Tracking, kerning (manual on the preceding character, or metric vs optical), leading modes, baseline shift, super/sub, small caps, scale, skew, faux styles | style-table tags |
| OpenType features (tag, u32), variable axes | style-table tags (from v1 of Text if possible) |
| Per-glyph transforms (Touch Type, SVG x/y/dx/dy/rotate) | style-table tags on one-character runs, or `story.glyph_adjust[]` keyed by byte offset |
| Area-text extras: columns, insets, auto-size, overflow, first-baseline rule, subtract shapes | `layout.area` tags |
| Threading across several frames | `layout.area.frames[]` length > 1 |
| Path-text extras: end offset, effect, align-to, spacing | `layout.path` tags |
| Text wrap around items | `text_wrap` tag on the *obstacle* item plus `ignore_wrap` on Text; required, because it depends on other items; same vector object only |
| Warp and envelope | the generic warp group or recipe already planned for envelopes |
| Several fills and strokes on the whole text above or below character paints | item-level style plus a "characters" position tag |
| Composite fonts (kana, kanji and Latin from different fonts) | a `fonts[]` descriptor kind `composite` mapping character classes to other font indices with scale and baseline |
| Colour glyphs: COLRv1 gradients and composite modes, bitmap emoji | `result.outlines` layer entries using later paint kinds; bitmap glyphs via the `images` slot |
| Embedded fonts, subsets, Google Fonts re-fetch hint | `capy.font/1` resource; `fonts` slot; descriptor `source` tag |
| Character and paragraph styles (named) | a style reference tag in style entries, pointing to the many-use editable record type already deferred in the repository design |
| Text in clip groups | Text item as a clip child (geometry = result outlines) |

### (c) Out of scope

- Ligature and kerning *editing*: Capy stores features, not font editing. The same applies to SVG fonts, `tref` and `altGlyph`, all removed from SVG 2.
- Graphite shaping: harfrust has none.
- Optical kerning that matches Adobe's proprietary algorithm.
- Dynamic or linked text: variables, data merge, Figma `boundVariables`.
- Hyperlink behaviour beyond storing a span.
- Incremental Font Transfer, and lazy loading of web fonts during layout (parley tracking issue #208 lists these as future work).
- Text tracking-changes and notes.
- Reading or writing CSP `.clip` text: undocumented.
- Pixel-exact match with Adobe's composers or Japanese engines.

---

## Sources

Standards and specifications
- https://www.w3.org/TR/SVG/latest/ · https://www.w3.org/TR/SVG/text.html · https://w3c.github.io/svgwg/svg2-draft/text.html
- https://github.com/w3c/svgwg/issues/1067 · https://github.com/w3c/svgwg/issues/1087 · https://github.com/w3c/svgwg/issues/1068 · https://lists.w3.org/Archives/Public/public-svg-issues/2026Mar/0004.html · https://lists.w3.org/Archives/Public/public-svg-issues/2026Apr/
- https://www.w3.org/TR/css-writing-modes-4/ · https://www.w3.org/TR/jlreq/ · https://www.w3.org/TR/css-ruby-1/ [K] · https://www.w3.org/TR/css-text-decor-3/ [K]
- https://www.unicode.org/reports/tr9/ · https://www.unicode.org/reports/tr14/ · https://www.unicode.org/reports/tr29/ · https://www.unicode.org/reports/tr50/
- https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fstype · https://learn.microsoft.com/en-us/typography/opentype/spec/name · https://learn.microsoft.com/en-us/typography/opentype/spec/featurelist [K] · https://adobe-type-tools.github.io/font-tech-notes/pdfs/5902.AdobePSNameGeneration.pdf [K]
- https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf (read from a local copy saved earlier in this session)
- https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/

Applications and formats
- Illustrator: https://ai-scripting.docsforadobe.dev/jsobjref/TextFrameItem/ · https://helpx.adobe.com/illustrator/kb/optimize-native-pdf-file-sizes.html · https://illustrator-scripting-guide.readthedocs.io/jsobjref/IllustratorSaveOptions.html · https://learn.adobe.com/illustrator/using/find-missing-fonts.html · https://learn.adobe.com/illustrator/using/creating-type-path.html · https://creativepro.com/set-type-circle-square-s-curve-whatever/ · https://creativepro.com/?p=14743 · https://helpx.adobe.com/illustrator/using/line-character-spacing.html
- InDesign/IDML: https://developer.adobe.com/indesign/dom/api/t/Text/ · https://developer.adobe.com/indesign/dom/api/c/CharacterStyle/ · https://developer.adobe.com/indesign/dom/api/t/TextFrame · https://github.com/sillsdev/pathway/blob/master/DistFiles/InDesignFiles/Scripture/Stories/Story_uc9.xml · https://github.com/sillsdev/pathway/blob/master/DistFiles/InDesignFiles/Scripture/Spreads/Spread_ub9.xml · https://community.adobe.com/t5/indesign-discussions/where-is-the-idml-specification/m-p/13170191 · https://helpx.adobe.com/si/indesign/desktop/troubleshoot/font-and-plugin-issues/missing-font-error.html
- Composite fonts and manga lettering: https://cssnite.jp/kobe/vol02/takano/CSSNiteKobe2-takano.pdf · https://www.itmedia.co.jp/news/article/2102/17/1210217084/2 · https://community.adobe.com/質問-225/合成フォント設定でグレーアウトしている項目-421318 · https://forest.watch.impress.co.jp/docs/news/1111761.html
- Photoshop/PSD: https://github.com/Agamnentzar/ag-psd/blob/master/src/text.ts · https://github.com/Agamnentzar/ag-psd/blob/master/src/psd.ts · https://psd-tools.readthedocs.io/en/latest/reference/psd_tools.api.layers.html · https://community.adobe.com/t5/photoshop-ecosystem-discussions/how-to-tell-what-fonts-are-missing-in-a-photosohp-cs5-document/m-p/3473659/highlight/true · https://helpx.adobe.com/fonts/kb/resolve-missing-fonts.html
- Figma: https://developers.figma.com/docs/rest-api/file-node-types/ · https://developers.figma.com/docs/rest-api/file-property-types/ · https://developers.figma.com/docs/plugins/api/properties/TextNode-setrangefontname/ · https://github.com/gridaco/grida/blob/main/.ref/figma/fig.kiwi · https://grida.co/docs/wg/feat-fig/glossary/fig.kiwi · https://help.figma.com/hc/en-us/articles/360039956994
- Sketch: https://github.com/sketch-hq/sketch-file-format · https://www.sketch.com/docs/interface-and-settings/document-settings/fonts/
- Office: https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.bodyproperties?view=openxml-3.0.1 · https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.textverticalvalues?view=openxml-3.0.1 · https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.normalautofit?view=openxml-3.0.1 · https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.runproperties?view=openxml-3.0.1 · https://support.microsoft.com/en-us/office/fonts/benefits-of-embedding-custom-fonts · https://www.microsoft.com/en-us/microsoft-365/blog/2015/07/06/document-font-embedding-demystified/
- Krita: https://krita.org/en/release-notes/krita-5-3-release-notes · https://docs.krita.org/en/reference_manual/tools/text.html · https://docs.krita.org/en/reference_manual/dockers/text_properties.html · https://krita-artists.org/t/i-need-help-in-fonts/15732 · https://krita-artists.org/t/the-text-editor-changes-the-default-font-with-each-krita-update/98109 · https://bugs.kde.org/show_bug.cgi?id=381560
- Inkscape: https://wiki.inkscape.org/wiki/Translations:Release_notes/1.0/134/en
- Clip Studio Paint: https://help.clip-studio.com/en-us/manual_en/480_text/Vertical_text_and_readings.htm · https://help.clip-studio.com/en-us/manual_en/810_subtools/T.htm · https://tips.clip-studio.com/en-us/articles/851

Platforms
- https://developer.android.com/ndk/reference/group/font · https://android.googlesource.com/platform/frameworks/native/+/android-16.0.0_r2/include/android/font.h · https://developer.android.com/develop/ui/views/text-and-emoji/downloadable-fonts
- https://developer.apple.com/videos/play/wwdc2019/227/
- https://developer.chrome.com/docs/capabilities/web-apis/local-fonts · https://developer.mozilla.org/en-US/docs/Web/API/Local_Font_Access_API · https://github.com/mozilla/standards-positions/issues/401

Rust crates (crates.io API, sources downloaded to `dl/src/`)
- https://crates.io/crates/parley · https://github.com/linebender/parley · https://github.com/linebender/parley/issues/208 · https://github.com/linebender/parley/issues/301
- https://github.com/harfbuzz/harfrust · https://github.com/harfbuzz/rustybuzz · https://github.com/googlefonts/fontations (skrifa, read-fonts, skera) · https://github.com/dfrg/swash · https://github.com/pop-os/cosmic-text · https://github.com/unicode-org/icu4x · https://github.com/servo/unicode-bidi · https://github.com/typst/hypher · https://github.com/tapeinosyne/hyphenation · https://github.com/typst/subsetter · https://github.com/yeslogic/allsorts

Repository (read only)
- `docs/reference/capy-package.md`; on `origin/main` commit `726cb6cdc`: `docs/history/vector-layers-research.md` §7.2–7.4, `docs/history/object-content-extensions-research.md` §2.3–4.9, `docs/history/object-content-extensions/01-capy-format-audit.md` §9; `docs/history/vector-layers-research/07-rust-feasibility.md`.
