# The `.capy` package and authored model: can they hold SVG-scale artwork?

[Object content extensions](../object-content-extensions-research.md) · source report, 2026-10-15 · baseline `37fabf996`

Every claim below was checked in code at the baseline. `pkg` means `crates/layer-core/src/package/` and `doc` means `docs/reference/capy-package.md`; other paths are relative to the repository root.

On 2026-10-16 the reader stopped treating well-formed relationships a later version may allow as damage. Known types in other slots and bindings are now unsupported rather than invalid; see [section 2.2 of the main record](../object-content-extensions-research.md#22-reader-classifications-fixed).


## 1. Package structure, references, sharing, root

- **Envelope.** `manifest.json` has `format:"capy.canvas"`, `version:1`, `document`, `root`, `objects[]`, `resources[]`, `outputs[]`, `default_output` and optional `metadata{exif,xmp,iptc}` (`pkg/manifest.rs:248-254`; `doc:13-26`).
  - IDs are 32 lowercase hex digits.
  - The document ID, object IDs and resource IDs must all differ (`manifest.rs:276-286`).
- **Registry.** 17 types (`pkg/registry.rs:30-48`):
  - 11 object types: `capy.composition/2`, `capy.stack/1`, `capy.occurrence/3`, `capy.paint-source/2`, `capy.image/1`, `capy.image-object/1`, `capy.coverage-source/2`, `capy.effect/2`, `capy.selection/1`, `capy.guides/1`, `capy.output/2`.
  - 6 resource types: `capy.raster-tile/1`, `capy.selection-coverage/1`, `capy.icc/1`, `capy.wgsl/1`, `capy.photo-metadata/1`, `capy.lut3d/1`.
  - `capy.image/1` is an object record, not a resource (`registry.rs:35`).
- **References.** The only form is `{"ref":"<id>"}`, and that object must have exactly that one key (`manifest.rs:35-39`).
  - References are discovered recursively, including inside unknown records (`pkg/json.rs:62-93`).
  - A dangling reference, or one pointing at the document ID, is Invalid (`manifest.rs:296`).
  - Evaluation endpoints are `{"object":{ref},"port":"color"}` (`manifest.rs:68-72`). `color` is the only implemented port (`artwork_records.rs:24-29`).
  - Binary payloads must not embed Capy IDs; they use local slots with visible bindings in JSON records (`doc:53-54`).
- **Sharing.**
  - Topology counts "uses" only for `composition.result` and for occurrence `content` and `mask` (`authored/topology.rs:120-148`).
  - Any count above 1 marks "Shared editable content", which opens as Preserved (`topology.rs:160-164`). Test: `codec/tests.rs:202` points occurrence 21 at paint 10.
  - An occurrence in two stacks is Invalid (`topology.rs:128`).
  - `capy.image/1` referenced from paint bases and image objects is not counted, so images are freely shared and stay editable. Test: `artwork_records.rs:586-605`, where two objects and a paint base share one image and one image record is written. LUT and other resources can also have many uses (`doc:316-317` in authored-model.md).
- **Document-level records.**
  - `capy.composition/2`: size, color, blend, resolution.
  - `capy.guides/1`: rulers. Nothing references it; it is just present in `objects`.
  - `capy.output/2`: listed in manifest `outputs`.
  - Photo metadata, through the manifest `metadata` field.
  - Not found in the package: palettes, swatches, grids, document title/author or other metadata. Palettes live in UI/presets (`crates/layer-ui/src/color/palette_view.rs`, `layer-core/src/presets.rs`).
- **Root.** The root must be a composition; otherwise the file is Preserved with "Unsupported authored root" (`topology.rs:85`).
  - The editable subset needs exactly 1 composition and 1 output (`topology.rs:166-170`).
- **Cross-layer references.** Only these known relationships exist:
  - occurrence → paint, image-object, stack, effect or selection, plus mask → coverage source;
  - paint and image-object → image;
  - effect values → image (texture) or LUT resource;
  - output → composition, and output context → effects (phases).
  - Content cannot reference another layer's occurrence, or the guides or output records. Effect image inputs are still "planned" (`doc:267-273`).
  - Ancillary records may reference artwork and resources but not other ancillary records (`manifest.rs:308-311`, which is Invalid).

## 2. Forward compatibility

The outcomes are `OpenOutcome::{Candidate, Preserved, RecoveredView, Failure}` (`pkg/codec.rs:20-25`).

| Case | Result | Code | Test |
|---|---|---|---|
| Unknown envelope field or version | Preserved, original bytes and preview only | `manifest.rs:252-253`, `codec.rs:191` | `codec/tests.rs:198` (version 2) |
| (a) Unknown non-ancillary record type, even unreferenced | Preserved ("Unknown authored type") | `topology.rs:155`, `artwork_records.rs:451` | `codec/tests.rs:199`; table at `doc:776` |
| (a) Unknown ancillary record | Editable; kept on edited save only if `copy_safe` and every reference still resolves | `pkg/../authored/extensions.rs:119-134` | `codec/tests.rs:449-480` |
| (b) Unknown field in a known record's `data`, or at record top level | Preserved | `manifest.rs:87,90`; `values.rs:20-26` returns Unsupported | `codec/tests.rs:200`; `artwork_records.rs:664-665` |
| (c) Unknown enum: blend, attachment, color mode, interpolation, built-in ID, data version, parameter key, choice | Preserved | `values.rs:121`, `artwork_records.rs:346,364,408`, `effect_records.rs:267-268,327,91` | `codec/tests.rs:201`; `effect_records.rs:470-473`; `artwork_records.rs:669-670` |
| (d) Unknown resource type or encoding referenced by artwork, or unreferenced | Preserved ("Unknown required resource") | `manifest.rs:149-179,327-332`; `resources.rs:294-301` | `codec/tests.rs:214-250`; `manifest/tests.rs:122-136` |
| (d) Unknown resource referenced only by an ancillary record | Editable; kept as an opaque resource | `extensions.rs:83-118` | `manifest/tests.rs:122-131` |
| Known resource type in the wrong binding | Invalid | `resources.rs:299` | `codec/tests.rs:244-249` (RecoveredView) |
| (e) `/2` suffix of a known type | An unknown type string, so Preserved; no version negotiation | `registry.rs:49` | `admission.rs:76-91` (`capy.occurrence/2`); `artwork_records.rs:623-624`; `codec/tests.rs:217-231` (`-future` suffix for every type) |
| Malformed known data, even inside an otherwise Preserved file | Invalid, giving Failure or RecoveredView | `codec.rs:215` | `codec/tests.rs:310-313` |

- **What Preserved means.** The whole document opens read-only; nothing is per layer.
  - No editable `Document` is constructed (`doc:600-612`).
  - UI capabilities are `edit:false, save:false, copy_original:true`, with export of the preview only (`crates/layer-ui/src/package_view.rs:29-31`).
  - A preview shows only if `preview.png` (at most 1,024 px, 8 MiB; `pkg/preview.rs:3-4`) matches the output's `representation` (`codec.rs:155-160`).
  - There is no fresh render (`doc:241-243`).

## 3. Admission limits

| Limit | Value | Where |
|---|---|---|
| Object records | 65,536 | `topology.rs:55`, `manifest.rs:241` |
| Evaluation edges | 262,144 | `topology.rs:55` |
| Evaluation depth | 128 | `topology.rs:55,184` |
| Resource records | 262,144 | `manifest.rs:10` |
| ZIP members | 262,144 | `codec.rs:180` |
| Manifest | 64 MiB | `manifest.rs:10` |
| JSON nodes | 4,194,304 | `manifest.rs:10` |
| JSON nesting | under 128 containers, whole manifest | `json.rs:7,53,68` |
| Occurrences | 4,096 | `project.rs:79,110` |
| Pixel extent | 32,768 | `lib.rs:110` |
| Tiles | 16,384 | `project.rs:77` |
| Raster | 1 GiB | `project.rs:76` |
| Assets | 512 MiB | `project.rs:75` |
| Total decoded resources | raster + asset budget | `resources.rs:282-285` |

- Exceeding any of these limits gives Preserved, not Invalid.
- Per-resource decoded bounds are checked before allocation (`resources.rs:322-336`):
  - ICC 16 MiB (`color/source.rs:8`);
  - photo metadata 64 MiB (`image_metadata.rs:20`);
  - WGSL 4 MiB (`resources.rs:398`);
  - LUT side 65 (`lut3d.rs:68`);
  - each tile is exactly 256² samples.
- Group nesting: 62 nested groups are editable and 64 are Preserved, because each group level adds two DAG nodes, occurrence plus stack (`admission.rs:47-56,114-131`).
- Strings:
  - layer names 4,096 bytes (`authored/objects.rs:211`; test `admission.rs:60-73`);
  - effect strings and LUT titles 256 (`effect_records.rs:29`; `resources.rs:406`);
  - proof names 1,024 (`values.rs:339`);
  - ZIP member names 255 (`archive.rs:77`).
- Other bounds:
  - occurrence and mask offsets are admitted only up to |2²⁴| (`offsets.rs:3,11-13`); beyond that is Preserved (`artwork_records.rs:620-621`);
  - 1,024 rulers (`rulers.rs:5`);
  - 32 gradient stops or curve points (`effect_records.rs:71,94`).

## 4. Layer model

- **Content.** `OccurrenceContent::{Paint, Objects, Stack, Effect, Selection}` (`authored/artwork.rs:47-49`) maps to `LayerKind::{Paint, Object, Group, Effect, Selection}` (`lib.rs:262-269`).
  - On the wire, `content` has exactly one key: `paint`, `objects`, `stack`, `effect` or `selection` (`artwork_records.rs:398-400`).
  - `objects` takes a single reference to an image object; an array is Invalid (`manifest.rs:110` → `reference()`).
- **Occurrence fields.** The field set is closed: `content, name, visible, locked, alpha_locked, reference, opacity, blend, attachment, offset, mask` (`registry.rs:33`; struct at `artwork.rs:56-68`).
  - There is no color label or tag field (not found).
  - Opacity is F32 in [0,1] (`artwork_records.rs:404`).
  - `alpha_locked` applies to paint only (`artwork_records.rs:410`).
  - Saved-selection occurrences are fixed: visible, opacity 1, normal blend, no mask or attachment (`lib.rs:1662-1667`).
- **Groups.** Blend `normal` (or any blend mode) is isolated; `pass_through` is allowed only on stacks (`artwork_records.rs:403`).
  - The pass-through formula is `B + opacity·mask·(group(B) − B)` (`authored-model.md:339-343`).
  - Pass-through groups cannot clip, be a clipping base, or own effects (`lib.rs:1661`).
- **Clipping** (`attachment:"clip"`). Resolved from stack order, Photoshop-style.
  - Consecutive clipped rows share the first unclipped eligible base below them: paint, object or isolated group. Generators, adjustments and pass-through groups are not eligible (`authored/scene.rs:39-64`).
  - The renderer's `blend_clip` blends the clipped row inside its base's coverage (`docs/internals/rendering.md:361-365`).
  - An adjustment between clipped rows is an error (`scene.rs:55`).
- **Masks.** Raster only: `{source: coverage-source ref, enabled, linked, inverted, offset}` (`artwork.rs:92-98`).
  - The coverage source is U8/U16 tiles plus `default_coverage` (`registry.rs:37`).
  - There is no vector mask (not found).
- **Blend modes** (25, `pkg/values.rs:126-131`): normal, multiply, screen, add, overlay, soft_light, color, darken, lighten, color_burn, linear_burn, color_dodge, hard_light, vivid_light, linear_light, pin_light, hard_mix, difference, exclusion, subtract, divide, hue, saturation, luminosity, pass_through.
- **Placement.** Paint, object and group occurrences carry an i64 integer `offset` as canonical decimal strings (`manifest.rs:133-147`). Effects and selections carry none (`artwork_records.rs:241`).
  - There is no per-occurrence transform.
  - A `placement` field is preserved as unsupported (`admission.rs:76-91`).
  - Only image objects own an affine.

## 5. Effects

- **Record.** `capy.effect/2` `{builtin, version, values{key:{kind,value}}, spatial?}` (`registry.rs:38`; `effect_records.rs:46-120`).
  - Value kinds: `number` (F32), `toggle`, `choice`, `color`, `curve`, `gradient`, `lut3d`, `texture`.
  - Every key is required (`effect_records.rs:325-341`).
  - `spatial` is an F64 reference→composition affine plus extent (`artwork.rs:105-121`).
  - Custom programs (`capy.filter/1`) are private only; portable save refuses them (`registry.rs:53-56`; test `codec/tests.rs:364-389`).
- **Stack or graph.** A linear stack, not a graph.
  - Effects are occurrences: unattached adjustments act on the scoped lower composite; `attachment:"effect"` forms a contiguous chain above its owner; generators are content (`scene.rs:44-61`).
  - `backdrop` and `clip` are not persisted edges (`doc:49-50`).
- **Built-ins.** `codec/fixtures/builtin-contracts.json` has 54 filters plus 8 texture programs. Version 2 applies to gradient_map, gradient_fill, denoise, domain_warp, posterize, kaleidoscope and threshold (`effect_records.rs:76-78`).
  - Filters: black_white, bloom, brightness_contrast, channel_mixer, chromatic_aberration, clarity, color_balance, color_lookup, crosshatch, crt, curves, dehaze, denoise, desaturate, domain_warp, edge_detect, emboss, exposure, film_grain, gaussian_blur (sigma), glass, gradient_fill (linear/radial/reflected, 32-stop gradient), gradient_map, halftone, heat_haze, high_pass, hue_saturation, invert, iridescence, kaleidoscope, levels, motion_blur, painterly, pencil, photo_filter, pixel_mosaic, posterize, rainy_glass, ripple, selective_color, shadows_highlights, soft_focus, solarize, solid_color, split_tone, swirl, threshold, unsharp_mask, vhs, vibrance, vignette, white_balance, brightness_to_opacity, paper.
  - Textures: watercolor, drawing, laid, honeycomb, sanded, washi, handmade, canvas.
  - There is no drop shadow, stroke or glow layer style (grep found none).
  - Doc mismatch: `doc:793-794` says 53 contracts and 285 keys; the file has 54 filters with 287 keys.
- **Own mask, blend, opacity.** Yes, effects carry these as occurrences (`docs/reference/runtime-filters.md:84-86`).
- **Pixel space.** Composition pixels at native resolution, in premultiplied linear document RGB or the composition's blending space (`doc:377`). Positions are mapped through the saved affine (`doc:374`).

## 6. Color

- **Portable color.** `{"rgba":[…]}` (transfer-encoded) or `{"linear_rgba":[…]}`, with `space` one of srgb, display_p3, adobe_rgb or pro_photo (`values.rs:83-91`; `color/rgb.rs:29-31`).
  - F32 precision, straight (not premultiplied) alpha in [0,1], extended RGB allowed (`color/value.rs:8-17,37-39`).
- **Working color.** `{space, depth: u8/u16/f16/f32}` (`values.rs:102-106`). Integer samples are display-referred; float is linear, with 1.0 = 203 cd/m² (`doc:372`).
- **Blend domain.** `linear` or `perceptual`, set once per document on the composition (`values.rs:113-117`; `artwork.rs:37-43`). Float documents must use linear (`artwork_records.rs:337`).
- **ICC.** `capy.icc/1` holds image interpretation and proof profiles only; the working space is always built-in.
- **CMYK.** Only as an imported image's channels (`artwork_records.rs:110`) and in proof output intent.
- **Spot colors, overprint.** Not found.
- **Gradients.** Only as effect values: 32 stops, interpolation classic, linear_rgb or oklab (`values.rs:76-79`). The Gradient tool writes pixels through a transient `RasterOperationKind::Gradient` (`layers.rs:376-384`); unfinished operations are refused at save (`artwork_records.rs:187`).

## 7. Document

- Size is positive u32 pixels, up to 32,768 (`artwork_records.rs:333-334`).
- Optional rational density in inch, centimetre or metre; no implicit 72 or 300 (`values.rs:158-166`; `doc:144`). There is no physical-size field.
- There is no background field; Paper is a Solid Color or `paper` effect occurrence (`doc:108-109`).
- Multiple compositions or outputs are structurally valid but Preserved (`topology.rs:153,166-170`).
- Off-canvas content is retained: independent source domains, signed offsets and any finite affine (`authored-model.md:100,133-138`). The frame has no saved origin (`doc:93`).

## 8. Image objects

- `capy.image-object/1 {image: ref, affine?: [a,b,c,d,tx,ty] F64, interpolation?: linear|nearest}` (`artwork_records.rs:360-367`).
  - Meaning: x′ = a·x + c·y + tx (`doc:323`).
  - Validation: finite and invertible; precision limits are Unsupported, singular maps Invalid (`authored/objects.rs:158-195`).
  - JSON f64 values round-trip bitwise (`json.rs:118-135`).
- There is no crop field (not found).
- Images are always embedded, never linked: `capy.image/1` holds 256-px lossless LZ4 tiles, with channels gray, gray_alpha, rgb, rgba or cmyk, depths u8 to f32, a profile and an optional resolution (`artwork_records.rs:101-124`).
  - The original PNG/JPEG bytes and MIME types are not stored.
  - Resource locations can only be a ZIP member or a pack (`manifest.rs:186-201`).

## 9. Text, fonts, vector data

- **Text, fonts, Béziers, shapes:** not found in the package code.
- **Figures** (line, rectangle, ellipse) are transient raster operations (`figures.rs:1`; `layers.rs:371`).
- **Selection contours:** `{"contours":[[[x,y]…]…]}`, F32 polygon points only (at least 3 per contour), even-odd fill, optional **F32** affine and `inverted` (`selection_records.rs:56-95`; affine at `:79`).
- **Rulers:** `straight` or `parallel` (start, end) and `radial` (center) as F32 points (`artwork_records.rs:148-169`).

## 10. Evaluation-meaning table (`doc:360-402`)

- It fixes the meaning of saved values. Changing a stable data meaning needs a new record or data version, with a concrete converter and a fixed-file test (`doc:217-225,396-402`).
- Precedent for new types: a binary64 reference implementation (`crates/layer-render-wgpu/src/object_sampling_reference.rs`), independent-equation tests with tolerances, and fixed hardware renders (`layer-render-wgpu/src/fixtures/authored-renders.tsv`).
- No conversion framework, shader generations or reserved future fields (`doc:245-250,290-292`).

## 11. Blockers or complications

- **(a) Tree of items in one object.**
  - Nothing in the codec forbids it, and the GA design explicitly allows a type-owned hierarchy (`docs/history/object-layer-ga-design.md:1695-1700`).
  - Inline JSON is limited by nesting under 128 for the whole manifest. Record `data` already starts at depth 4, so a `{children:[…]}` tree reaches only about 60 levels.
  - Inline JSON is also limited to 4,194,304 nodes and 64 MiB. Records-per-item hit the 65,536-record limit and the single-use rule.
  - A tree must go in binary resources without Capy IDs.
- **(b) Per-item F64 affine.** No blocker; `Affine64` and its validation already exist. Selection, point and color values are F32.
- **(c) Shared definitions record** (symbols, swatches, gradients).
  - No record type exists for this.
  - Immutable multi-use is already allowed (images). A many-use editable type needs a new `Shape`, explicit propagation, edges and cycle checks; the design says no placeholder (`object-layer-ga-design.md:1751-1760`).
  - Item payloads must reference definitions through visible JSON slots.
  - Older readers open the file as Preserved.
- **(d) Font resources.**
  - The resource types and encodings are closed sets (`manifest.rs:156-176`); there is no MIME field.
  - A new `capy.font/1` raw or `lz4-bytes` resource is easy, but older readers open the file as Preserved.
  - It must be charged against the asset and decoded-size budgets.
- **(e) Opaque round-trip data.** Already supported through ancillary (`copy_safe`) records plus opaque resources. Constraints:
  - no ancillary→ancillary references;
  - artwork cannot depend on ancillary data (`topology.rs:108`);
  - dropped when a reference stops resolving;
  - not cloned on duplicate (`doc:76-81`);
  - JSON that could contain `{"ref"}` must be stored as a binary resource (`doc:43-44`).
- **(f) Per-item opacity, blend, clip, mask.**
  - Today these exist only per occurrence: masks are raster coverage, clipping is sibling stack order, and the blend domain is per document.
  - Per-item versions must be typed fields inside the new object, with defined evaluation stages (`object-layer-ga-design.md:1717`).
- **(g) Layer-level transforms.** These are blocked by policy, not only by the codec.
  - Occurrences have integer offsets only, and `placement` was removed; any addition makes older readers Preserve the file.
  - The design states containers are "not arbitrary transforms on paint-layer containers" (`object-layer-ga-design.md:1695-1700,1792-1800`).
  - SVG `<g transform>` must be baked into object-local items.

## Facts most relevant to SVG-scale extensibility

1. A new record or resource type is safe: older readers keep the whole file read-only, show a ≤1,024 px preview at most, and Copy Original is byte-exact (`admission.rs:206-213`). They cannot edit any part of it.
2. Version suffixes are just different type strings: `capy.vector-object/1` and later `/2` are unrelated types.
3. Hard reader limits: 65,536 records, 4,096 occurrences, 4.19 M JSON nodes and 64 MiB of JSON, nesting under 128, evaluation depth 128 (about 62 group levels), 262,144 resources. Bulk geometry and deep trees must live in binary resources.
4. Binary payloads cannot carry Capy IDs; every reference to a definition, font or image must be a visible JSON `{"ref"}`.
5. Sharing is single-use for editable content, except images; a shared-definitions model needs new topology rules.
6. Occurrences carry only an integer offset; the only F64 affines are on image objects and effect spatial references; selection and ruler geometry is F32 polygons and points.
7. Blend modes, isolated and pass-through groups, sibling clipping and raster masks are occurrence-level; the blend domain is per document; colors are F32 straight RGB in 4 built-in spaces, with no CMYK working space, spot colors or ICC working space.
8. No text, font, path, gradient-paint or stroke primitives exist in the format; gradients appear only as effect parameters (32 stops).
9. Ancillary `copy_safe` records with opaque resources already allow unknown Inkscape or Illustrator data to round-trip, with closure and no-cloning limits.
10. A layer or group transform contradicts the frozen design; transforms must be inside the object type.
