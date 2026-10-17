# PSD import compatibility: importer survey and model gaps

[Technical documentation](../README.md) · [Design and validation history](README.md)

Research date: **2026-10-16**, source baseline **`af7a22a6a`**.

This record asks whether Capy's authored model and `.capy` format can grow to hold
what PSD and PSB files carry, so that artists can bring their work over from
other programs. "Other programs" means Photoshop and the many apps that write
PSD: Illustrator, Fresco, Procreate, Clip Studio Paint, Krita, Affinity, SAI and
others.

- **What it covers:**
  - what those files actually contain;
  - how about thirty other importers handle each feature;
  - where each feature would live in Capy;
  - the gaps.
- **What it builds on:** the [PSD and layer styles record](psd-and-layer-styles-research.md) of 2026-10-07. Its gap table is superseded here, and its style compositing analysis and Properties design still stand.
- **Status:** research only. No importer exists, and no PSD was opened in Capy.

The research combines five streams. Their write-ups, with every source URL, are in [`psd-import-compatibility/`](psd-import-compatibility):

| # | Stream | File |
|---|---|---|
| 1 | Everything a PSD/PSB can carry: structures, meanings, stored fallback pixels, features since the 2019 specification | [01-psd-capability-inventory.md](psd-import-compatibility/01-psd-capability-inventory.md) |
| 2 | What Illustrator, Fresco, Procreate, Clip Studio Paint, Krita, Affinity and 13 other programs write into PSD | [02-psd-writers.md](psd-import-compatibility/02-psd-writers.md) |
| 3 | Desktop importers: Photopea, Affinity, Pixelmator Pro, Krita, GIMP, Corel, Paint.NET, Clip Studio Paint, Painter, Rebelle, Elements, Illustrator | [03-desktop-importers.md](psd-import-compatibility/03-desktop-importers.md) |
| 4 | Mobile, web and design importers; PSD libraries and test corpora | [04-mobile-web-and-library-importers.md](psd-import-compatibility/04-mobile-web-and-library-importers.md) |
| 5 | Capy's model, format and plans for each capability (file:line at `af7a22a6a`) | [05-capy-model-audit.md](psd-import-compatibility/05-capy-model-audit.md) |

**Caveats on the evidence**
- **Vendor pages.** Adobe help pages and several community sites refused direct fetches. Those claims come from archived copies or search extracts, which the reports mark.
- **Thin documentation.** Most mobile apps document their PSD behaviour in a sentence or two. Fresco's and Procreate's encodings of vector layers, masks, clipping and text, and everything ibisPaint and MediBang write, need real sample files to settle.
- **No tests run.** Nobody opened a file in any app for this record. Rows come from documentation, source code and user reports.
- **No parity measured.** No blend mode or adjustment has been checked numerically against Photoshop.

---

## 1. Summary

**The model is flexible enough.** No PSD capability needs a redesign:
- **Additive features:** every one either has a planned slot or fits as new vocabulary (a record type, field, value or choice), which the [package contract](../reference/capy-package.md) allows.
- **Combinations:** the few that Capy reads as unsupported today, such as a clipped adjustment, stay open for a later version to define.
- **Defaults:** Photoshop's advanced blending options all default to Capy's fixed behaviour today: Blend Clipped Layers as Group on, Transparency Shapes Layer on, Layer Mask Hides Effects off. Files that keep the defaults need no new fields.

**Three things should be decided before the format freezes,** because frozen readers apply their rules to every later file (section 5):
1. **Whole-document read-only.** Every PSD feature added after the freeze (styles, fill opacity, text, vector masks, Blend If and more) makes older versions open the *whole* document read-only. Opening only the affected layer read-only needs a reader rule that exists at the freeze.
2. **Presentation-only properties.** Color labels, position locks, linked layers and comps should not make files read-only in older versions. The existing copy-safe ancillary records can carry them, but that should be stated as policy now.
3. **Clipped adjustments.** A guide describes them as frozen semantics, while the code rejects them. The code is right to leave them open, and the guide should say so.

**Biggest gaps for real files**, most common first (section 6):
1. **Layer styles.** Stroke, drop shadow and color overlay are common in design and comic lettering. Photoshop's stored layer pixels never include effects, so without a style renderer Capy cannot even bake them.
2. **Clipped adjustment layers,** which act on the whole clipping run below them. Clipping is common in comic files (report 1). Capy attaches an adjustment to one owner only.
3. **Fill opacity,** separate from opacity, including its different results in eight blend modes.
4. **Text,** planned as a vector-object item. PSD text needs CJK composition rules for manga, anti-alias modes and warp.
5. **Shape layers and vector masks.** Vector masks need their own field. Per-subpath combine and subtract need a Bézier boolean engine at import.
6. **Transparency Shapes Layer off.** Clip Studio's Glow Dodge and Add (Glow) are written as Color Dodge and Linear Dodge with this option off, according to a reverse-engineered converter that matches Clip Studio's output. SAI's normal modes also behave like Photoshop's with it off. Capy cannot express that.
7. **Adjustment parity.** Curves interpolation, Hue/Saturation ranges and gradient interpolation methods differ from Photoshop.
8. **CMYK print files.** Capy documents are RGB, so an importer would convert them, losing the separate ink values (inferred).

**Illustrator and Fresco** (section 7):
- **Illustrator.** Its PSD export rasterizes most vector art, so PSD is the wrong route. Its PDF-compatible `.ai`, or a PDF export, should keep paths, gradients, transparency and clipping. That needs a PDF or AI importer, which the vector design defers to "later" without a milestone. The [extension research](object-content-extensions-research.md) maps PDF's model onto Capy's slots.
- **Fresco.** Its PSDs carry pixel layers, blend modes and three adjustments that Capy already has. Groups, masks and clipping are probably Photoshop's own encodings but unverified. How its vector layers are encoded is unverified too, and live paint state is probably lost at the source.

**How importers degrade** (section 3). The good ones keep layers they cannot edit, showing Photoshop's stored pixels, and tell the user once, per category, what changed. GIMP's compatibility notice and Fresco's read-only Photoshop objects are the models. Capy can do the same with existing record types.

---

## 2. What real PSDs contain

### 2.1 Fallback pixels

Every PSD layer stores pixels, but not all of them can stand in for the layer:

| Layer | Stored pixels | Usable as fallback |
|---|---|---|
| Pixel layer | Exact | Yes. Effects, mask, opacity and blend are never baked in |
| Text, shape layer | Photoshop's rendering | Usually. Some writers (GIMP, ag-psd) store none for text; a feathered vector mask leaves a shape's fill unrendered beyond the path |
| Fill layer without a vector mask | Often an empty rectangle | Rarely: the fill must be computed |
| Smart object | Transformed and filtered, **without** the filter mask | Yes, with care. The unfiltered source and the filter mask are in the global `FEid` block |
| Adjustment layer | None ("undefined, probably all white") | No: the adjustment must be computed |
| Layer styles | Not stored anywhere as pixels | No: they must be rendered |
| Merged composite | The whole document, only when Maximize Compatibility was on (resource 1057) | Only for a flat import. At least one third-party writer saved a wrong one |

So an importer can keep the content of most layers from stored pixels. Adjustments, styles, fill layers and text from some writers need a renderer, or the whole file falls back to its flat composite.

### 2.2 By writer

| Writer | What its PSDs carry | What is lost at the source |
|---|---|---|
| Photoshop | Everything in the inventory (report 1) | — |
| Photopea | Its own documents are PSDs, so probably the richest files after Photoshop's (inferred) | Knockout; CMYK is only a preview; styles are approximations |
| Illustrator (Export As PSD) | With Maximum Editability, top-level layers as groups and live opacity and blend modes; horizontal and vertical point type as type; compound shapes as shape layers only in a top-level layer with matching appearance | Gradients, effects, appearance stacks, clipping paths and live-shape parameters become pixels. Anything nested deeper than 5 levels is merged |
| Fresco | Pixel layers and blend modes; Brightness/Contrast, Hue/Saturation and Color Balance as adjustment layers. Groups, masks and clipping probably in Photoshop's encoding (unverified) | Probably live oil and watercolor state and reference layers (inferred). Vector layers arrive as smart objects or pixels (reports conflict) |
| Procreate | Layers, names, opacity, visibility, blend modes; probably clipping | Text probably rasterized; reference layers. Its global layer-mask block is malformed |
| Clip Studio Paint | Groups including "Through", masks with density and feather, clipping including on folders, Levels as an adjustment. Text as pixels, editable type, or both | Vector layers, fill layers, tones, frame borders, 3D and paper as pixels. Glow Dodge and Add (Glow) as `div `/`lddg` with Transparency Shapes Layer off |
| Krita | Groups, pass-through, shape layers (live data for rectangles and ellipses), basic text, fill layers, one transparency mask per layer | Clipping (always written as off), filter, clone and file layers, Krita-only blend modes |
| Affinity | Per-feature choice: rasterize or keep editable. Simple solid shapes as shape layers | Text is rasterized; live filters. With "Rasterize: Nothing", unsupported content is left out |
| SAI 2 | Layers, folders, opacity, modes, clipping | Text, shapes and linework as pixels. Its normal modes need Transparency Shapes Layer off |
| Sketchbook, Infinite Painter, Painter, Rebelle, ArtRage, Concepts | Layers and most blend modes; Painter drops groups; Concepts writes pixels only | App-only blend modes become Normal; paint simulation state |

**Quirks every importer meets**
- **The composite.** Rebuild it from the layers rather than trusting the stored one.
- **Pass-through** is stored in the group's `lsct` divider, not in its layer record.
- **Clipped folders.** Clip Studio clips folders, which Photoshop does not render.
- **Names.** Some names are in Shift-JIS; prefer `luni`.
- **Malformed blocks.** Procreate, Clip Studio and others write blocks that must be parsed defensively.
- **Transparency Shapes Layer off** (`tsly` = 0) changes how layer transparency enters the blend.

---

## 3. How other importers handle PSD

| Feature | Best handling found | Typical handling |
|---|---|---|
| Groups, pass-through | Affinity, Krita, GIMP: exact. Photopea keeps groups; its pass-through is unverified | Paint.NET flattens the tree; Painter drops groups |
| Clipping | Photoshop on iPad; Rebelle | GIMP emulates it with groups; Krita drops it (bug 350114, still open). Photopea is undocumented |
| Layer masks | Photopea keeps density and feather | Corel bakes density and feather; Paint.NET multiplies masks into alpha |
| Vector masks | Photopea, Krita 5.3 | GIMP rasterizes them |
| Fill opacity | Not documented by any importer; Affinity has a native fill control | Unverified, or dropped (Paint.NET) |
| Blend If, knockout | Photopea: Blend If only | Dropped everywhere; Photopea's developer declined knockout as undocumented |
| Adjustment layers | Photopea; Affinity and Pixelmator for some types | Krita drops them; GIMP shows empty layers; Clip Studio misreads values |
| Layer styles | Photopea, Affinity, Pixelmator, Krita: approximate | GIMP 3.2 keeps drop shadow, inner shadow and outer glow as filters; Clip Studio, Painter and Paint.NET drop them |
| Shape layers | Photopea with live-shape parameters; Pixelmator; Krita 5.3 | Rasterized |
| Text | Photopea (keeps the original font name when substituting), Pixelmator, PHOTO-PAINT; Krita and Affinity offer editable or pixels | Rasterized (Clip Studio, Procreate, Sketchbook, GIMP 3.2) |
| Smart objects | Photopea: embedded editable, smart filters partly; Affinity: optional embedded document | Rasterized; Fresco and Photoshop on iPad keep them read-only |
| CMYK, Lab | Krita and Corel keep them native | Converted to RGB (Clip Studio, Photopea, Painter, Elements) |
| Layer comps | Photopea; Illustrator picks one at import | Dropped |

**Patterns worth copying**
1. **Keep, don't drop.**
   - Fresco groups unsupported layers into a read-only "Photoshop preview object" with their pixels; Photoshop on iPad keeps smart objects it cannot edit.
   - GIMP stopped dropping unsupported layers in 2024, because dropping them broke clipping groups.
2. **One notice, by category, with the outcome.** GIMP's Compatibility Notice says, for example: "Adjustment layers are partially supported. Unsupported adjustment layers will show up as empty layers." It also marks each blend mode as exact or approximate.
3. **The fallback ladder.** From best to worst:
   1. exact mapping;
   2. approximate mapping with a warning;
   3. a fixed, non-editable conversion;
   4. stored pixels;
   5. a named placeholder that keeps the stack;
   6. silently dropping the feature;
   7. the composite only.
4. **Round-trip data.** GIMP keeps unknown document resources and duotone data for re-export. Among editors, none was found keeping unknown *layer* blocks. Libraries do: photocraft-psd keeps unknown blocks byte-exact, PhotoCraft's importer keeps unreadable adjustments verbatim, and ImageMagick keeps extra blocks as raw data.
5. **Limits first.** Check dimensions, layer count and decoded bytes before decoding anything. Mobile apps cap layers by RAM, for example Procreate's 11 layers at 8192² on an 8 GB iPad.

---

## 4. Capability map

Columns:
- **Today:** what the code does now.
- **Slot:** where the feature would live, or "none".
- **Class:**
  - **Have:** supported now.
  - **Planned:** a documented slot.
  - **New:** additive vocabulary without a design yet.
  - **Decide:** see section 5.
  - **Out:** not promised; rasterize or drop with notice.

Evidence for every row is in report 5.

### 4.1 Document

| Capability | Today | Slot | Class |
|---|---|---|---|
| RGB 8/16/32-bit | Working RGB spaces; U8, U16, F16, F32 storage; float requires Linear blending | — | Have. Map Photoshop's 0–32768 16-bit range |
| Grayscale, Bitmap | Grayscale and two-tone paint layers in an RGB document | — | Have, as layer modes |
| CMYK, Lab | RGB documents only; image sources can be CMYK. An importer would convert to RGB | Typed color contracts and an output-profile record ([extensions §4.9](object-content-extensions-research.md#49-document-level-additions)) | Planned. Native editing needs a new compositor |
| Duotone, Indexed, Multichannel | — | None | Out: convert, and keep duotone curves only if export ever needs them |
| ICC | Built-in working spaces; image source profiles | A custom working profile would be a new composition field | New |
| Size | 32,768 px per side, 4,096 occurrences, per-source tile caps | — | Have for PSD (30,000 px). PSB beyond that: refuse or downscale at admission |
| Resolution | Exact rational per axis | — | Have |

### 4.2 Structure

| Capability | Today | Slot | Class |
|---|---|---|---|
| Groups, pass-through | Isolated and pass-through stacks | — | Have |
| Clipping onto layers and groups | Bases: paint, image and isolated groups. Groups can clip, so Clip Studio's clipped folders also fit | — | Have |
| Clipped adjustment acting on the run | Unsupported: an adjustment cannot clip and ends a run | A later definition of `clip` on adjustments | Decide (5.3), then New |
| Fill layer as a clipping base | Generators cannot be bases | Same combination rule | New |
| Artboards | One composition and one output are editable; more open read-only | Additional outputs with a region, or one composition per artboard | Planned |
| Frames (Frame tool) | — | Occurrence `frame` naming a vector object | Planned |
| Locks: transparency, all | `alpha_locked`, `locked` | — | Have |
| Locks: pixels, position, artboard | — | Copy-safe ancillary record | Decide (5.2) |
| Color labels, linked layers | — | Copy-safe ancillary record | Decide (5.2) |
| Names, visibility, layer IDs | Names and visibility kept; IDs are Capy's own | Photoshop IDs only if export needs them | Have |

### 4.3 Blending

| Capability | Today | Slot | Class |
|---|---|---|---|
| 24 of Photoshop's 27 modes | Same names; Linear Dodge is `add` | — | Have; numeric parity unchecked |
| Darker Color, Lighter Color | — | Blend names ([extensions §4.8](object-content-extensions-research.md#48-layer-and-occurrence-additions)) | Planned |
| Dissolve | — | A new blend name with a fixed noise definition | New |
| Photoshop's 32-bit Soft Light | Float documents use W3C Soft Light | A new blend name | New |
| Fill opacity | One `opacity` | Occurrence `fill_opacity` | Planned. Its rule for the eight special modes is fixed when it ships |
| Blend If | — | An occurrence field; its tonal domain is fixed when it ships | New |
| Knockout, Blend Clipped Layers as Group off, Blend Interior Effects, Mask Hides Effects | Fixed to Photoshop's defaults | Flags whose absence means today's behaviour | New |
| Transparency Shapes Layer off | Fixed on | Same kind of flag, or Clip Studio's glow modes as blend names | New; needed for comic files (6) |
| Channel restrictions | — | — | Out |

### 4.4 Masks

| Capability | Today | Slot | Class |
|---|---|---|---|
| Raster mask: enabled, linked, inverted, offset, default outside | One `mask` slot, 8 or 16 bit | — | Have |
| Density, feather | — | Mask-use fields (photo roadmap LYR-4) | Planned |
| Vector mask | — | A separate field naming a vector object. The `mask` slot cannot hold both kinds, and Photoshop allows both on one layer | Planned; choose the field (6) |
| Shared smart-filter mask | Each attached effect has its own mask | Wrapper hypothesis in the [earlier record](psd-and-layer-styles-research.md#a-possible-shared-smart-filter-mask-mapping) | New |

### 4.5 Adjustments and fill layers

All of Photoshop's 16 adjustment types have a Capy filter of the same purpose, and adjustment layers become effect occurrences with opacity, blend and mask. Report 5 has the full table.

| Gap | Class |
|---|---|
| Curves: Capy interpolates with monotone Hermite, Photoshop with its own spline | New choice; until then, approximate |
| Hue/Saturation ranges: center, width and feather instead of Photoshop's four sliders | Conversion, approximate at the edges |
| Brightness/Contrast Legacy | New choice |
| Gradient Map and Gradient Fill: midpoints, noise gradients, dither, reverse and separate opacity stops (Capy interpolates premultiplied stops). Photoshop's Perceptual, Linear, Classic and Smooth methods (report 1) need checking against Capy's interpolation | New data version with a converter (allowed); new choices |
| Gradient Fill styles Angle and Diamond | New choices |
| Color Lookup: 3DL and CSP formats, device-link and abstract profiles | Convert to Capy's 3D LUT; profiles are New |
| Clarity, Dehaze and Grain (Photoshop 27.3) | Unknown encoding; Out until documented |
| Pattern fill | Planned pattern generator using image inputs for effects |

### 4.6 Layer styles

| Capability | Today | Slot | Class |
|---|---|---|---|
| Drop and inner shadow, outer and inner glow, bevel and emboss, satin, color, gradient and pattern overlays, stroke; up to 10 instances of five kinds; global light; style scale | None. Attached effects run in sequence and their alpha feeds clipping, which is wrong for styles | Owner-held styles ([earlier record](psd-and-layer-styles-research.md#recommended-direction)); built-in effect IDs | Planned. Where styles sit relative to mask, clipping, fill opacity and knockout is decided when they ship |

### 4.7 Vector content

| Capability | Today | Slot | Class |
|---|---|---|---|
| Shape layers: path, solid, gradient or pattern fill, stroke | — | A vector layer ([vector layers §7.2–7.4](vector-layers-research.md#72-layer-model)) | Planned |
| Per-subpath combine, subtract, intersect, exclude | — | A boolean recipe with its result computed at import, which needs a Bézier boolean engine | Planned |
| Live shapes: rectangle with four radii, ellipse, line, polygon and star | — | Live-shape recipes (V1); the encoding of Photoshop's polygon and star is undocumented | Planned |
| Stroke alignment, dashes, caps, joins, miter limit | Centered strokes in V0 | Stroke alignment and dashes are later stroke properties | Planned |
| Gradient and pattern strokes | — | Paint kinds | Planned |
| Saved paths, work path, clipping path | — | None. Hidden vector layers, or a paths record | New; choose when import ships |

### 4.8 Text

| Capability | Today | Slot | Class |
|---|---|---|---|
| Point and box text, style runs, paragraph settings, vertical text, text on a path | — | A required Text item in a vector object ([extensions §4.10](object-content-extensions-research.md#410-text)) | Planned |
| Warp text (15 styles) | — | Envelope recipe with stored result | Planned |
| Named character and paragraph styles | — | Document styles | Planned |
| Kinsoku and MojiKumi sets, tsume, hanging punctuation, composer choice | — | Paragraph properties | New; matters for manga lettering |
| Anti-alias modes (sharp, crisp, strong, smooth) | — | A text property, or approximate | New |
| Faux bold and italic, OpenType flags, tracking, kerning, baseline shift | — | Style-run properties | Planned |
| Fonts | — | Font descriptor; Photoshop names fonts only. Keep the original name when substituting, as Photopea does | Planned |

### 4.9 Smart objects

| Capability | Today | Slot | Class |
|---|---|---|---|
| Embedded raster | Image object with an F64 affine | — | Have |
| Perspective (four-corner transform) and warp mesh | Affine only for objects; mesh warp exists for paint | A required image property, reusing the paint warp's Bézier mesh | Planned |
| Embedded PSD, PSB, AI or PDF | — | Placed document with a cached raster | Planned |
| Linked files | — | Embed at import | Out by policy |
| Smart filters with per-filter blend and opacity | Attached adjustments in order, each with blend, opacity and mask | — | Have, except the shared mask (4.4) |
| Smart filters Capy lacks (Liquify, Camera Raw, Puppet Warp…) | — | Bake from stored pixels | Out |

### 4.10 Other data

| Capability | Today | Slot | Class |
|---|---|---|---|
| Alpha channels | Saved selections, contour or 4/8-bit coverage | 16-bit coverage is a new depth value | Have; New for 16-bit |
| Spot channels | — | Spot colors (4.1) | Planned |
| Guides | Straight rulers | — | Have |
| Layer comps | — | A document record; an ancillary record fits | Planned |
| Slices, notes, color samplers | — | Slices are planned records; notes fit an ancillary type | Planned / New |
| Frame animation, video timeline, 3D | Effect phases only | — | Out: import the current frame |
| XMP, EXIF, IPTC | Kept byte-exact | — | Have |
| Content Credentials, generative-layer metadata | — | Records, if provenance is ever kept | New |
| Unknown PSD blocks | No contract | Not needed (5.2) | Out |

---

## 5. Decisions before the format freeze

The [format freeze](../development/capy-format.md) fixes how readers treat data they do not know. These three questions depend on that, so they cannot wait for the features themselves.

### 5.1 Whole-document read-only

- **The problem.**
  - Today any unknown required field, type or value makes a reader open the whole document read-only (Preserved).
  - Nearly every PSD feature in section 4 marked Planned or New arrives as required data: styles, fill opacity, vector masks, text, Blend If, knockout, warped images and placed documents.
  - So every such file opens read-only in every version released before the feature. A whole comic page becomes read-only because one layer has a drop shadow.
  - The same question is open for text items as [vector layers open question 11](vector-layers-research.md#9-open-questions).
- **Options:**
  1. **Keep whole-document Preserved.** Simple, and correct about what an older version can show. It relies on people updating, and the tablets and desktops a person uses may run different versions.
  2. **Layer-local Preserved.** An occurrence that a reader does not fully understand is shown locked, from a fallback image stored with it, and the rest of the document stays editable.
     - This needs an optional fallback-image field that frozen readers already know, and a rule that an edit elsewhere keeps the locked layer byte-exact.
     - Writers would store the fallback only for layers that use post-freeze features.
     - It conflicts with the rule that object-layer content keeps no saved raster cache, and must state when the fallback goes stale.
- **Recommendation:** decide before M3. Option 2 is the only way to give PSD imports with styles and text a graceful path in older versions. If it is chosen, the fallback field must ship in the first frozen reader.

### 5.2 Presentation-only properties

- **The problem:** unknown fields in a required record make the document read-only. A color label or position lock added later must not do that to every older reader.
- **Policy:** presentation and editing aids go in copy-safe ancillary records that name their occurrence. That covers color labels, pixel, position and artboard locks, linked-layer groups, layer comps, notes and Photoshop layer IDs.
  - Older readers keep these records while the occurrence exists, and ignore them for rendering.
  - They lose them on Duplicate, which is acceptable.
- **No format change.** The existing ancillary rules support this. It only needs stating, so that nobody adds such properties as occurrence fields.
- **Recipes over a layer's content.** Some later records would describe a layer's content, such as a baked style kept beside its pixels or a smart object's source. The package contract says copy-safe records may depend only on the identity of what they reference. Such records can still be kept if they carry a fingerprint of the content they describe, as vector recipes do. A reader that knows the type then drops a stale one. The contract needs that convention written down when the first such record ships; frozen readers need nothing new.

### 5.3 Clipped adjustments

- **The conflict.**
  - The [format plan](../development/capy-format.md) lists "clipped adjustments operate on the clipping stack" among the semantics to freeze.
  - The scene index instead rejects an adjustment with clip attachment as unsupported (`crates/layer-core/src/authored/scene.rs:54`).
- **Photoshop's meaning:** a clipped adjustment acts on the composite of its clipping run (base plus earlier clipped layers), inside the base's alpha.
- **Capy's nearest form** is an adjustment attached to one owner, which acts before the clipped members are combined. That is a different meaning.
- **Recommendation:** keep the code's behaviour, so a later version can give `clip` on adjustments Photoshop's meaning, and correct the plan before the freeze.

---

## 6. Gaps by impact on real files

| # | Gap | Who hits it | Simplest addition |
|---|---|---|---|
| 1 | Layer styles | Designers and comic letterers: stroke, drop shadow and color overlay are common (report 1) | Owner-held styles with their own compositor ([earlier record](psd-and-layer-styles-research.md)); until then, import the layer without them and say so |
| 2 | Clipped adjustments on clipping runs | Anyone clipping a Curves or Hue/Saturation adjustment to a character or object, a common use of clipping (inferred) | Define `clip` on adjustments as acting on the run composite within the base's coverage |
| 3 | Fill opacity | Anyone using styles; glow and dodge layers | `fill_opacity` on occurrences, with the eight-mode rule |
| 4 | Text | Letterers, designers, manga | The planned Text item, plus CJK composition and anti-alias properties; stored pixels until then |
| 5 | Shape layers, vector masks | Designers, logo work | Vector layers (V1–V2), a vector-mask field, a Bézier boolean engine for subpath operations |
| 6 | Transparency Shapes Layer off | Clip Studio comic artists using Glow Dodge and Add (Glow); SAI files | A flag for the off case, or Clip Studio's glow modes as blend names that the importer selects |
| 7 | Adjustment parity | Photographers, colorists | New choices for Photoshop's Curves spline, Legacy contrast and gradient interpolation; a gradient data version with midpoints and opacity stops |
| 8 | Smart objects with perspective or warp | Mockups, design | The planned non-affine image property; stored pixels until then |
| 9 | CMYK documents | Print and comic print | Convert to RGB on import with a notice. Native CMYK is a separate compositor project |
| 10 | Mask density and feather | Retouchers | Mask-use fields |
| 11 | Blend If | Retouchers, photographers | An occurrence field with a defined tonal domain |
| 12 | Artboards | UI and product designers | Output regions; until then, one group per artboard on one canvas |
| 13 | Pattern fills, overlays and strokes | Designers | Pattern generator and pattern paint |
| 14 | Dissolve, Darker and Lighter Color | Occasional | Blend names |
| 15 | Saved paths, layer comps, labels, locks, slices | Occasional | Section 5.2 records; a paths record |

Out of scope by design:
- knockout and channel restrictions (rare; flags are possible later);
- linked files;
- duotone, indexed and multichannel editing;
- timeline, video and 3D;
- Photoshop-only smart filters.

The importer rasterizes or drops these with a notice.

---

## 7. Illustrator and Fresco

**Illustrator**
- **PSD is a lossy route.** Export As PSD rasterizes gradients, effects, appearance stacks, clipping paths and live shapes, and keeps only point type and some compound shapes editable.
- **PDF should keep the vector art.** An `.ai` saved with "Create PDF Compatible File" contains a full PDF copy, and PDF export can keep top-level layers. Reading that copy should give paths, gradients, transparency groups, soft masks and clipping, with effects and symbols already expanded (inferred from the PDF model, not tested).
- **Mapping.** The [extension research](object-content-extensions-research.md) maps PDF's model, including knockout groups, soft masks, shadings, patterns, spot colors and overprint, onto Capy's planned slots.
- **What's missing is the importer.** The [vector phasing](vector-layers-research.md#8-phasing) has SVG import in V2, and §7.8 there says only that PDF and AI import "come later" (hayro). Scheduling a PDF-content importer after V2 is the way to bring Illustrator artwork across.

**Fresco**
- **What carries over.** Its PSD export carries pixel layers and Photoshop's blend modes. Fresco has groups, layer masks and clipping masks, probably written in Photoshop's own encoding (unverified). Its three adjustment types (Brightness/Contrast, Hue/Saturation, Color Balance) round-trip to filters Capy has.
- **What probably doesn't** (inferred): live oil and watercolor become plain pixels, and reference layers have no PSD equivalent.
- **Vector layers.** User reports conflict on whether they arrive as smart objects or pixels, and recent reports describe wrong scale and position. PDF export keeps them as editable shapes, so the PDF route applies to Fresco vectors too.
- **Fresco's own format** is a cloud document with no public specification.
- **To settle:** test files from current Fresco versions.

---

## 8. Import design

**Placement.** The import pipeline in the [earlier record](psd-and-layer-styles-research.md#import-into-the-capy-format) stands: bounded parsing off the input path, shared conversion, then admission before the document replaces anything. The rules below add fallback and notice behaviour.

**Fallback rules**
1. **Never drop a layer.** A layer Capy cannot edit becomes an image layer of its stored pixels, under its own name, in its own place in the stack. Clipping runs and masks stay intact around it.
2. **Adjustments.** One Capy cannot map becomes a hidden placeholder that keeps its place, its name and its mask.
3. **Styles.** Until styles exist, a layer's styles are left out and the notice names the layer.
4. **The composite** is used only when a file has no usable layers, never to repair a partial import.
5. **Text and smart objects.** Import editable when Capy can do it faithfully; otherwise use stored pixels.
   - For text, that means its fonts are present and its features are supported.
   - Decide automatically rather than asking, as AGENTS.md prefers better behaviour to settings. Keep the original font names, so a later re-import can do better.
6. **Blend modes.** One without an exact Capy equivalent maps to the nearest mode, and the notice marks it approximate, as GIMP does.

**One notice.** After import, one message lists each category that was approximated, rasterized or left out, with the affected layer count, in plain words. Shared Rust owns its text, in the Fluent catalogs.

**Parsing rules**
- **Limits:** check dimensions, layer counts and decoded sizes before decoding.
- **Structure:**
  - read Unicode names;
  - take pass-through from the section divider;
  - honour Clip Studio's clipped folders.
- **Defensive reads:** tolerate known malformed blocks.
- **Composite:** rebuild it from the layers.

**Libraries and test files**
- **The reader:** `photocraft-psd` (MIT or Apache-2.0, Rust) is the only permissive crate found that covers PSB, 16- and 32-bit, CMYK and raw-block retention. It is weeks old and alpha.
- **Semantic references:** psd-tools and ag-psd (both MIT). Krita and GIMP are GPL, for behavioural study only.
- **Test files:**
  - photocraft-corpus: 258 Photoshop-rendered files, MIT or Apache-2.0;
  - the psd-tools and ag-psd test sets.
- **Still needed:** sample files written by Fresco, Procreate, Clip Studio Paint, SAI, ibisPaint and MediBang. They must be made by the team or licensed for testing.

---

## 9. Docs that disagree with the code

Found during the audit and not changed here:
- **Clipped adjustments.** [`docs/development/capy-format.md`](../development/capy-format.md) (lines 185–186) says clipped adjustments operate on the clipping stack; `crates/layer-core/src/authored/scene.rs:54` rejects them (section 5.3).
- **Generators as clipping bases.** [The extension format audit](object-content-extensions/01-capy-format-audit.md) §4 lists generators as clipping bases; `authored/scene.rs:45` makes them ineligible.
- **Groups as clipping bases.** [The layers audit](photo-editing-research/08-audit-layers.md) says groups cannot be clipping bases; isolated groups now can.

---

## 10. Not verified

- **Encodings unknown:**
  - how Fresco and Procreate encode vector layers, masks, clipping and text;
  - ibisPaint, MediBang and Affinity 3 output.
- **Undocumented PSD storage:** generative variations, Dynamic Text, the 27.3 adjustments, variable-font axes, Photoshop's polygon and star live shapes, and `frgb` frame types.
- **Parity:** no blend mode or adjustment has been compared numerically with Photoshop, and Capy's accepted parameter ranges have not been checked against Photoshop's.
- **Photoshop behaviour:** its layer limit, and how a base layer's styles interact with clipped layers.
- **Rows from search extracts:** many Pixelmator, Clip Studio, Corel, Rebelle and Elements rows rest on search extracts or old help pages (marked in reports 3 and 4).
