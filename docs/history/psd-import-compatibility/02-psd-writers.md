# What PSDs from other paint and vector programs contain

[PSD import compatibility](../psd-import-compatibility-research.md) · source report, 2026-10-16

Research for the [PSD import compatibility record](../psd-import-compatibility-research.md), which holds the gap analysis built from it.

Notation: [n] refers to the numbered sources at the end. **[inferred]** is my own deduction. **[unverified]** means no primary source confirms it. adobe.com help pages block automated fetches, so I read their Wayback snapshots from 2025–2026. The Krita and GIMP findings come from reading their current exporter source code.

## Problems an importer will meet with any of these writers

- **The composite image can't be trusted.** Photoshop writes a real merged image only when "Maximize Compatibility" is on [80]. Resource 1057 records `hasRealMergedData` and the writer name [79]. One developer wrote a PSD whose composite was wrong, and it still opened fine in SAI and Krita [81], so a writer's bad composite can go unnoticed. When layers are present, rebuild the image from the layers. [inferred]
- **The `tsly` flag (Transparency Shapes Layer) matters.** When it is 0, layer transparency acts like fill opacity inside the blend [79]. SAI's normal modes behave like Photoshop modes with this flag off [67]. A reverse-engineered converter that aims to match CSP's own PSD output byte for byte writes CSP's Glow Dodge and Add (Glow) as `div `/`lddg` with `tsly=0` [35]. One user reports that SAI PSDs look "faded" in Photoshop until the flag is turned off by hand [68].
- **Pass-through groups.** The pass-through mode is stored in the group's `lsct` section divider, while the group's own layer record says `norm` [64]. Sketchbook and Infinite Painter make new groups pass-through by default [60][77].
- **Clipping on groups.** CSP lets a folder clip to the layer below. psd-tools hit infinite recursion on such a file, and its maintainer found Photoshop does not render group clipping [37][41].
- **Blend modes Photoshop doesn't have become Normal, silently.** Krita [45], GIMP [64], Sketchbook [59] and Painter (GelCover) [73] all do this. The original look can't be recovered from the file.
- **Parse defensively.** Known cases:
  - Procreate PSDs with a broken "global layer mask info" block [26][27].
  - CSP Levels adjustment layers with undocumented trailing data [38].
  - Mask blocks whose layout can't be told apart by length alone (seen in a CSP file) [39].
  - Old-style layer names in Shift-JIS; prefer the `luni` Unicode name [40]. CSP writes `luni` without the trailing nulls [35].

## 1. Adobe Illustrator (Export As > Photoshop)

**Options [1]:**
- Color Model: RGB, CMYK or Grayscale. If it differs from the document's color mode, only Flat Image is allowed.
- Resolution.
- Flat Image or Write Layers.
- Preserve Text Editability.
- Maximum Editability.
- Anti-aliasing (supersampling).
- Embed ICC Profiles.
- Use Artboards (one PSD per artboard) or All / Range.

Older tutorials say Write Layers only works in CMYK or Grayscale. That is probably the color-model rule above, seen in CMYK documents. [inferred]

| Illustrator content | What it becomes in the PSD |
|---|---|
| Layers and sublayers | With Maximum Editability, top-level layers become Photoshop groups. Groups, compound shapes, nested layers and slices become separate layers. Anything nested more than 5 levels deep is merged into one layer [1]. |
| Compound shapes | Become shape layers only if they sit in a top-level layer and the result looks the same. Solid strokes need Round joins [1]. Experts advise turning each path into its own compound shape [5]. |
| Simple paths, live shapes | Rasterized at the export resolution, keeping their look [6]. Live-shape parameters are lost. [inferred] |
| Gradients | Become pixels. Users change them to flat colors to keep paths. Gradients across a stroke blocked layered export entirely [4]. |
| Multiple fills/strokes, effects | Rasterized. An effect on type forces the type to rasterize [8]. One small effect on an image mask flattened a whole layer [4]. |
| Clipping masks | A frequent cause of flattened output [4]. The clip path does not survive as a vector [7]. |
| Opacity < 100%, opacity masks, blend modes | Stay live, editable layers with Maximum Editability [1]. How opacity masks are encoded is [unverified]. |
| Text | Horizontal and vertical point type becomes editable type [1]. Text with appearance attributes or effects is rasterized [6][8]. Area type and type on a path: likely rasterized [unverified]. |
| Symbols, placed images | Expanded or rasterized into pixel layers [inferred]. |

**Better routes for vector art.** An .ai saved with "Create PDF Compatible File" contains a full PDF copy of the document [2]. PDF export can also turn top-level layers into Acrobat layers [3]. SVG saves only the active artboard [2] and supports few blend modes; those modes are lost when the SVG is reopened [9]. Reading the PDF content would keep paths, gradients, transparency, soft masks and clipping. Effects, appearance stacks and symbols arrive already expanded. [inferred] PDF looks like the better source than PSD for Illustrator art. [inferred]

## 2. Adobe Fresco

**Exports [10]:**
- Images: PNG, JPG, PSD, PDF.
- Motion: GIF, MP4, PNG sequence.
- Timelapse: MP4.
- Send to Illustrator.

Fresco artwork is a cloud document that Photoshop opens directly [12]. I could not confirm a separate ".fresco" file format. [unverified]

- **Pixel and live layers.** Live brushes (watercolor, oil) are pixel-based physics simulations [14]. In a PSD they become plain pixels, and wetness and paint thickness are lost. [inferred]
- **Vector layers.** Adobe doesn't document how they appear in a PSD. User reports from 2020 conflict: one saw Smart Objects [16], another saw rasterized layers [17]. In 2025, users reported vector layers landing at the wrong scale and position in exported PSDs [18]. This needs test files.
- **Adjustment layers.** Brightness/Contrast, Hue/Saturation and Color Balance "match Photoshop adjustment layers" and save correctly. They can be clipped or masked [13].
- **Blend modes.** Fresco has "all the blend modes that you get in Photoshop" [13].
- **Groups, layer masks, clipping masks.** Fresco has all three [13]. Their PSD encoding is [unverified], but likely the native Photoshop equivalents [inferred].
- **Text.** Fresco has horizontal and vertical type tools [15]. Whether text stays editable in a PSD is [unverified].
- **Reference layers.** Fresco has them [13]. PSD has no equivalent, so they are lost. [inferred]
- **Send to Illustrator.** Illustrator keeps the vector and pixel layers. Merging the two kinds converts vectors to pixels [11]. Vector strokes arrive as filled compound shapes, not stroked centerlines [19]. Since Illustrator 26.0.1, masks and clipping open natively only if every layer involved is a vector layer with Normal blending [20].
- **PDF route.** Layers arrive as sub-layers or clip groups made of editable shapes [21][17].

## 3. Procreate (and Procreate Dreams)

- **Officially preserved.** PSD export keeps "layers, layer names, opacity, visibility, and blend modes" [22].
- **Other formats.** .procreate keeps everything; TIFF and PNG are flattened; "Share Layers" writes one PDF page or one PNG per layer [22].
- **Masks, clipping, groups.** None of these are documented for PSD. The Drawpile developer says Procreate, CSP and PSD all use clipping groups [28], which suggests Procreate writes PSD clipping. [inferred]
- **Text.** An Adobe expert saw "no evidence of editable text" [25], so it is probably rasterized. [unverified]
- **Procreate-only blend mode.** "Shade" is the old Darken, introduced in 5.4 [23]. How it maps in PSD is [unverified].
- **Reference layers.** Lost, since PSD has no equivalent. [inferred]
- **Parser quirk.** Procreate PSDs with a malformed global layer mask block [26][27].
- **CMYK canvases** reportedly open in Photoshop as RGB [30, unverified: the site was unreachable].
- **Procreate Dreams** has no PSD export. It writes video, image sequences and .drm project files [29].

## 4. Clip Studio Paint

- **Commands [31][32]:**
  - Save / Save As to .psd: text is rasterized [33].
  - Save Duplicate .psd/.psb: keeps layers.
  - Export (Single Layer): merges everything into one layer, with an "Output as Background" option.
- **Save Duplicate dialog [31]:**
  - Preset.
  - Output image: drafts, crop marks, inner border, text, story information, folio, camera paths, safety margins.
  - Expression color: Grayscale, RGB or CMYK (no CMYK in DEBUT).
  - Advanced color settings (the tone export settings are not available here).
  - Embed ICC profile.
- **Batch export** (EX edition) needs "Combine and export image" unchecked to keep text editable [33].
- **Text.** The "Text layer" setting offers three choices [33][34]:
  - Image only: rasterized.
  - Text only: editable type.
  - Image & text: a raster layer plus a hidden text layer.

  Font, color and size survive. Outlines and bold become pixels. Balloons split into an image plus text, and multiple text frames become a folder [33].
- **Vector and solid/gradient fill layers** are written as plain pixel layers [35].
- **Tone, frame borders, 3D, paper layers.** The CSP-to-PSD converter [35] supports none of these. CSP probably writes them as rendered pixels. [inferred]
- **Correction layers.** CSP writes Levels as an adjustment layer, with buggy trailing data [38]. The converter's README says CSP rasterizes or drops correction layers [35], which conflicts with that. Gradient map, posterize, binarize and color balance are [unverified].
- **Blend modes.** Beyond Photoshop's set, CSP has Glow Dodge, Add (Glow), Subtract, Divide and Brightness [36]. The converter's mapping [35]:
  - Glow Dodge → `div `, Add (Glow) → `lddg`, both with `tsly=0`.
  - Subtract → `fsub`, Divide → `fdiv`, Brightness → `lum `, folder "Through" → `pass`.
- **Clipping on folders** is supported in CSP but not by Photoshop [37][41].
- **Layer masks** can carry density/feather parameters [39].
- **Bit depth** is likely 8-bit only. [inferred]

## 5. Krita

The Krita docs list "raster layers, blending modes, layer styles, vector shapes, text layers, layer groups, and transparency masks" as supported [42]. The source code gives the details:

- **Color.** RGB, Gray, CMYK and Lab at 8 or 16 bit [43]. Float images are converted to 16-bit RGB. The ICC profile and guides are always written, along with a composite image [46].
- **Groups** become PSD groups, including pass-through [44].
- **Vector layers.** A vector layer with several shapes becomes a group with one layer per shape [44]. Path shapes become shape layers: vector mask, solid/gradient/pattern fill and stroke. Simple rectangles and ellipses also get live-shape (`vogk`) data [44].
- **Text** is written as type, but only basic features. Text on a path and OpenType features are dropped [43].
- **Fill layers.** Color, gradient and pattern fills become PSD fill layers. Other generators are rasterized [44].
- **File, clone and filter layers** are rasterized from their rendered result [44].
- **Masks.** Only a layer with exactly one transparency mask gets a PSD layer mask [44]. Filter, transform and multiple masks are baked into the pixels. [inferred]
- **Clipping** is always written as 0 [44]. Alpha inheritance is lost; this is a long-standing wishlist bug [47][28].
- **Blend modes** Photoshop lacks become Normal [45].

## 6. Affinity (V1 and V2 help; V3 not verified)

- **PSD options [48]:**
  - Compatibility mode.
  - Smallest file sizes.
  - Rasterize all layers (keeps the layer structure).
  - Per-feature choice of Preserve accuracy (rasterize) or Preserve editability, for Gradients, Layer effects, Lines, Adjustments and Adv Blending.
- **General Rasterize setting:** Nothing, Everything or Unsupported properties. With **Nothing, unsupported elements are left out of the file** [48].
- **ICC.** The export ICC profile can be chosen and embedded [48].
- **Text** is rasterized on export [49][50].
- **Vector shapes.** Simple solid-fill shapes become shape layers. Gradients or transparency become pixels or masks, and dashed strokes are a problem [51].
- **Live filters** aren't supported by PSD [53].
- **Hidden layers** reportedly export as empty, transparent layers [52].

## 7. Other PSD writers

- **MediBang Paint.** PSD keeps text and layers but "not features specific to MediBang Paint" [54]. A canvas background color becomes an automatic "Background" layer [55]. The 1-bit, 8-bit and halftone layers are [unverified].
- **ibisPaint.** Two choices: "PSD (Preserved Layers)" and "PSD (Flattened Layer)". The flattened one is recommended for print to avoid blend-mode color shifts [57]. Details are [unverified].
- **Sketchbook (formerly Autodesk SketchBook).**
  - Keeps layers, groups, names and most blend modes [58].
  - Text is rasterized; the background color reverts to white [58].
  - Glow, Soft Glow, Alpha Mask and Inverse Alpha Mask become Normal [58][59].
  - Groups default to pass-through [60].
- **Photopea** stores documents as PSD internally and saves the full structure [61]. It probably writes the richest files of this group. [inferred]
- **Pixelmator Pro.** Since 2.1.3 its PSD engine reads and writes shapes as shapes. It has an option to optimize PSDs for Final Cut Pro and Motion [62]. Text and effects on export are [unverified].
- **GIMP 3.** From the docs [63] and source [64]:
  - 8/16/32-bit; indexed and duotone; CMYK when a soft-proof profile is set; optional ICC profile.
  - Groups including pass-through; layer masks; paths and clipping path.
  - The PSD clipping flag is set from GIMP's "Clip to Backdrop" composite mode.
  - No text writer, so text becomes pixels. [inferred]
  - Unmapped modes become Normal [64].
- **PaintTool SAI 2.**
  - An unofficial guide says PSD keeps raster layers, folders, opacity, blend modes and clipping, and rasterizes text, shape and linework layers [65].
  - Shine and Shade match Photoshop's Linear Dodge and Linear Burn; Shine/Shade matches Linear Light [66][69].
  - The [TS] modes match Photoshop's defaults [67].
  - See the `tsly` note at the top.
- **Rebelle.** Layers are saved to PSD, but PSD keeps "only color and transparency information"; only .reb holds everything [70].
- **ArtRage.** "Basic" PSD export with layers and blend modes [71]. Paint depth and wetness are lost [72, forum page not re-checked].
- **Corel Painter.**
  - Every layer becomes a standard transparent layer, and **groups are not kept** [73].
  - Composite methods are mapped by a table: Gel→Darken, Colorize→Color, Magic Combine→Lighten, Reverse-Out→Normal, Pseudocolor→Normal; GelCover is not converted [73].
- **Concepts.** Each layer is written as a PNG and bundled into a PSD, so it is raster only [74]. Vectors are available through PDF or SVG [75].
- **Infinite Painter.** PSD can include adjustment layers and masks. Paper textures don't export [76]. Groups default to pass-through [77].
- **Tayasui Sketches.** The App Store listing claims layered PSD export, with no other details [78].

## Summary matrix

Cells are: ✓ = native PSD feature; R = rasterized; N = mapped to Normal; ✗ = lost; ? = not verified.

| Writer | Groups / pass-through | Masks | Clipping | Text | Vector | Adjustments | Main losses |
|---|---|---|---|---|---|---|---|
| Illustrator | ✓ / ? | opacity masks live (encoding ?) | R | point type ✓, rest R | compound shapes ✓, rest R | — | gradients, effects, appearance, clip paths |
| Fresco | ✓? | ✓? | ✓? | ? | ? (Smart Object or R) | 3 types ✓ | live-paint state, reference layers |
| Procreate | ? | ? | ✓? [inferred] | R? | — | — | Shade mode?, reference layers |
| Clip Studio | ✓ / ✓ | ✓ | ✓ (incl. folders) | option | R | Levels ✓, rest ? | tones, frame borders, glow modes need `tsly` |
| Krita | ✓ / ✓ | one transparency mask | ✗ | basic ✓ | shape layers ✓ | R | alpha inheritance, filter/clone/file layers, extra modes |
| Affinity | ✓ | ✓? | ? | R | simple solid ✓ | option | live filters, hidden layers; with Rasterize "Nothing", unsupported content is dropped |
| MediBang | ✓? | ? | ? | ? | — | — | app-specific features |
| ibisPaint | ? | ? | ? | ? | — | ? | ? |
| Sketchbook | ✓ / ✓ | — | — | R | — | — | 4 modes → N, background color |
| Photopea | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | little [inferred] |
| Pixelmator Pro | ✓? | ✓? | ? | ? | ✓ | some | ? |
| GIMP 3 | ✓ / ✓ | ✓ | from Clip to Backdrop | R [inferred] | paths only | — | unmapped modes → N |
| SAI 2 | ✓ | ? | ✓ | R | R | — | needs `tsly` handling |
| Rebelle / ArtRage | layers only | — | — | — | — | — | paint simulation, depth |
| Painter | ✗ (flattened) | ? | ? | ? | ? | — | groups, GelCover |
| Concepts | layers | — | — | R | R | — | all vectors |
| Infinite Painter | ✓ / ✓ | ✓ | ? | ? | — | ✓ | paper textures |

Bit depth and color: Krita writes 8/16 bit (float becomes 16) with ICC; GIMP writes 8/16/32 bit; Illustrator, Clip Studio and GIMP can write CMYK; Illustrator, Clip Studio, Affinity and GIMP embed ICC optionally. All other writers are [unverified] on bit depth, CMYK and ICC.

## Sources

[1] https://helpx.adobe.com/illustrator/using/exporting-artwork.html
[2] https://helpx.adobe.com/illustrator/using/saving-artwork.html
[3] https://helpx.adobe.com/illustrator/using/pdf-options.html
[4] https://community.adobe.com/t5/illustrator-discussions/why-is-illustrator-flattening-my-export-to-photoshop/td-p/3413353/page/2
[5] https://community.adobe.com/questions-652/import-vector-shape-layers-from-illustrator-to-photoshop-775649
[6] https://community.adobe.com/questions-652/preserving-vector-shapes-from-illustrator-when-going-to-photoshop-773916
[7] https://community.adobe.com/t5/illustrator-discussions/transfer-illustrator-clipping-masks-to-photoshop-with-an-image/m-p/8950961/highlight/true
[8] https://smallbusiness.chron.com/illustrator-text-pixelated-photoshop-65421.html
[9] https://community.adobe.com/t5/illustrator-discussions/why-does-exporting-svgs-change-the-transparency-layer-s-to-normal/m-p/10438406
[10] https://helpx.adobe.com/fresco/using/publish-export-share.html
[11] https://helpx.adobe.com/fresco/using/fresco-and-illustrator.html
[12] https://helpx.adobe.com/fresco/using/fresco-and-photoshop.html
[13] https://helpx.adobe.com/fresco/using/layers.html
[14] https://helpx.adobe.com/fresco/using/live-brushes.html
[15] https://helpx.adobe.com/fresco/desktop/draw-paint-animate-and-share/create-text-font.html
[16] https://community.adobe.com/t5/fresco-discussions/export-and-import-from-fresco/td-p/10996351
[17] https://community.adobe.com/questions-646/suggestion-for-improvement-export-pdf-with-layers-306360
[18] https://community.adobe.com/questions-646/problem-with-vector-layers-in-fresco-and-importing-them-to-photoshop-in-vectors-305930
[19] https://community.adobe.com/questions-646/vector-lines-turn-to-filled-compound-paths-from-fresco-to-illustrator-307398
[20] https://community.adobe.com/t5/fresco-discussions/fresco-imports-vector-layers-as-merged-pixel-background-image/m-p/12502387/highlight/true
[21] https://community.adobe.com/questions-646/vectors-fresco-pdf-open-in-ai-creative-cloud-layers-vectors-not-preserved-306511
[22] https://help.procreate.com/procreate/handbook/actions/actions-share
[23] https://help.procreate.com/procreate/handbook/layers/layers-blend
[25] https://community.adobe.com/questions-671/change-text-font-of-an-imported-procreate-file-876707
[26] https://github.com/psd-tools/psd-tools/issues/226
[27] https://github.com/psd-tools/psd-tools/issues/177
[28] https://krita-artists.org/t/layer-alpha-inheritance-compatibility-with-psds/186638
[29] https://help.procreate.com/articles/deaqnd-export-formats
[30] https://forum.svslearn.com/post/105387 (unreachable)
[31] https://help.clip-studio.com/en-us/manual_en/210_file/Save_file.htm
[32] https://help.clip-studio.com/en-us/manual_en/210_file/Exporting_files.htm
[33] https://tips.clip-studio.com/en-us/articles/6549
[34] https://support.clip-studio.com/en-us/faq/articles/20220034
[35] https://github.com/dobrokot/clip_to_psd (README and clip_to_psd.py)
[36] https://help.clip-studio.com/en-us/manual_en/180_layers/Blending_modes.htm
[37] https://github.com/psd-tools/psd-tools/issues/325
[38] https://github.com/psd-tools/psd-tools/issues/466 and https://github.com/psd-tools/psd-tools/pull/470
[39] https://github.com/psd-tools/psd-tools/issues/693
[40] https://github.com/psd-tools/psd-tools/issues/137
[41] https://esotericsoftware.com/forum/d/13783
[42] https://docs.krita.org/en/general_concepts/file_formats/file_psd.html
[43] https://invent.kde.org/graphics/krita/-/blob/master/plugins/impex/psd/psd_export.cc
[44] https://invent.kde.org/graphics/krita/-/blob/master/libs/psd/psd_layer_section.cpp
[45] https://invent.kde.org/graphics/krita/-/blob/master/libs/psdutils/psd.cpp
[46] https://invent.kde.org/graphics/krita/-/blob/master/plugins/impex/psd/psd_saver.cpp
[47] https://bugs.kde.org/show_bug.cgi?id=363946
[48] https://affinity.help/photo/en-US.lproj/pages/ExportPersona/exportSettings.html
[49] https://affinity.help/photo2/en-US.lproj/pages/Appendix/fileformat.html
[50] https://affinity.help/designer2/en-US.lproj/pages/Appendix/fileformat.html
[51] https://forum.affinity.serif.com/index.php?/topic/23471-how-do-i-get-a-vector-shape-into-ps/
[52] https://forum.reallusion.com/Topic435227.aspx
[53] https://forum.reallusion.com/PrintTopic384050.aspx
[54] https://medibangpaint.com/en/tutorial/pc/save
[55] https://medibangpaint.com/en/use/2022/12/especiallyfaq_012
[57] https://ibispaint.com/lecture/index.jsp?no=119
[58] https://help.sketchbook.com/docs/import-from-photoshop
[59] https://help.sketchbook.com/docs/blending-layers
[60] https://help.sketchbook.com/docs/grouping-layers
[61] https://www.photopea.com/learn/opening-saving
[62] https://www.pixelmator.com/blog/2021/08/19/pixelmator-pro-2-1-3-brings-huge-improvements-to-psd-support
[63] https://docs.gimp.org/3.2/en/file-psd-export.html
[64] https://github.com/GNOME/gimp/blob/master/plug-ins/file-psd/psd-export.c and https://github.com/GNOME/gimp/blob/master/plug-ins/file-psd/psd-util.c
[65] https://painttoolsai.readthedocs.io/en/latest/features/canvas-and-file-formats.html
[66] https://documentation.help/PaintTool-SAI/layer.html
[67] https://krita-artists.org/t/feedback-wanted-paint-tool-sai-blend-modes-psd-compatibility-shine-fixes/144191
[68] https://ask.clip-studio.com/en-us/detail?id=106517
[69] https://github.com/cromachina/psd-export
[70] https://escapemotions.com/products/rebelle/manual/7/creating-opening-saving/
[71] https://www.artrage.com/wp-content/uploads/ArtRage-Feature-Comparison.pdf
[72] https://forums.artrage.com/archive/index.php/t-56809.html
[73] https://product.corel.com/help/Painter/540232071/Main/EN/Doc/Corel-Painter-Importing-and-exporting-layers.html
[74] https://concepts.app/en/tutorials/psd-import-and-export-concepts-58/
[75] https://concepts.app/manual/export
[76] https://docs.infinitestudio.art/painter/data/export/
[77] https://docs.infinitestudio.art/painter/layers/grouping/
[78] https://apps.apple.com/us/app/tayasui-sketches/id641900855
[79] https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/
[80] https://community.adobe.com/questions-712/how-to-read-the-full-resolution-composite-from-a-psd-file-1104503
[81] https://github.com/psd-tools/psd-tools/issues/513

**Biggest gaps:** I found no primary documentation of how Fresco and Procreate PSDs encode vector layers, masks, clipping and text, how MediBang and ibisPaint layers come out, or what Affinity V3 writes. These need real sample files from those apps to settle.
