# Photoshop PSD capabilities in Capy's model, format and plans

[PSD import compatibility](../psd-import-compatibility-research.md) · source report, 2026-10-16

Code and design audit at `af7a22a6a` for the [PSD import compatibility record](../psd-import-compatibility-research.md). File:line references apply to that baseline.

Read-only audit of the repository at `af7a22a6a`; it edited, built and ran nothing.

**Abbreviations.** Code paths are under `crates/layer-core/src/` unless shown otherwise.
- **AM**: `docs/reference/authored-model.md`
- **PKG**: `docs/reference/capy-package.md`
- **PSDR**: `docs/history/psd-and-layer-styles-research.md`
- **EXT**: `docs/history/object-content-extensions-research.md`
- **PAINT**: `docs/history/object-content-extensions/08-painting-app-content.md`
- **VEC**: `docs/history/vector-layers-research.md`
- **GA**: `docs/history/object-layer-ga-design.md`
- **ROAD**: `docs/development/photo-editing-roadmap.md`
- **FMT**: `docs/development/capy-format.md`

**Terms in the last column.**
- **Additive**: a new record type, field, value or choice. The contract allows this (PKG:66-69, 162-172). Older readers open such a file as Preserved, which makes the whole document read-only.
- **Combination**: existing values used together in a way that is read as Unsupported today. A later version may define it (PKG:166-171).
- **Redesign**: changes the meaning of an existing field or the envelope.

**No capability needs a redesign.** Two things still need decisions:
- non-RGB document modes need a new compositor, even though the format change is additive;
- a few reader policies matter only if they are fixed before the freeze (see the last section).

## Document

| Capability | Today (evidence) | Planned slot | Later addition needs |
|---|---|---|---|
| Color modes | RGB only. Composition `color.space` is srgb, display_p3, adobe_rgb or pro_photo (`color/rgb.rs:6-12`, `package/values.rs:102-106`). AM:101 defines it as "Working RGB primaries". Each paint layer has a `color_mode`: full_color, grayscale or two_tone, stored as gray+alpha (`color.rs:22`, PKG:123-128). Images keep their own Gray/RGB/CMYK data and ICC profile (`color/source.rs:43-49`), but CMYK has no alpha (`source.rs:59-61`). Palettes are forbidden (PKG:453). The renderer is RGB-only (rendering.md:317). | CMYK, Lab, spot colors and overprint: "typed color contracts and an output-profile record" (EXT:370, GA:1721). FMT:33 excludes a universal channel model. Grayscale maps to an RGB document with grayscale layers, Bitmap to two-tone layers. Duotone, Indexed and Multichannel have no slot. | Additive: a new composition field or type, never a new `space` value. It also needs a new compositing evaluator, which is substantial work. |
| Bit depth | U8, U16, F16, F32 (`color/profile.rs:14-22`). Float requires Linear blending (`package/artwork_records.rs:337`). Photoshop's 16-bit range (0–32768) needs mapping on import. No 1-bit depth. | None | 1-bit would be a new depth value (additive). Two-tone layers are the current approximation. |
| Canvas size vs PSB | `MAX_EXTENT` is 32,768 (`lib.rs:110`, `project.rs:78`). Wire sizes are u32, and an oversize composition is Unsupported, not Invalid (`package/artwork_records.rs:334`). Each source is also capped at 16,384 tiles, raster at 1 GiB and assets at 512 MiB (`project.rs:74-77`). PSD's 30,000 px fits; PSB's 300,000 px does not. | None; PSDR:114 only notes the ceiling. | Admission and renderer work only; no format change. |
| ICC | The working space must be a built-in RGB space. Other profiles live only on images (`source_profile` policy) and on output proofs (PKG:440, AM:107, AM:180-187). Untouched originals convert relative colorimetric without black-point compensation (PKG:394). | None | A custom working profile would be a new composition field (additive). |
| Resolution / DPI | Exact rational per axis, in inch, cm or m (`package/values.rs:158-166`, AM:103). PSD's 16.16 fixed-point values fit exactly. | — | Supported. |

## Structure

| Capability | Today | Planned slot | Later addition needs |
|---|---|---|---|
| Groups | Isolated or `pass_through`; pass-through is allowed only on stacks (`package/artwork_records.rs:403`). The pass-through formula is at AM:339-343. | — | Supported. |
| Clipping runs | The base is the first unclipped paint layer, image layer or isolated group below (`authored/scene.rs:40-63`, :45).<br>- Groups can clip and can be bases.<br>- Generators (fill layers) can be clipped but cannot be bases.<br>- Pass-through groups can do neither (:54).<br>- An adjustment cannot clip, and an unattached adjustment ends the run (:54-55). It can only attach as `effect` to one owner, which may be a clipped member.<br>So Photoshop's clipped adjustment, which acts on the run below it, has no direct form. | No slot; PSDR:104 records the gap. FMT:185-186 says "clipped adjustments operate on the clipping stack", which contradicts the code. | Combination: these relationships are read as Unsupported (`package/artwork_records.rs:477`), so a later version can define `clip` on adjustments and generator bases. Until then, restructuring into isolated groups is an untested workaround. |
| Artboards | The editable subset allows exactly one composition and one output (`authored/topology.rs:164-168`, AM:313,326-329), so a second output opens Preserved. `capy.output/2` has no region or background; export owns delivery size (PKG:108). Outputs cannot act as artboards today. | "Additional outputs" (EXT:361, GA:1720). PAINT:381 routes artboard clipping to `frame`. | Additive: an output region, or one composition per artboard. |
| Frames (Frame tool) | None | Occurrence field `frame` naming a vector object (EXT:349, PAINT:349,381). Depends on vector objects. The PSD encoding of frames is unverified (PAINT:103,424). | Additive |
| Locks | `locked` is a lock-all: it blocks paint, move and delete and is inherited from ancestors (`layers.rs:640-651`). `alpha_locked` is the transparency lock, for paint only (`package/artwork_records.rs:410`). No position-only, pixels-only or artboard lock. | None | Additive field, but see freeze item F2. |
| Color labels | None; the occurrence field set is closed (`package/registry.rs:33`). | None | Additive field or ancillary record |
| Linked layers | None. AM:48 "linked duplicate" means a shared source, which is unrelated. | None | Additive |
| Layer IDs | Random 128-bit portable IDs (PKG:28-33). Photoshop's `lyid` is not retained. | — | Keep it in an ancillary record if needed. |
| Hidden layers | `visible` (`authored/artwork.rs:59`); hidden content is kept (AM:74). | — | Supported |
| Layer count | 4,096 occurrences, counting groups, effects and selections (`project.rs:79,110`). Also 65,536 records and depth 128, which allows about 62 nested groups (01-capy-format-audit §3). Photoshop's 8,000-layer limit is from memory, not verified. | — | Admission only |

## Blending

| Capability | Today | Planned slot | Later addition needs |
|---|---|---|---|
| Blend modes | 24 modes plus `pass_through` (`layers.rs:162-192`, `package/values.rs:126`). Linear Dodge is `add`. Of Photoshop's 27, three are missing: Dissolve, Darker Color and Lighter Color. Soft Light uses Photoshop's formula only in Perceptual (8/16-bit) documents. Float documents use the W3C formula and do not offer modes defined only on [0, 1] (rendering.md:392-414). | Darker and Lighter Color: EXT:354. Dissolve: no slot; PSDR:110 only notes it is absent. | New blend names (additive). Dissolve needs a fixed noise definition. Photoshop's 32-bit Soft Light would need a new name. |
| Fill opacity | None; there is one `opacity` (`authored/artwork.rs:60`). | Occurrence `fill_opacity`, "once layer styles exist" (EXT:351, PAINT:396). | Additive. Even without styles, Fill differs from Opacity for eight modes, so exact import needs it. |
| The eight special Fill modes | All eight modes exist as blend names; their Fill behavior is not modelled. | No slot | Must be fixed in the fill-opacity contract when it ships; it cannot change afterwards. |
| Blend If | None | ROAD:112 (LYR-3, M7), with no format design. Tonal-ramp math exists (photo-editing-research/08-audit-layers.md:152-170). | Additive field. Its domain (encoded gray or linear) must be fixed when it ships. |
| Knockout | None | Only as a vector Group property (EXT:232). PAINT:418 rules layer knockout out of scope. | Additive |
| Blend Clipped Layers as Group | Always on: the base's opacity and blend apply to the finished run (GA:797-804). | Out of scope (PAINT:418) | Additive flag, defaulting to the current behavior |
| Blend Interior Effects as Group | Not applicable without styles. | Out of scope (PAINT:418). PSDR:171-173 says styles need it. | Additive, with styles |
| Transparency Shapes Layer | Effectively on: effects follow alpha. | No slot | Additive |
| Layer/Vector Mask Hides Effects | Fixed order: the mask applies before attached effects (AM:347-349, layer-clipping-and-effects.md:18-23). This equals Photoshop's default (option off). | No slot | Additive |

## Masks

| Capability | Today | Planned slot | Later addition needs |
|---|---|---|---|
| Raster masks | One `mask` slot: a coverage source with `default_coverage`, used with enabled, linked, inverted and offset (`authored/artwork.rs:84-98`, `package/artwork_records.rs:405`). Depth is u8 or u16 (PKG:456-458). | — | Supported |
| Density, feather | None | ROAD:113 (LYR-4, M7) | Additive mask-use fields |
| Linked / disabled | Yes | — | Supported |
| Vector masks | None; out of GA scope (GA:122). | "A new mask field, or a known type in `mask.source`" (EXT:350). PAINT:346 notes Photoshop needs a raster and a vector mask on one layer. | Additive. The slot choice is still open; `mask.source` alone cannot hold both. |
| Masks on groups and adjustments | Yes, on any occurrence. | — | Supported |
| Several masks per layer | One slot | Only the `frame` and vector-mask fields | Additive |
| Filter masks | Each attached effect can have its own mask; there is no shared Smart Filter mask. | PSDR:200-222: a wrapper-group hypothesis, untested | Restructuring today; additive later |

## Adjustment and fill layers

Adjustment layers become effect occurrences with their own opacity, blend and mask. Line numbers are in `assets/filters/manifest.json`.

| Photoshop | Capy built-in (line) | Gaps |
|---|---|---|
| Brightness/Contrast | `brightness_contrast` (123) | No Legacy choice |
| Levels | `levels` (84), per channel | — |
| Curves | `curves` (24): RGB plus three channel curves, at most 32 points | Monotone Hermite interpolation (PKG:385), not Photoshop's spline |
| Exposure | `exposure` (469): exposure, offset, gamma | — |
| Vibrance | `vibrance` (504) | Adds a `protect_skin` toggle |
| Hue/Saturation | `hue_saturation` (151): master, six ranges, colorize | Ranges use center/width/feather instead of Photoshop's four sliders |
| Color Balance | `color_balance` (383) | — |
| Black & White | `black_white` (537), with tint | — |
| Photo Filter | `photo_filter` (261) | — |
| Channel Mixer | `channel_mixer` (334), with monochrome | — |
| Color Lookup | `color_lookup`: a 3D LUT of at most 65³ (PKG:443) | No ICC device-link or abstract profiles; 3DL/CSP need conversion |
| Invert | `invert` (210) | — |
| Posterize | `posterize` (631) | — |
| Threshold | `threshold` (225) | — |
| Gradient Map | `gradient_map` (601) | No midpoints, dither, reverse or noise |
| Selective Color | `selective_color` (279), relative/absolute | — |
| Solid fill | `solid_color` (2073), a generator | — |
| Gradient fill | `gradient_fill` (2123): linear, radial and reflected (2140), with angle, scale and center | Missing Angle and Diamond styles and noise gradients. No midpoints (`gradient.rs:4-14`). Stops interpolate premultiplied (PKG:391), unlike Photoshop's separate opacity stops. |
| Pattern fill | None. The paper `texture` is gray height data (PKG:362-367). | Planned: a "pattern fill generator ID" (PAINT:393), using the planned image inputs for effects (PKG:279-282). |

- **Additions:** new choices such as `angle` or `diamond` are additive (PKG:818-819). Midpoints and noise change the gradient value, so they need a parameter-data version with a converter (PKG:236-259); that is allowed.
- **Not verified:** numeric parity with Photoshop for any of these.

## Layer styles

- **Today:** no style record and no shadow, glow, stroke, satin or bevel built-in (none among the 54 filters; Emboss is not Bevel, PSDR:107).
  - Attached effects run one after another on the owner, and their alpha feeds the clipping shape (layer-clipping-and-effects.md:18-25).
  - That is wrong for styles: a shadow's alpha would clip the layers above, and there is no shared untouched source (PSDR:194-198).
- **Planned:**
  - PSDR:20-24 and :371-375 recommend styles owned by the layer, with stable IDs, category order, repeat order and global light.
  - EXT:352 and PAINT:395 say only "Effect IDs"; ROAD:124 places styles in M8 (LYR-8).
  - No wire design exists.
- **Later:** additive (a new field or attachment value plus new built-ins). Where style output sits relative to the mask, the clipping run, fill opacity and knockout is still a model decision.

## Vector

No vector record type is registered (`package/registry.rs:30-48`), and Figure paints pixels.

| Capability | Planned (VEC §7.2–7.4: `capy.vector-object/1` with `capy.vector-items/1` chunks) | Later addition needs |
|---|---|---|
| Shape layers | A vector layer with a boolean recipe. Importing needs a Bézier boolean engine (EXT:544, PAINT:383). | Additive new types |
| Live shapes | Live-shape recipes in V1 (VEC:589-592; `vogk` at PAINT:384) | Additive |
| Stroke alignment | Strokes are centered in V0. Inside/outside are "later stroke properties" (VEC:569), defined as twice the width clipped to the fill (EXT:274). | Additive; the definition is fixed when it ships |
| Dashes | V2; arrays in absolute units or stroke widths (EXT:271-273) | Additive |
| Gradient and pattern fills and strokes | Linear and radial in V2; other kinds, pattern paint and gradients on strokes later (EXT:254-267, :280) | Additive |
| Saved paths (image resources 2000–2998, clipping path) | **No slot.** Saved selections hold polygon contours only (PKG:326). | Needs a model decision |

## Text

- **Today:** none.
- **Planned (EXT §4.10, :377-413):** Text is a required leaf item in a vector object.
  - Point, area, on-path and threaded text (EXT:483); text on a path uses geometry the item owns.
  - Style runs with a style table, spans, and per-paragraph settings.
  - Writing mode on the item, which covers vertical text.
  - Font descriptor, an optional `capy.font/1` resource or a name-only form (PAINT:403), and a stored glyph result.
  - Warped text is an envelope recipe (EXT:315, PAINT:400); named styles come later (EXT:366).
- **Later:** additive. Older readers open files with text as Preserved. The text rules freeze with the first text release (EXT:380-382).

## Smart objects

| Capability | Today | Planned slot | Later addition needs |
|---|---|---|---|
| Embedded raster | Image object: an immutable image, an F64 affine and nearest or linear sampling (AM:197-212). Several objects can share one image. Only decoded pixels are kept, not the original file bytes. | — | Supported for pixels |
| Embedded layered document (PSB/AI) | None | "Placed document" with a cached raster (EXT:228); many-use definition plus Instance (PAINT:405, GA:1716) | Additive |
| Linked files | None | Excluded by policy; imports embed (EXT:222, :603; PAINT:415) | None |
| Warp / perspective | Affine only. Perspective and mesh exist only as destructive paint operations (GA:133-135). | "Required image property" (PAINT:404, GA:1713) | Additive |
| Smart filters | Attached adjustments on the image layer, bottom to top, each with its own opacity, blend and mask (AM:347-357). This matches Photoshop's order and per-filter blending. | The shared filter mask has only the PSDR:200-222 hypothesis. | Additive |

## Other

| Capability | Today | Planned slot | Later addition needs |
|---|---|---|---|
| Channels / saved selections | Saved-selection layers hold contours or u4/u8 pixel coverage (PKG:326, :439). Overlay color and opacity are working state and are not saved (AM:81-82). No 16-bit and no spot channels. | Channel selections: ROAD:111 (SEL-8). Spot colors: EXT:370. | Additive: a u16 depth value; new spot types |
| Guides | `capy.guides/1` holds straight, parallel and radial drawing rulers (`rulers.rs:9-22`). The Inkscape mapping turns guides into straight rulers (EXT:459). | VIEW-4 (ROAD:128) | New ruler kinds are new values |
| Slices | None | Records (EXT:368) | Additive |
| Layer comps | None | Document record (EXT:367, PAINT:406) | Additive; an ancillary record fits |
| Annotations | No defined type. An ancillary copy-safe note is the reference test fixture (PKG:786). | None | Additive ancillary type |
| Timeline / animation | Effect phases only, "not a seekable timeline" (AM:518). | Typed tracks (GA:1723); held cels (FMT extension cases); out of scope (PAINT:416) | Additive |
| Metadata / XMP | Exif, XMP and IPTC kept byte-exact (PKG:26, :441). A new kind would be an envelope binding, which old readers open as Preserved (`package/manifest.rs:271`). | — | Supported. New kinds should be records, not envelope keys. |
| Unknown imported data | Ancillary records with opaque resources. Copy-safe records survive while their references resolve, but may depend only on identity. Non-copy-safe records are dropped on any edited save (`authored/extensions.rs:119-122`, PKG:76-86). | PSDR:95-97 says this needs an authored-data contract. PAINT:409 notes a per-layer raster fallback risks stale caches. | Additive types, but no class keeps data while its layer is unchanged |

## No slot today: needs a model decision

**Decide before the format freeze.** Frozen readers will apply these rules to every later file.

- **F1. Whole-document Preserved (VEC:977, open question 11).** Every Photoshop-parity feature will land as a required field or type: fill opacity, styles, vector masks, text, Blend If, knockout, extra locks. A file using any one of them then opens read-only in every frozen reader. Opening just that layer read-only, with a stored fallback, would need a reader outcome that exists at the freeze.
- **F2. No ignorable fields in JSON records.** Any unknown field makes the document Preserved (PKG:162-164).
  - Cosmetic properties (color labels, link groups, position or pixel locks, notes, comps, guide colors) can avoid that only as ancillary records, which are lost on Duplicate.
  - Choose that policy, or add an ignorable-field convention to frozen readers. The second option conflicts with GA:127 ("do not reserve unused fields").
- **F3. Ancillary retention classes.** Today a record is either kept by identity or dropped on any edit. The vector framing's "keep while the subject is unchanged" rule, checked by fingerprint (VEC:674-679), has no equivalent here. It would let layer-level result-plus-recipe (baked styles, smart-object sources) and preserved PSD blocks survive unrelated edits in older readers.
- **F4. Clipped adjustments.** FMT:185-186 lists clipped adjustments under semantics to freeze, but the code makes them Unsupported (`authored/scene.rs:54-55`). Resolve this so the meaning of `clip` on an adjustment stays undefined for a later Photoshop-compatible definition.

**Decide when the feature ships:**
- styles: where they sit and which slot holds them;
- fill opacity and the eight special modes;
- which field holds vector masks, and the shared filter mask;
- artboards: an output region, or one composition per artboard;
- saved paths;
- Dissolve;
- the Blend If domain;
- non-RGB document modes, 16-bit and spot channels;
- position lock, color labels, linked layers, Photoshop layer IDs.

These are listed as out of scope today: knockout, Blend Clipped/Interior as Group and channel restrictions (PAINT:418), linked files, and the timeline.

## Docs that disagree with the code

- FMT:185-186 (clipped adjustments) disagrees with `authored/scene.rs:54-55`.
- `object-content-extensions/01-capy-format-audit.md` §4 lists generators as clipping bases. `authored/scene.rs:45` and :59-60 make them ineligible.
- `photo-editing-research/08-audit-layers.md` (other gaps, item 4) says groups cannot be clipping bases. Isolated groups now can.

## Not confirmed

- Numeric parity of any blend mode or adjustment with Photoshop, and whether Capy's accepted ranges cover Photoshop's (manifest min/max may be slider bounds only).
- Photoshop's 8,000-layer limit; clipping onto pass-through groups in Photoshop; the PSD encoding of frames; how a base layer's styles interact with clipped layers.
- Data versions: the manifest has no `version` key. PKG:203 says Gradient Map, Gradient Fill and Posterize use data version 2.
