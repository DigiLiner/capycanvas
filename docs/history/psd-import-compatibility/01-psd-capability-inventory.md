# PSD/PSB capability inventory for Capy Canvas import

[PSD import compatibility](../psd-import-compatibility-research.md) · source report, 2026-10-16

Research for the [PSD import compatibility record](../psd-import-compatibility-research.md), which holds the gap analysis built from it.

**Sources.** The base is Adobe's *Photoshop File Formats Specification*, November 2019 web edition ([spec]). I cross-checked it against the current source of three open-source parsers:
- **psd-tools** ([pt-const] and siblings), main branch, 2026-10-16.
- **ag-psd** ([ag-doc]), master branch, 2026-10-07.
- **PhotoCraft's clean-room `photocraft-psd` and `photocraft-io`** ([pc-psd]), main branch, 2026-10-16. They record behaviour measured against Photoshop 27.11.

Adobe and press pages cover the features added in 2020–2026.

Coverage limits:
- **Krita:** I read its feature list only ([krita]) and none of its code.
- **GIMP:** I read release coverage only, no code. No GPL code was read or quoted.
- **libpsd:** not studied.
- **Photopea:** its public pages give no format details ([ppea]).
- **Cloud documents (.psdc):** the spec excludes them.

**Legend.** **Use** rates how often a capability appears in real files: **C** common in artist, designer and comic files, **O** occasional, **R** rare. These ratings are my judgment, not measurements. *(inferred)* marks my own reasoning with no source behind it.

## 1. File structure every importer must handle

- **Section order:** header, then colour-mode data, then image resources (`8BIM` plus a 16-bit ID), then layer and mask info (layer records, per-channel data, global mask info, global tagged blocks), then the merged composite ([spec]).
- **Tagged blocks:** `8BIM` or `8B64`, a 4-character key, then a length.
  - In PSB files the spec lists 13 keys with 8-byte lengths.
  - PhotoCraft found 8 more in real files: `lnk3 lnkE pths extd extn FELS cinf artd` ([pc-tagged]).
- **16- and 32-bit documents** keep the whole layer list inside a global `Lr16` or `Lr32` block ([spec]).
- **Descriptors:** almost everything added after Photoshop 6 is a version-16 ActionDescriptor, a typed key/value tree. New keys appear with each release, so the durable approach is a descriptor parser plus a raw round trip *(inferred)*.
- **Layer flags:**
  - bit 0 is the transparency lock;
  - bit 1 means **hidden** (the spec calls it "visible", and readers invert it, [pt-lm]);
  - bit 4 means "pixel data irrelevant to appearance".
- **Order and groups:** records run bottom-first. Each group is the folder record (`lsct` 1 open, 2 closed) plus a hidden bounding divider record (`lsct` 3) below it ([spec]).

### Stored pixels an importer can fall back to

| Layer kind (how detected) | Stored channel data | Caveats |
|---|---|---|
| Pixel or Background layer | Exact pixels | Effects are never baked in. Mask, opacity and blend mode are not applied ([ag-doc]). A bottom record with no −1 channel opens as a locked Background whatever its name ([pc-import]). |
| Group, artboard, frame (`lsct` 1/2) | None | The artboard background is drawn from `artb`. |
| Adjustment (`levl`, `curv` …) | None. The spec says "undefined (probably all white)" ([spec]). | Must be re-implemented. |
| Fill layer (`SoCo`/`GdFl`/`PtFl`, no vector mask) | Often an empty rectangle. When present, it is Photoshop's rendering ([pc-blocks], [pc-import]). | |
| Shape layer (vector mask plus a fill key, or `vscg`/`vstk`) | Rasterized fill and stroke ([ag-doc]) | Missing in some 32-bit and third-party files. With a feathered vector mask, the pixels lack the fill beyond the path ([pc-import]). |
| Text (`TySh`) | Rasterized text ([ag-doc]) | ag-psd, GIMP and other writers store none ([pc-text]). Fonts are named, not embedded. |
| Smart object (`SoLd`/`SoLE`/`PlLd`) | The transformed, smart-filtered rendering **without the filter mask**. The unfiltered pixels and the mask sit in the global `FEid` block ([pc-parity]). | A linked source may have changed since the save *(inferred)*. |
| Video, legacy 3D (`PxSc`, `PxSD`) | Current frame or render *(inferred)* | |
| Masks | Channel −2 holds the user mask. When a vector mask also exists, −2 can be the rasterized vector mask (mask flag bit 3) and −3 the real user mask ([spec], [ag-doc]). | |
| Merged composite | The full rendering including effects, when "Maximize compatibility" was on. Otherwise it is not a real composite. Check `hasRealMergedData` in resource 1057 ([spec]). | |

## 2. Document

| Capability | Structure | Notes | Use |
|---|---|---|---|
| Colour modes | Header mode: 0 Bitmap, 1 Gray, 2 Indexed, 3 RGB, 4 CMYK, 7 Multichannel, 8 Duotone, 9 Lab ([spec]) | Bitmap and Indexed cannot hold layers ([pse-modes]). Photoshop discards the layer records of a Multichannel file ([pt-usage]). CMYK samples are stored inverted, so 0 means full ink ([pc-psd]). | RGB C; CMYK O (print); Gray O; the rest R |
| Bit depth | 1 (Bitmap only), 8, 16, 32 ([spec]) | 32-bit is linear float ([ag-doc]). A 32-bit file needs an `hdrt` block in colour-mode data or Photoshop rejects it ([pt-cmd]). HDR toning settings are in 1070. | 8 C, 16 O, 32 R |
| Size | PSD up to 30,000 px per side, PSB up to 300,000 px; 1–56 channels ([spec]). PSD files are capped at 2 GB ([bwill]). | | PSB O |
| Indexed palette | 768 bytes of colour-mode data in planar order; count in 1046, transparent index in 1047 | | R |
| Duotone | Colour-mode data is undocumented; the spec says to read as gray and keep the bytes ([spec]). It probably mirrors the .ADO format: 1–4 inks, curves, overprints *(inferred)*. Related: 1018, 1014/1017, 1066. | | R |
| ICC | 1039 holds the raw profile; 1041 flags "intentionally untagged" | | C |
| OCIO (Oct 2024, 26.0) | Global `OCIO` descriptor `documentColorManagementInfo`: `Knd` icc or ocio, plus display and view ([ag-ai]) | Per-document OpenColorIO and ACES ([cg26], [ocio]) | R |
| Resolution, background colour, print, halftone, transfer, EPS | 1005, 1010, 1011–1017, 1021, 1062, 1071, 1082–1085, 10000 | | 1005 C, rest R |
| Thumbnail | 1036 (JPEG); 1033 is the older BGR form | | C |
| Global light | 1037 angle (defaults to 30), 1049 altitude | Shared by every effect with "use global light" on | C with effects |

## 3. Layers and structure

| Capability | Structure | Meaning | Use |
|---|---|---|---|
| Name, ID | Pascal name plus `luni` (Unicode); `lyid`; ID seed in 1044 | | C |
| Groups | `lsct` holds the type, a blend key (`pass` = pass-through) and a subtype (1 = timeline scene group). `lsdk` is a nested variant ([pt-tb]). | | C |
| Clipping | Record byte: 1 = clipped to the base below | psd-tools ignores a clip flag on a *group* in Photoshop files but honours it for Clip Studio and SAI files ([pt-layers]) | C (comics) |
| Locks | Record bit 0 plus `lspf`: 0x1 transparency, 0x2 pixels, 0x4 position, 0x8 artboard/frame auto-nest, 0x80000000 lock all ([spec], [pt-tb], [pc-blocks]) | | C |
| Label colour | `lclr`: none, red, orange, yellow, green, blue, violet, gray ([ag-doc]) | | O |
| Link Layers | 1026 holds a group ID per record (0 = unlinked); also 1072 ([spec], [pc-import]) | Linked layers move together | O |
| Selection | 1069 layer IDs, 1024 target index | UI state | C |
| Artboards (CC 2015) | `artb` on the group: rect, preset name, background type and colour, guide indices. `artd`/`abdd` hold document settings ([spec], [ag-doc]). | Clip their children and draw a background | O (UI design) |
| Frames (CC 2019) | Undocumented `frgb` descriptor `framedGroup` on a group: `framedGroupRect`, `framedGroupType`, `LockPPI`, `PreferredResolution`. I read these from a psd-tools test fixture ([pt-frgb]). | The frame shape clips the placed content *(inferred)* | R |
| Reference point | `fxrp`, two doubles | Pivot point *(inferred)* | — |
| Per-layer metadata | `shmd` items: `cust` (timestamp), `cmls` (layer comp state), `mlst` (frame state), `tmln` (timeline), `mdyn`, `extn` ([pt-tb], [ag-ai]) | | C |
| Export/Generator settings | `extn`, `extd` | Asset settings for Generator and Export As | O |
| Name source, version | `lnsr`, `lyvr` | Bookkeeping | — |

## 4. Blending

- **Modes.** There are 27: `norm diss dark mul idiv lbrn dkCl lite scrn div lddg lgCl over sLit hLit vLit lLit pLit hMix diff smud fsub fdiv hue sat colr lum`. Groups also have `pass` (pass-through) ([spec]). psd-tools composites every mode except Dissolve, which needs Photoshop's noise to match ([pt-index]).
- **Fill vs opacity.**
  - Record opacity scales the layer together with its effects. Fill opacity is stored in `iOpa`, one undocumented byte ([ag-ai]), and leaves effects alone.
  - When "Blend Interior Effects as Group" is on, the interior effects also fade with fill *(inferred)*.
  - Eight modes give different results for fill and opacity: Color Burn, Linear Burn, Color Dodge, Linear Dodge, Vivid Light, Linear Light, Hard Mix and Difference ([ptc-fill]). For these, a compositor has to apply fill inside the blend function.
- **Advanced blending.** Defaults are from [pc-blocks]; meanings come from [spec] where it gives them.
  - `clbl`, Blend Clipped Layers as Group (default on): the base layer's mode blends the whole clipping group. When off, each clipped layer blends with its own mode.
  - `infx`, Blend Interior Effects as Group (default off): inner glow, satin and the overlays merge with the content before the layer's mode is applied.
  - `tsly`, Transparency Shapes Layer (default on): the layer's transparency shapes its effects. When off, transparency acts like fill opacity ([spec]).
  - `lmgm` / `vmgm`, Layer Mask / Vector Mask Hides Effects (default off): the mask becomes a final crossfade over the layer and its effects, instead of shaping the layer before effects ([spec]).
  - `knko`, knockout: 0 none; 1 shallow, to the enclosing group's backdrop; 2 deep, through pass-through groups, stopping at an isolated group or at the Background layer. It shows only when fill is below 100% ([pt-const], [pt-comp]).
  - `brst`: channel restrictions, the R/G/B checkboxes ([spec]).
  - **Blend If:** the record's blending ranges hold a composite-gray range plus one per colour channel. Each has a source ("This Layer") and destination ("Underlying Layer") range of four bytes: black low, black high, white low, white high. The low/high pairs are the split sliders ([spec], [pc-blocks]).
- **Pass-through edge case:** a pass-through group that is a clipping base and contains an adjustment renders isolated ([pc-parity]).
- **Use:** fill opacity, clipping and the common modes are C. Blend If is O (retouching). Knockout and the advanced flags are R–O.

## 5. Masks

| Capability | Structure | Use |
|---|---|---|
| Raster layer mask | Channel −2. The record holds a rect, a default colour for outside the rect, and flags: bit 1 disabled, bit 0 "position relative to layer", which PhotoCraft reads as *unlinked* ([spec], [pc-import]) | C |
| Density, feather | Mask parameters: user and vector density (1 byte each) and feather (a double, in pixels) ([spec]). PhotoCraft matches the feather with blurs of equal variance ([pc-parity]). | O |
| Vector mask | `vmsk` (`vsms` since CS6): flags invert / not linked / disabled (values 1/2/4) plus path records ([spec], [ag-ai]). On a pixel layer it is a mask; with fill content it makes a shape layer. | O |
| Vector and raster mask together | Channel −3 plus "real" flags and rect ([spec]) | O |
| Smart-filter mask | `FMsk` stores only the overlay colour and opacity ([spec]). The pixels live in `FEid`/`FXid`, keyed by placed ID ([pc-import]). Enable, linked and extend-with-white flags are in `SoLd` ([ag-psd]). | O |
| Global mask overlay | Global mask info: overlay colour, opacity, kind (UI only) | — |

## 6. Adjustment and fill layers

The older binary adjustments reuse Photoshop's preset file formats, so precision is 8-bit; curve points, for example, run 0–255 ([spec]). Later ones are descriptors. Parameter lists are from [ag-psd].

| Key | Layer | Parameters |
|---|---|---|
| `brit` (+`CgEd`) | Brightness/Contrast | Brightness, contrast, mean, Lab-only. `CgEd` adds the legacy flag and auto. |
| `levl` | Levels | 29 records (input/output floor and ceiling, gamma) for master and each channel; `Lvls` extension; preset |
| `curv` | Curves | Point list per channel (up to 19 points, 0–255); `Crv ` extension; preset |
| `expA` | Exposure | Exposure, offset, gamma |
| `vibA` | Vibrance | Vibrance, saturation |
| `hue2` (`hue ` in PS4) | Hue/Saturation | Colorize, master HSL, six ranges each with 4 range values and HSL |
| `blnc` | Color Balance | Three sliders each for shadows, midtones, highlights; preserve luminosity |
| `blwh` | Black & White | Six colour weights, tint flag and colour |
| `phfl` | Photo Filter | Colour (XYZ in version 3), density, preserve luminosity |
| `mixr` | Channel Mixer | Monochrome flag; per output channel, weights plus constant |
| `clrL` | Color Lookup | Embedded 3D LUT bytes (.cube, .3dl or .look), abstract or device-link profile, dither, data/table order |
| `nvrt`, `post`, `thrs` | Invert, Posterize, Threshold | None / levels / threshold level |
| `grdm` | Gradient Map | Solid gradient (stops, midpoints, smoothness) or noise gradient; dither, reverse, interpolation method |
| `selc` | Selective Color | Relative/absolute; CMYK values for 9 colour ranges |
| `SoCo`/`GdFl`/`PtFl` | Solid/Gradient/Pattern fill | Colour; gradient (linear/radial/angle/reflected/diamond, angle, scale, reverse, dither, align, offset, method); pattern (ID, scale, phase, link) |
| Unknown | Clarity & Dehaze, Grain (27.3, January 2026) | Keys not documented anywhere I found ([cg273]) |

**Use:** Curves, Levels, Hue/Sat, Color Balance, Gradient Map and solid fills are C. Selective Color, Channel Mixer, Color Lookup, Photo Filter and Black & White are O. Posterize, Threshold and Invert are R–O. Adjustment presets and the Adjustment Brush produce ordinary adjustment layers and groups *(inferred)*.

## 7. Layer styles

**Storage**
- `lfx2` is the descriptor when each kind has one instance.
- `lmfx` appears when any kind has several, and supersedes `lfx2`. It uses the lists `dropShadowMulti`, `innerShadowMulti`, `solidFillMulti`, `gradientFillMulti` and `frameFXMulti`.
- Groups use `lfxs`.
- `lrFX` is the PS5 binary form, and Photoshop ignores it on groups ([ag-ai], [pc-import]).
- Since CC 2015, stroke, drop shadow, inner shadow, colour overlay and gradient overlay allow up to 10 instances each ([cpro]).
- Global settings: `masterFXSwitch`, the style scale `Scl `, and resource 1042 ("effects hidden").

**Effects.** Every effect has enabled, present, show-in-dialog, blend mode, opacity and colour ([ag-psd]).
- **Drop Shadow `DrSh` / Inner Shadow `IrSh`:** angle, global light, distance, spread, size, contour, anti-alias, noise. Drop shadow adds "layer knocks out drop shadow".
- **Outer Glow `OrGl` / Inner Glow `IrGl`:** colour or gradient, technique (softer/precise), source (centre/edge, inner glow only), spread, size, contour, range, jitter, noise.
- **Bevel & Emboss `ebbl`:**
  - shape: style (outer/inner/emboss/pillow/stroke emboss), technique (smooth/chisel hard/chisel soft), depth, direction, size, soften;
  - shading: angle, altitude, gloss contour, highlight and shadow mode, colour and opacity;
  - sub-effects: Contour and Texture (pattern, scale, depth, invert, link). ag-psd does not type the texture keys.
- **Satin `ChFX`:** angle, distance, size, contour, invert.
- **Overlays:**
  - Colour `SoFi`;
  - Gradient `GrFl`: style, angle, scale, reverse, dither, align, offset, interpolation method;
  - Pattern `patternFill`: pattern, scale, phase, align.
- **Stroke `FrFX`:** size, position (inside/centre/outside), fill (colour/gradient/pattern), overprint.

**Shared resources.** Contours are named curves with corner flags ([ag-psd]). Patterns are referenced by name and ID. They resolve against the global `Patt`/`Pat2`/`Pat3` blocks, which allow any colour mode and depth ([spec]), and against `shpa`.

**Use:** Stroke, Drop Shadow and Colour Overlay are C (design, comic lettering). Bevel, Satin, Pattern Overlay and multiple instances are O.

## 8. Vector content

- **Path records** are 26 bytes ([spec]):
  - each knot has three points (in, anchor, out), stored as 8.24 fixed-point fractions of the document height and width;
  - knots are linked or unlinked, and subpaths open or closed;
  - other records hold the fill rule, an "initial fill starts with all pixels" flag, and clipboard bounds.
  - Undocumented: each subpath length record also carries its boolean operation (0 exclude, 1 combine, 2 subtract, 3 intersect) and its own fill rule ([ag-ai]).
- **Shape layers** (CS6+) are built from four blocks ([spec], [pc-import]):
  - `vsms`: the path;
  - `vscg`: the fill content, a key (SoCo, GdFl or PtFl) plus a descriptor;
  - `vstk`: the stroke;
  - `vogk`: optional live-shape data.

  Older shape layers use a fill key plus `vmsk`.
- **Strokes**, `vstk` `strokeStyle` ([pt-shape], [ag-doc]):
  - on/off for stroke and fill;
  - geometry: width, dash set and offset, cap (butt/round/square), join (miter/round/bevel), miter limit, alignment (inside/centre/outside);
  - scale lock, stroke adjust, blend mode, opacity, resolution;
  - paint: colour, gradient or pattern.
- **Live shapes**, `vogk` `keyDescriptorList` ([pt-shape], [ag-ai]):
  - `keyOriginType`: 1 rectangle, 2 rounded rectangle (four radii), 4 line (start, end, weight), 5 ellipse;
  - each entry also holds a bounding box, box corners and a transform;
  - `keyShapeInvalidated` is set once the shape has been edited as a path.

  The polygon, star and triangle properties from the 2021 rework ([polyps]) use origin types no library decodes.
- **Saved paths:** resources 2000–2997, with Unicode names in `pths`. Also: clipping path 2999, working path 1025, path selection 1088, origin path info 3000 ([spec]).
- `sn2P` turns on aligned (pixel-snapped) rendering ([spec]).
- **Use:** shape layers C for design and O for illustration. Vector masks O. Clipping paths O (product photos).

## 9. Text

- **`TySh`** ([spec], [ag-psd]):
  - 2×3 transform matrix;
  - text descriptor: `Txt `, orientation `Ornt`, anti-alias `AntA` (none/sharp/crisp/strong/smooth/platform/platform LCD), gridding, bounds, `TextIndex`, and the raw EngineData;
  - warp descriptor.

  `tySh` is the PS5 legacy form.
- **EngineData** is a per-layer, PostScript-like dictionary ([ag-text], [pt-type], [pc-textpsd]):
  - text: UTF-16 with `\r` line breaks;
  - runs: `StyleRun` and `ParagraphRun`;
  - `Rendered/Shapes`: ShapeType 0 point or 1 box, PointBase, BoxBounds, WritingDirection;
  - `ResourceDict`: a FontSet of PostScript names, named character styles (`StyleSheetSet`) and paragraph styles (`ParagraphSheetSet`), Kinsoku/MojiKumi sets, and superscript/subscript/small-caps sizes.
- **Character attributes:**
  - font and size: font, size, faux bold/italic, leading, horizontal/vertical scale;
  - spacing: tracking, kerning, baseline shift;
  - decoration: caps, super/subscript, underline, strikethrough;
  - ligatures and OpenType flags: `ContextualLigatures Swash OldStyle StylisticAlternates Titling Ornaments Ordinals Fractions`;
  - paint: fill and stroke colours, fill-first, outline width;
  - language and script: language, no-break, and CJK/Arabic options such as tsume and kashida.
- **Paragraph attributes:**
  - justification (7 kinds), indents, space before and after;
  - hyphenation; word, letter and glyph spacing (minimum, desired, maximum); auto-leading;
  - roman hanging punctuation, kinsoku, single-line or every-line composer.
- **`Txt2`** is a global, newer engine model. It holds things EngineData lacks ([ag-ed2], [pc-textpsd]):
  - Fractions, a stylistic-sets bitmask, and per-pair kerning mode;
  - `TextFrameSet`, which stores **type on a path** as bezier control points.
- **Warp text styles:** arc, arc lower, arc upper, arch, bulge, shell lower, shell upper, flag, wave, fish, rise, fisheye, inflate, squeeze, twist. Each has bend, horizontal and vertical distortion, and an orientation ([ag-psd]).
- **Fonts** are stored as PostScript names only. Where variable-font axes and Dynamic Text (26.8, auto-fit box, [cg273]) are stored is undocumented.
- **Use:** text is C for designers and letterers. Clip Studio Paint rasterizes text on PSD export unless "Text only" is chosen ([csp]). Vertical text is O (manga). Path and warp text are O.

## 10. Smart objects and smart filters

- **Layer block.** `SoLd` is the main form; `SoLE` is a CC 2015 variant and `PlLd` the CS2 legacy form. It holds ([spec], [ag-psd]):
  - identity: `Idnt` (links to the file payload) and `placed` (this instance's ID);
  - source info: type (vector, raster, image stack), page and total pages for PDFs, crop, frame step and duration for video, size, resolution, anti-alias;
  - transform: `Trnf` (four corners, so any quad including perspective), `nonAffineTransform`, and `warp`, an envelope mesh that includes the `quiltSliceX/Y` split-warp grid;
  - `compInfo`: the layer comp chosen inside the child document;
  - `filterFX`: the smart filter list.
- **Payloads** live in the global blocks `lnkD`, `lnk2`, `lnk3` and `lnkE`:
  - `liFD` is embedded bytes, `liFE` an external file (paths, size, date), `liFA` an alias ([spec]);
  - version 5 adds a child document ID, version 6 an asset modification time, version 7 a Libraries asset lock ([spec]);
  - version 8 (Photoshop 26.9+) appends a descriptor that Photoshop requires ([pt-cl]).
- **Embedded file types seen:** PSD, PSB, PDF/AI, PNG, JPEG, GIF, TIFF, BMP and ZIP-based files ([pt-so]). SVG places as a vector smart object ([svg]).
- **Smart filters.** Each filter has a name, ID, enabled flag, opacity, blend mode, options, and foreground and background colours. The filters share one mask ([ag-psd]). ag-psd types about 55 filters, including Liquify, Puppet Warp and Perspective Warp. PhotoCraft maps Camera Raw ([pc-parity]).
- **Use:** embedded smart objects are C (design, mockups). Linked and Library smart objects O. Smart filters O (photography).

## 11. Channels and saved selections

- Alpha and spot channels exist only as extra planes of the merged image, never per layer *(inferred from the structure)*.
- **Channel metadata:**
  - names in 1006 and 1045, identifiers in 1053;
  - `DisplayInfo` (1077) per channel: colour, opacity, and kind (0 alpha, 1 inverted, 2 spot) ([pt-const]);
  - alternate spot colours in 1067, Quick Mask in 1022.
- **Merged transparency** is signalled by a negative layer count or by `Mtrn`/`Mt16`/`Mt32` ([spec], [pc-parity]).
- **Use:** alpha channels O (retouching). Spot channels R (packaging).

## 12. Guides, slices, grids, notes

- **Guides** (1032): horizontal and vertical, in 1/32-pixel units ([ag-ir]); artboard guides are in `artb`. C
- **Grid:** not stored; the field is reserved ([spec]).
- **Slices** (1050, URLs in 1054). R
- **Notes** (`Anno`): text annotations with author and date. R
- **Other tools:** colour samplers (1073), count tool (1080), measurement scale (1074), ImageReady variables (7000/7001). All R.

## 13. Layer comps

Resource 1065 is a descriptor. Each comp has a name, ID and comment, and captures visibility, position and appearance; the last-applied comp is recorded too. Per-layer state is in `shmd` `cmls` ([ag-ir], [ag-ai]). Use: O.

## 14. Timeline, animation, video, 3D

- **Frame animation:** plug-in resource 4000 `mani`/`AnDs` (frames, delays, disposal), per-layer `mlst`, `anFX`, and onion skins in 1078 ([ag-ir]).
- **Video timeline:**
  - resource 1075: frame rate, duration, tracks, audio;
  - per-layer `tmln` keyframes for opacity, position, transform, style and global light;
  - scene groups via `lsct`;
  - video layers via `PxSc` (external link) and `PxSD`.
- **3D** was removed by July 2024 ([3d]).
- **Use:** frame animation R–O; video and 3D R.

## 15. Metadata and provenance

- **Standard metadata:**
  - XMP (1060), EXIF (1058/1059), IPTC (1028);
  - caption (1008), copyright (1034), version info (1057);
  - Lightroom workflow (8000), compositor (`cinf`).
- **Content Credentials:** the `CAI ` block holds `enab` and `generationalGuid` ([pt-cai]). Signed manifests are attached on JPG/PNG export ([cc-export]).
- **Generative AI:** `GenI` holds `genTechInfo` with `isUsingGenTech` and `externalModelList` ([ag-ai]). Generative layers keep several variations each ([genvar]), in undocumented storage.

## 16. Additions since the spec

| Release | Feature | Where it is stored |
|---|---|---|
| CC 2015 | Multiple effect instances | `lmfx` |
| CC 2015 | Artboards | `artb`/`artd`/`abdd` |
| CC 2019 | Frames | `frgb` |
| 2020 | Split warp grid; compositor info | `quiltSlice*`; `cinf` |
| 2021 | Polygon/star/triangle live shapes | `vogk`, types unknown |
| 2022–23 | Gradient methods Perceptual, Linear, Classic, Smooth ([gradm], [grad23]) | `gradientsInterpolationMethod` = `Perc`/`Lnr `/`Gcls`/`Smoo` in every gradient ([pc-blocks]) |
| 25.0 (2023) | Generative layers | `GenI`, rest unknown |
| 2023–24 | Content Credentials | `CAI ` |
| 26.0 (2024) | OCIO/ACES | `OCIO` |
| 26.8 / 26.9 (2025) | Dynamic Text / linked-file entry version 8 | unknown / `lnk*` |
| 27.3 (2026) | Clarity & Dehaze, Grain | unknown |

## 17. Not verified

- Storage for generative variations, Dynamic Text, the 27.3 adjustment layers, variable-font axes and polygon live shapes.
- The mask "linked" bit: the spec names none; PhotoCraft reads bit 0 as unlinked.
- Whether a PSD ever holds a full C2PA manifest.
- The values of `framedGroupType`, and how `abdd` differs from `artd`.
- The meaning of `vowv`, `FELS` and resource 1092.
- All Use ratings, which are judgment.

[spec]: https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/
[pt-const]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/constants.py
[pt-tb]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/psd/tagged_blocks.py
[pt-lm]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/psd/layer_and_mask.py
[pt-comp]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/composite/composite.py
[pt-shape]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/api/shape.py
[pt-layers]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/api/layers.py
[pt-so]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/api/smart_object.py
[pt-type]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/api/typesetting.py
[pt-usage]: https://github.com/psd-tools/psd-tools/blob/main/docs/usage.rst
[pt-index]: https://github.com/psd-tools/psd-tools/blob/main/docs/index.rst
[pt-cl]: https://github.com/psd-tools/psd-tools/blob/main/docs/changelog.rst
[pt-cmd]: https://github.com/psd-tools/psd-tools/blob/main/src/psd_tools/psd/color_mode_data.py
[pt-frgb]: https://github.com/psd-tools/psd-tools/blob/main/tests/tagged_blocks/frgb_1.dat
[pt-cai]: https://github.com/psd-tools/psd-tools/blob/main/tests/tagged_blocks/CAI.dat
[ag-doc]: https://github.com/Agamnentzar/ag-psd/blob/master/README_PSD.md
[ag-psd]: https://github.com/Agamnentzar/ag-psd/blob/master/src/psd.ts
[ag-ai]: https://github.com/Agamnentzar/ag-psd/blob/master/src/additionalInfo.ts
[ag-ir]: https://github.com/Agamnentzar/ag-psd/blob/master/src/imageResources.ts
[ag-text]: https://github.com/Agamnentzar/ag-psd/blob/master/src/text.ts
[ag-ed2]: https://github.com/Agamnentzar/ag-psd/blob/master/src/engineData2.ts
[pc-psd]: https://github.com/storytold/photocraft/blob/main/crates/psd/README.md
[pc-tagged]: https://github.com/storytold/photocraft/blob/main/crates/psd/src/tagged.rs
[pc-parity]: https://github.com/storytold/photocraft/blob/main/docs/file-format-parity.md
[pc-import]: https://github.com/storytold/photocraft/blob/main/crates/io/src/psd_import.rs
[pc-blocks]: https://github.com/storytold/photocraft/blob/main/crates/io/src/blocks.rs
[pc-text]: https://github.com/storytold/photocraft/blob/main/crates/io/src/text_import.rs
[pc-textpsd]: https://github.com/storytold/photocraft/blob/main/crates/text/src/psd.rs
[krita]: https://docs.krita.org/en/general_concepts/file_formats/file_psd.html
[ppea]: https://www.photopea.com/learn/opening-saving
[pse-modes]: https://helpx.adobe.com/ca/photoshop-elements/desktop/working-with-colors/using-image-modes-color-tables.html
[bwill]: https://bwillcreative.com/psd-vs-psb-photoshop-files/
[ocio]: https://helpx.adobe.com/photoshop/using/opencolorio-transform.html
[cg26]: https://www.cgchannel.com/2024/10/adobe-releases-photoshop-26-0-and-updates-photoshop-on-the-web/
[cg273]: https://www.cgchannel.com/2026/01/adobe-releases-photoshop-27-3-and-photoshop-27-4-beta/
[ptc-fill]: https://photoshoptrainingchannel.com/tips/opacity-vs-fill-blending-modes/
[cpro]: https://creativepro.com/adding-multiple-strokes-to-editable-text-in-photoshop/
[polyps]: https://community.adobe.com/t5/photoshop-ecosystem-discussions/is-there-a-way-to-get-the-legacy-polygon-shape-tool-back-in-ps2021/m-p/11813020/highlight/true
[gradm]: https://luminousworks.substack.com/p/grokking-gradients
[grad23]: https://andrewhaysom.myportfolio.com/the-gradient-tool-changes-in-ps-2023
[svg]: https://community.adobe.com/t5/photoshop-ecosystem-discussions/importing-svg-into-photoshop-without-rasterizing-it/m-p/11117377/highlight/true
[csp]: https://support.clip-studio.com/en-us/faq/articles/20220034
[3d]: https://helpx.adobe.com/photoshop/using/creating-3d-objects-animations-photoshop.html
[cc-export]: https://helpx.adobe.com/ca/photoshop/desktop/save-and-export/metadata-content-credentials/export-your-work-with-content-credentials.html
[genvar]: https://community.adobe.com/t5/photoshop-ecosystem-discussions/is-there-a-quot-simple-quot-way-to-export-all-generation-variations/m-p/14442167
