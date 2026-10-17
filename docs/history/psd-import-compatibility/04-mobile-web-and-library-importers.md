# PSD/PSB import survey: apps, libraries, test corpora (checked 2026-10-16)

[PSD import compatibility](../psd-import-compatibility-research.md) · source report, 2026-10-16

Research for the [PSD import compatibility record](../psd-import-compatibility-research.md), which holds the gap analysis built from it.

**Legend:** **E** editable · **R** raster fallback (Photoshop's stored pixels are used, or the feature is baked into pixels) · **P** kept but read-only or display-only · **D** dropped · **F** flat composite only · **U** unknown · **—** no PSD import. "[inf]" marks my inference and "[unv]" marks an unverified user or forum report. The per-app notes in §2 hold the citations for each matrix row.

## Key findings

- **Few vendors publish what their importers keep.** Most mobile apps document one or two sentences. Krita's source, PhotoCraft's source, and Adobe's community threads are the richest evidence.
- **The most common fallbacks are raster text and dropped layer styles.** Procreate, Clip Studio Paint and Sketchbook rasterize text. Clip Studio Paint and Infinite Painter drop styles, and Procreate did too in a 2016 report. Krita and Affinity offer a choice between editable text and pixel-accurate text.
- **Several apps keep unsupported layers as read-only placeholders instead of dropping them.** Fresco keeps a "Photoshop preview object", Photoshop on iPad keeps smart objects it cannot edit, and PhotoCraft keeps unreadable adjustments verbatim with a warning. GIMP stopped dropping unsupported layers in 2024 because dropping them broke clipping groups.
- **Clipping semantics break most often between apps.** Krita has no true clipping. Its PSD clipping bug is still open (last modified 2026-05) and a community clipping branch appeared in July 2026. Clip Studio Paint, ibisPaint and MediBang use folder clipping, which differs from Photoshop's.
- **The best permissive Rust reference is PhotoCraft (MIT/Apache-2.0).** It has a standalone `photocraft-psd` crate plus a mapping and compositing layer, and a Photoshop-rendered oracle corpus of 258 PSDs. It is also extremely new: the repository was created 2026-09-30 and calls itself early alpha.
- **ag-psd's README limitations list is stale.** Its changelog adds PSB (v12.1.0), 16-bit reading (v20.0.0) and initial 32-bit support (v20.1.0).

## 1. Feature × app matrix

### 1a. Structure

| App | Groups | Pass-through | Clipping | Pixel mask | Vector mask | Blend modes | Fill opacity |
|---|---|---|---|---|---|---|---|
| Procreate | E | U | U | U | R (vector layers rasterized) | E | U |
| Adobe Fresco | E | U | P (2019) | E | U | E | U |
| Photoshop iPad | E | E [inf] | E | E | P/U (none at 2019 launch) | E | U |
| Clip Studio Paint (iPad/Android) | E | E ("Through") | E, folder semantics differ | E [inf] | U | E, "may not be fully reproduced" | U |
| Affinity (iPad) | E | U | E [inf] | E | U | E | U |
| Pixelmator Pro (Mac/iPad) | U | U | U | U | E as shapes [inf] | U | U |
| Krita (Android/ChromeOS = desktop) | E | E | D/approximate | E | E (5.3) | E | U |
| ibisPaint | U | U | U | U | U | U | U |
| MediBang Paint | U | U | U | U | U | U | U |
| Sketchbook | E | U | U | U | U | E ("most") | U |
| Infinite Painter | U | U | U | E (7.x) | D | U | U |
| Concepts | F | F | F | F | F | F | F |
| Artstudio Pro | U | U | U | U | U | U | U |
| Canva (web) | U | U | U | E or R | U | U | U |
| Adobe Express | E (approx.) | U | U/R | U | U/R | Multiply and Screen only | U |
| Figma, Penpot, Lunacy, Kittl, Photoroom, Framer, Linearity Curve | — | — | — | — | — | — | — |
| Sketch | U | U | U | U | U | U | U |

### 1b. Content and colour

| App | Adjustment layers | Layer styles | Shape layers | Text | Smart objects | 16-bit | CMYK |
|---|---|---|---|---|---|---|---|
| Procreate | U ("compatible filters") | D [unv, 2016] | R | R | U | U (8-bit needed [unv]) | U |
| Adobe Fresco | E for 3 types, P otherwise | P ("Photoshop FX") | U | U | U | U (8-bit needed [unv]) | U |
| Photoshop iPad | E | P; Stroke and Drop Shadow E | U | E [inf] | P (not editable) | U ([unv]: no) | U |
| Clip Studio Paint | U | D | U | R | U | U | converted to RGB (PRO/EX) |
| Affinity (iPad) | E for "certain" types | E | U | E (option) or R | E (option) or R | U | U |
| Pixelmator Pro | E for some types | improved, partial | E | E | U | U | U |
| Krita | R/D, except fill layers E | E | E (5.3) | E (asks) or R | U/R [inf] | E | E |
| Infinite Painter | compatible ones kept, others D | D | D | U | U | U | U |
| Sketchbook | U | U | U | R | U | U | U |
| Concepts | F | F | F | F | F | F | F |
| Canva | U | U/R | U | E [inf] | U | U | U |
| Adobe Express | U/R | U/R ("some") | U | E with font substitution | U/R | U | D (RGB only) |
| ibisPaint, MediBang, Artstudio Pro, Sketch | U | U | U | U | U | U | U |

## 2. App notes

**Procreate.** The handbook says PSD import "preserves locked layers, layer groups, compatible filters and layer blend modes" and that it "can also rasterize and import vector and text layers" ([handbook](https://help.procreate.com/procreate/handbook/gallery/gallery-file-types)). Masks, clipping, adjustment layers and smart objects are not addressed. A 2016 forum reply says FX layers "would no longer be there" [unv] ([AppleInsider](https://forums.appleinsider.com/discussion/197482)). PSD export keeps "layers, layer names, opacity, visibility, and blend modes" ([handbook](https://help.procreate.com/procreate/handbook/actions/actions-share)).
- Layer limits depend on "your iPad model's RAM and the pixel dimensions of your canvas" ([help](https://help.procreate.com/articles/YB7CjQ-maximum-layer-limit)). For example, an M1 8 GB iPad holds 58 layers at 4096², 11 at 8192², and 3 at the 16,384×8,192 maximum ([Procreate 2021](https://procreate.com/insight/2021/layer-limits)).
- Canvas caps are 33–134 MP and a 16,384 px edge ([help](https://help.procreate.com/articles/dabqrn-maximum-canvas-size)).
- I found no documentation of what happens when a PSD exceeds the layer limit.
- Pain point [unv]: a user says Procreate and Fresco "require 8-bit files" ([Adobe community, 2023](https://community.adobe.com/t5/photoshop-ecosystem-ideas/8bit-file-preference-trouble/idi-p/13820809)).

**Adobe Fresco.** Most layers can arrive "merged into one layer with a little PS icon". A relayed designer explanation says clipped and adjustment layers are most prone to this, because unsupported layers are grouped into a "Photoshop preview object" that cannot be unpacked ([thread, 2019–2023](https://community.adobe.com/t5/fresco-discussions/opening-photoshop-docs-in-fresco-with-layers/m-p/10632252)).
- Users report that layers with effects stay uneditable until rasterized, and that Fresco "is not friendly with layer styles and color correction layers" (same thread).
- In 2024 Fresco told one user that "some of my layers were unreadable because they contain Photoshop FX". The Adobe staff reply suggested merging the effects in Photoshop ([thread](https://community.adobe.com/t5/fresco-discussions/fresco-turned-my-layers-into-ps-read-only/td-p/14586559)).
- Fresco's Brightness/Contrast, Hue/Saturation and Color Balance adjustments round-trip as Photoshop adjustment layers (search snippet of [helpx](https://helpx.adobe.com/fresco/using/layers.html); the page returned 403).
- A 2021 review says Fresco has groups, layer masks and Photoshop's blend modes ([CG Channel](https://www.cgchannel.com/?p=120473)).

**Photoshop on iPad.** It uses the desktop PSD engine. A reviewer reports accurate opening, clipping to layers and groups working, "bitmap masks only (no vector masks yet)" at the 2019 launch, and no layer cap beyond RAM ([iMore](https://www.imore.com/photoshop-ipad-review)). An Adobe employee says the layer limit equals desktop's ([thread](https://community.adobe.com/t5/photoshop-ecosystem-discussions/loading-and-using-psd-files-on-ipad-pro/m-p/8636889/highlight/true)).
- Stroke and Drop Shadow effects became editable in 4.7 (June 2023) (search snippet of [helpx](https://helpx.adobe.com/photoshop/ipad/work-with-layers/add-manage-edit-layers-ipad.html)).
- "Editing a smart object is not supported on the iPad yet" ([community, 2025, citing helpx](https://community.adobe.com/questions-712/no-smart-objects-in-ipad-why-1179360)).
- [unv] A user says mobile Photoshop "does NOT translate my 16-bit originals" ([2023](https://community.adobe.com/t5/photoshop-ecosystem-ideas/8bit-file-preference-trouble/idi-p/13820809)).

**Clip Studio Paint.** "You will not be able to keep text layers editable when opening .psd/.psb files"; they are rasterized ([official tip](https://tips.clip-studio.com/en-us/articles/6549)).
- CMYK PSDs convert to RGB on open in PRO/EX, and DEBUT cannot open them ([manual](http://www.clip-studio.com/site/gd_en/csp/userguide/csp_userguide/500_menu/500_menu_file_open.htm)).
- Stroke, drop shadow and outer glow "disappeared" on import because "CSP doesn't have drop shadow/outer glow" ([ASK](https://ask.clip-studio.com/en-us/detail?id=43566)).
- Blend modes and colour correction "may not be fully reproduced", and folder clipping may fail ([ASK](https://ask.clip-studio.com/en-us/detail?id=126883)). Folder pass-through combined with clipping has its own rules ([ASK](https://ask.clip-studio.com/en-us/detail?id=122458)).
- PSD/PSB files can also be placed as "file objects" ([manual](https://help.clip-studio.com/en-us/manual_en/180_layers/File_objects.htm)).

**Affinity (iPad; same help as desktop).** Files import "with layers intact", and "certain Adjustment layers … are mapped". Two preferences control fallbacks: "Import PSD Text as Text (not Pixel Layers)" and "Import PSD Smart Objects where Possible"; otherwise these layers are rasterized. Import is "one-way" ([help](https://affinity.help/photo2ipad/en-US.lproj/pages/GetStarted/importAdobe.html)).
- One reviewer's tests found text, styles, masks and blend modes opened fine, and embedded smart objects in 32-bit files stayed editable. A gradient mask produced "Unknown property: EffectGradientFill" ([Beyond Photo Tips](https://www.beyondphototips.com/affinity-photo-adobe-psd-files/)).

**Pixelmator Pro.** Pixelmator 2.1.3 rewrote its PSD engine and "used the Rust programming language" for reading and writing ([archived blog, 2021](https://web.archive.org/web/2023id_/https://www.pixelmator.com/blog/2021/08/19/pixelmator-pro-2-1-3-brings-huge-improvements-to-psd-support/)). That release added:
  - shape layers opened as vector shapes;
  - PSB support;
  - "way better support for effects";
  - Hue/Saturation, Exposure, Invert and Channel Mixer adjustment layers;
  - improved text.

  Pixelmator Pro for iPad shipped 2026-01-28 ([Apple](https://support.apple.com/en-au/126259)). That it shares this engine is [inf]. The legacy Pixelmator for iOS opened layered PSDs ([PetaPixel 2015](https://petapixel.com/2015/05/28/pixelmator-brings-its-mobile-photo-editing-app-to-the-iphone/)).

**Krita, including Android and ChromeOS (GPL-2.0-or-later; behavioural study only).** The Android and ChromeOS builds are the desktop program ([Chrome Unboxed](https://chromeunboxed.com/krita-android-chrome-os-chromebooks-free-painting-professional-desktop-app)).
- Supported: "raster layers, blending modes, layer styles, vector shapes, text layers, layer groups and transparency masks" ([docs](https://docs.krita.org/en/general_concepts/file_formats/file_psd.html)).
- Krita 5.3 (2026) loads shapes, vector masks, guides, and text "including … text on path and open type features" ([release notes](https://krita.org/en/release-notes/krita-5-3-release-notes/)).
- Krita 5.1 added fill layers, but noted that most PSDs pair them with vector masks, which were "not yet implemented" at the time ([notes](https://krita.org/en/release-notes/krita-5-1-release-notes)).
- The loader asks the user: "Found text objects, do you wish to load them as editable text shapes? If not, they will be loaded as pixel data, which will be visually more accurate". It also maps pass-through groups and supports 8/16/32-bit RGB, CMYK, Gray and Lab ([loader](https://github.com/KDE/krita/blob/master/plugins/impex/psd/psd_loader.cpp), [colour modes](https://github.com/KDE/krita/blob/master/libs/psdutils/psd.cpp)).
- The loader has no smart-object, adjustment or clipping handling [inf, from source].
- "Text and adjustment layers will not be carried over" ([forum, 2022](https://krita-artists.org/t/krita-and-smart-objects-and-a-short-other-question/49218)); the text part is now superseded by 5.3.
- The clipping bug is still CONFIRMED and open ([bug 350114](https://bugs.kde.org/show_bug.cgi?id=350114)). A 2022 import-only fix is still a draft ([MR 1561](https://invent.kde.org/graphics/krita/-/merge_requests/1561)). In July 2026 users reported clipping loss round-tripping through PSD, and a test build adds basic clipping ([thread](https://krita-artists.org/t/layer-alpha-inheritance-compatibility-with-psds/186638)).

**ibisPaint.** Only "You can import work you created in Photoshop" is documented ([lesson](https://ibispaint.com/lecture/index.jsp?no=119&lang=en)). The export FAQ documents conversions:
  - Add → Linear Dodge;
  - vector layers rasterized;
  - "Photoshop does not support folder clipping … the clipping will be removed".

  ([FAQ](https://ssl.ibis.ne.jp/faq/11/ibisPaintforiPad/q/153/?lang=en)). This implies its own folder clipping has no PSD equivalent [inf].

**MediBang Paint.** I found no official statement on PSD import. Saving to PSD rasterizes objects (search snippet of the [MediBang manual](https://medibangpaint.com/en/medibang-pro/manual/layer/masking/)).

**Sketchbook.** PSD import and export work on all platforms. "Layers, layer groups, layer names, and most blend modes will be preserved", "Text layers will be rasterized", Sketchbook-only modes revert to Normal, and the background colour layer is dropped ([help](https://help.sketchbook.com/docs/import-from-photoshop)). Groups default to pass-through ([help](https://help.sketchbook.com/docs/grouping-layers)). Layer count is bounded by device memory and canvases by 100 MP ([help](https://help.sketchbook.com/docs/max-layers-on-mobile)).

**Infinite Painter.** It reads "only the regular RGB bitmap layers (ignoring vector paths, 3D and animation data, layer effects and incompatible adjustment layers)". A PSD always opens as a new project and cannot be imported as a layer ([docs](https://docs.infinitestudio.art/painter/data/import-export/)). There is no fixed layer limit; it depends on RAM ([docs](https://docs.infinitestudio.art/painter/technical/specifications/)). A 7.x App Store note mentions PSD layer-mask support [unv, Portuguese listing] ([App Store](https://apps.apple.com/pt/app/infinite-painter/id1146543227)).

**Concepts.** PSDs import "as flat PNG images … It is not possible to import individual layers", via Apple's iOS decoding ([tutorial](https://concepts.app/en/tutorials/psd-import-and-export-concepts-58/)).

**Artstudio Pro.** PSD is listed as an import format, and the app has groups, masks, clipping, adjustment layers and nine layer effects ([App Store](https://apps.apple.com/app/id1244142051)). How PSD features map onto these is undocumented.

**Web and design tools**
- **Canva:** PSDs up to 300 MB and 100 fonts. "Layers, gradients and masks are supported, but some elements might be rasterised." Canva describes the feature as beta and desktop-browser only ([help](https://www.canva.com/en_au/help/ai-import/), search snippet).
- **Adobe Express** (search snippet of [helpx](https://helpx.adobe.com/uk/express/using/best-practices-photoshop-illustrator-files.html)):
  - 500 MB limit; canvas capped at 8000 px, larger files scaled down;
  - RGB only, no CMYK;
  - Multiply and Screen are the only blend modes;
  - text styling is approximated and missing fonts are substituted.

  An Adobe community manager (2026-07) says "adjustment layers, clipping masks, Smart Objects … vector masks, 3D layers, and some layer effects may not import as expected" ([thread](https://community.adobe.com/questions-329/no-puedo-importar-de-psd-a-adobe-express-1561967)).
- **Figma:** no native import; the supported import types exclude PSD ([help](https://help.figma.com/hc/en-us/articles/360041003114-Import-files-to-the-file-browser)). Third-party plugins fill the gap (Convertify, psd.to.design, Codia), and no staff response exists in the feature thread ([forum](https://forum.figma.com/suggest-a-feature-11/ability-to-import-photoshop-files-into-figma-bonus-they-update-dynamically-27005)).
- **Penpot:** imports from other tools are on the roadmap, and SVG is the route ([forum](https://community.penpot.app/t/feature-request-ability-to-import-sketch-adobe-xd-or-figma-files-into-penpot/418)).
- **Lunacy:** the import list omits PSD ([about](https://docs.icons8.com/about/)), and PSD import is a forum request ([forum](https://lunatics.icons8.com/discussion/comment/522)).
- **Photoroom:** the formats table omits PSD ([help](https://help.photoroom.com/en/articles/14342892-supported-file-formats)).
- **Linearity Curve:** its format table omits PSD ([blog](https://linearity.io/blog/import-ai-files/)).
- **Framer:** it imports via Figma ([Framer](https://www.framer.com/figma-to-html/)). The old Framer Generator imported PSD assets ([StackShare](https://stackshare.io/companies/framer)).
- **Kittl and Sketch:** I found no documentation either way.
- **Photopea API:** free to embed through a URL with a JSON config and an iframe. It can POST saved PSD/PNG back to a server, and white-label use needs a Distributor account ([API](https://www.photopea.com/api/)). It is proprietary but could serve as a behavioural oracle [inf].

## 3. Libraries

| Library | Licence | Status | Structure and tagged blocks | Text (EngineData) | Effects | Vector masks and origination | Smart objects | Rendering | Depth, colour, PSB |
|---|---|---|---|---|---|---|---|---|---|
| [psd-tools](https://github.com/psd-tools/psd-tools) (Python) | MIT | 1.25.0, released 2026-10-14 ([PyPI](https://pypi.org/project/psd-tools/)) | full low-level read/write | `layer.text`, `typesetting`; [engine_data](https://psd-tools.readthedocs.io/en/latest/reference/psd_tools.psd.engine_data.html) parser | typed read ([effects API](https://psd-tools.readthedocs.io/en/latest/reference/psd_tools.api.effects.html)); "many" not composited | read; limited compositing | embedded/linked data via `layer.smart_object` | basic layers, fill effects, 7 adjustment types, all blend modes but dissolve; no font rendering ([usage](https://psd-tools.readthedocs.io/en/latest/usage.html)) | PSD/PSB; CMYK/Lab via ICC |
| [ag-psd](https://github.com/Agamnentzar/ag-psd) (TS) | MIT | 31.0.3, released 2026-10-07 ([npm](https://registry.npmjs.org/ag-psd)) | read/write, typed descriptors | text plus `engineData`; incomplete; no paragraph/character styles | read/write, including multiple instances | `vectorMask`, `vectorFill`, `vectorStroke`; origination partial ([doc](https://github.com/Agamnentzar/ag-psd/blob/master/README_PSD.md)) | `placedLayer`, `linkedFiles`, some smart filters | none: "does NOT generate new composite" | README says no 16-bit, PSB or CMYK, but the [changelog](https://github.com/Agamnentzar/ag-psd/blob/master/CHANGELOG.md) adds PSB (v12.1), 16-bit read (v20.0) and 32-bit (v20.1); 2 GB default memory cap (v31) |
| [@webtoon/psd](https://github.com/webtoon/psd) (TS) | MIT | 0.4.0, 2023-06; last commit 2023-07 | pixels, guides, slices, PSB | string values only | work in progress | — | — | merged image needs Maximize Compatibility; `layer.composite()` applies opacity only | PSB |
| [psd.js](https://github.com/meltingice/psd.js) (CoffeeScript) | MIT | npm 3.4.0, 2021-11 (dormant) | many blocks unparsed | via psd-enginedata | — | vector mask data | — | none | partial |
| [photocraft-psd](https://github.com/storytold/photocraft/tree/main/crates/psd) (Rust) | MIT/Apache-2.0 ([repo](https://github.com/storytold/photocraft)) | repo created 2026-09-30, early alpha | typed `luni lsct lsdk lyid clbl infx knko iOpa …`; unknown blocks kept raw; byte-exact round trip; no-panic fuzzing; size caps | kept raw in the crate | kept raw in the crate | kept raw in the crate | kept raw in the crate | crate has none ([README](https://github.com/storytold/photocraft/blob/main/crates/psd/README.md)) | 1/8/16/32-bit, all 8 modes, PSB |
| PhotoCraft io and compose layers (Rust) | MIT/Apache-2.0 | same repo | [psd_import.rs](https://github.com/storytold/photocraft/blob/main/crates/io/src/psd_import.rs) maps to a document model and "never fails: problems become warnings" | typed via photocraft-text; Photoshop's pixels kept as cache | effects mapping plus renderer | vector map | smart filters (`SoLd`/`PlLd`) | effects, 20 adjustment keys, blend modes; claims a Photoshop-matched render | CMYK and Lab native |
| [psd](https://github.com/chinedufn/psd) crate (Rust) | MIT/Apache-2.0 | 0.3.5, 2024-01; last commit 2025-01 ([crates.io](https://crates.io/crates/psd)) | layers, groups, blend-mode enum, clipping flag | — | — | — | — | RGBA flatten | header accepts version 1 only, so no PSB ([source](https://github.com/chinedufn/psd/blob/master/src/sections/file_header_section.rs)) |
| [rawpsd](https://github.com/wareya/rawpsd-rs) (Rust) | CC0 | 0.2.2, 2025-05 | layer records, groups | — | — | — | — | — | 8-bit RGB/CMYK/Gray; no PSB; CS6 target |
| [psd_sdk](https://github.com/MolecularMatters/psd_sdk) (C++) | BSD-2 | pushed 2026-08 | groups, masks including vector masks, alpha channels | — | — | vector masks | read | — | 8/16/32-bit, Gray/RGB |
| [PhotoshopAPI](https://github.com/EmilDohne/PhotoshopAPI) (C++/Python) | BSD-3 | v0.9.1, 2026-05 | read/write | editable text layers | — | planned | replace, warp, extract | none; written files lack a valid merged image | 8/16/32-bit, RGB/CMYK/Gray, PSB |
| [libpsd](https://sourceforge.net/projects/libpsd/) (C) | GPLv2 (study only) | last update 2015 | "decodes and blends"; CS and earlier | U | U | U | U | blending | U |
| ImageMagick [psd.c](https://github.com/ImageMagick/ImageMagick/blob/main/coders/psd.c) | [ImageMagick licence](https://imagemagick.org/license/) (Apache-style) | active | layers as frames; blend → composite op; masks as opacity; extra blocks kept as raw `psd:additional-info`; no group or clipping use [inf from source] | — | — | — | — | flatten approximates | PSD/PSB |
| GraphicsMagick | MIT-style | "PSD format is no longer supported since the 1.3.24 release" ([formats](http://www.graphicsmagick.org/formats.html)) | — | — | — | — | — | — | — |
| OpenImageIO [psdinput](https://github.com/AcademySoftwareFoundation/OpenImageIO/blob/main/src/psd.imageio/psdinput.cpp) | BSD-3/Apache-2.0 | active | subimage 0 is the merged image; layers are raw subimages | — | — | — | — | none | RGB/CMYK/gray/indexed/multichannel; no Lab or duotone ([docs](https://openimageio.readthedocs.io/en/latest/builtinplugins.html)) |
| Aseprite [psd_format.cpp](https://github.com/aseprite/aseprite/blob/main/src/app/file/psd_format.cpp) and [aseprite/psd](https://github.com/aseprite/psd) | app under EULA; library MIT, "experimental" | active | groups, opacity, slices; unmapped blend modes fall back to Normal | — | — | — | — | — | U |
| Blender | GPL | — | loads the merged composite through OIIO; needs Maximum Compatibility ([commit](https://developer.blender.org/rB4abb8fde95260e692b3cd5b5b2f8bcd1d1f3be3b)) | — | — | — | — | F | U |
| Krita, GIMP | GPL (study only) | active | see §2. GIMP keeps unsupported layers and reports them in an "unsupported features dialog" ([commit 2024](https://github.com/GNOME/gimp/commit/f8b8ffa10e03e8a0432f287cb8d1ca017e719407)) | | | | | | |

ag-psd notes that "Vector, text and smart object layers still have image data with pregenerated bitmap" ([doc](https://github.com/Agamnentzar/ag-psd/blob/master/README_PSD.md)). That stored bitmap is the universal raster fallback. ag-psd also warns that canvases can reach 300,000² and recommends decoding one layer at a time ([README](https://github.com/Agamnentzar/ag-psd)).

## 4. Test corpora

- **[photocraft-corpus](https://github.com/storytold/photocraft-corpus)** (MIT OR Apache-2.0): 258 PSDs made by scripts in Photoshop. Each file stores Photoshop's own render, and files are pinned by sha256. This is the best permissively licensed oracle set found.
- **[psd-tools tests](https://github.com/psd-tools/psd-tools/tree/main/tests/psd_files)** (repo MIT; no separate licence for the files [unv]). About 142 entries, including:
  - 32 blend modes and pass-through variants;
  - clipping and fill opacity;
  - 79 adjustment files with PNG oracles;
  - effects, smart objects, text, CMYK, 16/32-bit and PSB;
  - a `group-clipping` case with separate Clip Studio Paint and Photoshop renders.
- **[ag-psd tests](https://github.com/Agamnentzar/ag-psd/tree/master/test)** (MIT): 73 read cases with expected PNG and JSON output, including `pass-through`, `fill-opacity`, `blend-if`, `smart-filter`, `cmyk`, `16bits` and a Paint Tool SAI export.
- PhotoCraft also runs the psd-tools and ag-psd sets and reports that 307 of 309 files render the same after re-saving ([README](https://github.com/storytold/photocraft)).
- The Krita and GIMP test data are GPL; use them for behaviour comparison only [inf].

## 5. Patterns relevant to Capy (recommendations, inferred)

1. **Keep rather than drop.** Unsupported layers should become read-only items that keep Photoshop's stored pixels and their raw blocks, as Fresco, Photoshop on iPad, GIMP and PhotoCraft do. Dropping layers breaks clipping chains.
2. **Text and smart objects need a choice.** Offer editable versus pixel-accurate, with pixel-accurate safer for missing fonts. Krita and Affinity both offer this choice.
3. **Report losses once, per layer, in plain words.** Examples include Fresco's "unreadable because they contain Photoshop FX", PhotoCraft's per-layer warnings, and GIMP's dialog.
4. **Never rely on the merged image.** It is absent unless the file was saved with Maximize Compatibility (@webtoon/psd, psd.js, Blender).
5. **Bound memory before decoding.** Check dimensions, layer count and total decoded bytes first (ag-psd guidance, photocraft-psd caps). Mobile apps' RAM-scaled layer limits show the same constraint.
6. **Map clipping explicitly and test it against oracles.** Photoshop clips to the layer or group directly below. Krita's inherit-alpha and the folder clipping in Clip Studio Paint and ibisPaint behave differently.
7. **Dependency choice.** photocraft-psd is the only permissive Rust crate that covers PSB, 16/32-bit, CMYK and raw-block retention. Weigh its age (weeks) and alpha status before depending on it. The `psd` and `rawpsd` crates are too narrow. For semantics, use psd-tools and ag-psd (MIT) together with the PhotoCraft mapping code.

## 6. Not verified

- ibisPaint, MediBang and Artstudio Pro import behaviour beyond "it opens".
- Procreate's handling of masks, clipping, adjustment layers and files over the layer limit.
- Canva and Adobe Express help pages: both returned 403, so those claims rely on search snippets.
- Sketch and Kittl: I found no documentation either way.
- Pixelmator for iPad sharing the Mac PSD engine.
- Current Fresco clipping behaviour; the evidence is from 2019.
- No current tablet test was run. Every row comes from documentation, source code or user reports.
