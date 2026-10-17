# PSD/PSB import fidelity across desktop editors (state as of October 2026)

[PSD import compatibility](../psd-import-compatibility-research.md) · source report, 2026-10-16

Research for the [PSD import compatibility record](../psd-import-compatibility-research.md), which holds the gap analysis built from it.

## Scope, method, legend

I worked from vendor help pages, release notes, open-source importer code (GIMP, Krita, the Paint.NET PSD plugin), bug trackers and forums. Where a vendor publishes nothing, the cell says so. I did not test any app myself; the only side-by-side test I found is Heatware's (updated 14 Oct 2026; Photopea, Krita 5.3.4 and GIMP 3.2.6 checked against Photoshop's saved composite) [1].

**Cell codes**
- **E** = editable as a native equivalent
- **E~** = editable, but the mapping is approximate or loses parameters
- **R** = rasterized from the layer's stored pixels
- **C** = only the merged composite is used
- **D** = dropped, or left as an empty or placeholder layer
- **?** = unknown or unverified
- **(inf.)** = my inference

A † in the source list means I saw only a search excerpt of that page (the site blocked fetching), so I did not read the full page.

---

## Matrix A: photo and design editors

| Feature | Photopea | Affinity 2 / 3 | Pixelmator Pro | Krita 5.3 | GIMP 3.2 | PHOTO-PAINT | Paint.NET + PSD plugin 2.5 | PS Elements |
|---|---|---|---|---|---|---|---|---|
| Pixel layers | E [1] | E [26] | E [37] | E [38] | E [1] | E (inf.) [54] | E [57] | E [71] |
| Groups / pass-through | E [1] / ? (folders have their own blend mode [2]) | E; groups default to Passthrough [26][28] | ? | E; pass-through flag set on the group [41] | E [1]; pass-through mapped exactly [48] | ? | D: tree flattened into renamed marker layers [58]; pass-through becomes Normal [60] | E; "layer group imported from Photoshop" can be simplified [73†] |
| Clipping masks | ? (no primary doc) | ? (forum: rounded-corner clipping loses control [30†]) | ? | D: flag read but unused [42][43][44] | E~: emulated by inserting groups [47] | ? | D [58] | ? (Elements has native clipping [76†]) |
| Layer masks (density / feather) | E, including density and feather [3] | E [29]; density/feather ? | ? | E as transparency mask [38]; density/feather ? | E; density/feather ? | E as clip mask; density and feather baked, not editable [54] | R: multiplied into alpha; density/feather D [59] | ? |
| Vector masks | E [3] | ? | ? | E (5.3, vector selection on a transparency mask) [39][41] | R ("converted to raster") [47] | ? | D: paths not rendered (inf.) [59] | ? |
| Fill opacity vs opacity | ? | ? (Affinity has a native fill-opacity control [34†]) | ? | ? (not in the layer record [42]) | ? | ? | D: only opacity, visibility and blend mode copied [58] | ? |
| Dissolve, Darker/Lighter Color, Hard Mix, Subtract, Divide | ? (Subtract exists [4]) | E except Dissolve, which Affinity lacks [28]; its fallback is unverified | ? | All exist natively [45]; mapping not checked | Dissolve exact; the rest approximated with a warning; unknown modes become Normal [48] | Darker/Lighter Color become If Darker / If Lighter [54] | D: 10 modes mapped, the rest become Normal [60] | ? |
| Blend If / knockout | Blend If E [5] / knockout D (developer declined in 2018) [6] | ? (Blend Ranges is similar but weaker [33]) | ? | D / D (blending ranges read, unused) [42] | D ("not imported") [47] / ? | ? | D | Blend If D (Elements never had it) [75†] |
| Adjustment layers | E [2][1] | E for "certain" types [26][27]; bug: grayscale Levels values misread [31†] | E for some (Hue/Saturation, Exposure, Channel Mixer) [35] | D ("lost") [1] | D: empty layers in 3.2 [1][47]; 3.3 dev branch maps some to GEGL filters [51] | E~ as lenses: Vibrance, B&W (to Grayscale), Channel Mixer, Gradient Map (no noise, opacity stops or dither), Photo Filter [54] | D: pixel data undefined [58][85] | E for its own adjustment types [76†]; others ? |
| Fill layers (solid / gradient / pattern) | E [7] | ? (gradient fill on a mask flagged "Unknown property"; appearance kept [29]) | ? | E as generator layers; with a vector mask, becomes a shape layer [40][41] | R [47] | ? | R (inf.) | E [73†] |
| Layer styles | E~ (approximations [8]; Inner Glow differs [9]) | E~ (mostly [29]; unsupported ones listed as unknown properties [30†]) | E~ ("don't always translate exactly") [37] | E~ (bevel/emboss and inner glow render differently) [1] | Partly: drop shadow, inner shadow, outer glow, colour overlay become filters; the rest D [50][1] | Partly: "some cannot be imported" [54] | D [58] | ? |
| Shape layers / live shapes | E, including live-shape parameters [10] | ? (VectorOriginationData flagged unknown [30†]) | E as vector shapes since 2.1.3 [35][36] | E (5.3) [39] | R; 3.3 dev imports solid shapes as vector layers [51] | ? | R (inf.) | E [73†] |
| Text (fonts, warp, vertical) | E; missing font substituted, original font kept on save [11]; warp [12] and vertical [13] supported | E if a setting is on, else R [26]; Adobe Fonts don't carry over [32†] | E, including symbol sizes and capitalization [35] | E if the user accepts a prompt, else R; text on path and OpenType kept [39][41] | R [1]; 3.3 dev makes it editable [51] | E [54] | R (inf.) | E [73†] |
| Smart objects (embedded / linked / smart filters) | Embedded E [14]; smart filters from a partial filter set [15]; linked becomes R [16] | R by default; embedded document if a setting is on [26][27] | R (no Photoshop-style smart objects) (inf.) [37] | R [1] | R per test [1] (the code's dialog says "empty layers" [47], which conflicts) | Smart filters become a base object plus separate filter effects [54] | R (inf.) | Shown, but contents not editable; Simplify rasterizes [73†] |
| Artboards | E [17] | "become individual layers" [27] | ? | ? | ? | ? | D | ? |
| 16 / 32-bit | E [18] | E (32-bit opens) [29] | ? | E (inf.) [41] | E [47] | 48-bit RGB [54] | R: 16-bit cut to 8; 32-bit only RGB/gray, tone-mapped [59] | 16-bit layered: convert to 8-bit or flatten [72] |
| CMYK / Lab / Duotone / Indexed | CMYK edited as RGB; CMYK mode is a SWOP preview [19][20]; others ? | CMYK and Lab colour space preserved [26]; others ? | ? | CMYK/Lab mapped to native spaces [41]; duotone kept for re-save [41]; indexed ? | CMYK to RGB via embedded profile; Lab to sRGB; duotone as gray with data kept for export; indexed E; multichannel since 3.2.2 [47][52] | CMYK up to 32-bit, gray, duotone, monotone; spot channels kept [54] | CMYK by naive formula; Lab to sRGB; indexed via palette; duotone as gray [59] | CMYK converted to RGB [74†] |
| ICC profile | E: detected, kept, saved [19] | E (optional convert to working space) [26] | ? | ? | E: CMYK profile becomes the simulation profile; Lab profile ignored [49] | ? | D (inf.) [59] | ? |
| Layer comps | E [21] | ? | ? | D [41] | D (flagged in dialog) [49][47] | ? | D | ? |
| Saved paths / channels | E? (Paths and Channels in the docs) [22] | ? | ? | ? | E: saved paths, work path, clipping path, alpha channel names [49] | Spot channels E [54] | D | ? |
| Guides | ? | ? | ? | E [39][41] | E [49] | ? | D | ? |
| Slices / annotations / timeline | Slice export documented, PSD load ? [23]; animation only via layer-name convention [24] | ? | ? | ? | Slices kept as raw data; annotations not handled [49] | ? | D | ? |

## Matrix B: paint, vector and UI apps

| Feature | Clip Studio Paint | Corel Painter | Rebelle 8 (Pro) | CorelDRAW | Illustrator (Open/Place) | Sketch / Figma / Inkscape |
|---|---|---|---|---|---|---|
| Pixel layers | E (inf.) | E [67] | E [69] | Appearance preserved (X7) [56] | E as image objects [80] | Sketch: C, one flattened layer [81]; Figma: no native import [82][83†]; Inkscape: no native import (old forum) [84†] |
| Groups / pass-through | ? | ? | Group names kept [69] | ? | E as sublayers [80] | — |
| Clipping masks | ? | D [67] | E [69] | ? | Flattened (CS-era article) [79†] | — |
| Layer masks | ? | E [67][68] | E [69] | Appearance preserved [56] | E as opacity masks [78†] | — |
| Vector masks | ? | D ("layer clipping paths" not kept) [67] | Listed as preserved [69] | ? | ? | — |
| Blend modes | ? | Most become composite methods; the rest Default [67] | E, with listed exceptions (list truncated on the page) [69] | Darker/Lighter Color become If Darker / If Lighter [55] | Kept; some with transparent pixels merged [77†] | — |
| Blend If / knockout | ? | ? | ? | ? | Knockout layers merged (R) [77†] | — |
| Adjustment layers | Partly: come in as correction layers, but values can be wrong (Brightness/Contrast, Levels) [64][65] | D [67] | ? | Appearance preserved [56]; Gradient Map loses noise, opacity stops and dither [55] | R: merged into the layers beneath [77†] | — |
| Layer styles | D ("not compatible") [63] | D (help says rasterize in Photoshop first) [67][68] | ? | Partly [55] | R (CS-era) [79†] | — |
| Shapes | ? | D [67] | ? | ? | E [80] | — |
| Text | R [62] | ? | ? | ? | E "where possible", else R [77†] | — |
| Smart objects | ? | ? | ? | ? | ? | — |
| Bit depth / colour mode | 8-bit only (answer from about 2020) [66]; CMYK to RGB [61] | Non-RGB: prompt to convert to RGB [67] | Import 8/16/32-bit Gray, RGB, CMYK, Lab; ICC kept [69] | CMYK up to 32-bit, gray, duotone; spot channels become DeviceN [55] | ? | — |
| Comps / slices / hidden layers | ? | Alpha channels kept [67] | ? | ? | Pick one layer comp at import; linked files keep visibility overrides; optional "Import Hidden Layers" and "Import Slices"; Flatten keeps the document clipping path [77†] | — |

---

## Per-app notes

**Photopea (reference).** It is built around PSD as its own model. It is the only importer here with editable Blend If [5], layer comps [21], live-shape parameters [10], vertical and warped text [12][13], mask density and feather [3], and embedded smart objects with smart filters [14][15].
- **Known gaps:** the developer gave up on knockout and Blend If channel options in 2018 because "they are not documented anywhere" [6]. Linked smart objects arrive as raster layers, since a website can't reach your disk; the workaround is Replace Contents [16]. CMYK mode is only a SWOP preview [19], and one user saw a CMYK PSD reported as CMYK while its channels were RGB [20]. Layer styles are self-described approximations [8], and rendering bug reports are still being filed (Inner Glow, 2026) [9].
- **Degradation:** missing fonts are substituted for display, and the original font information is written back on save [11]. I found no warning dialog and no statement about keeping unknown PSD data.
- **Reputation:** reviewers and the 2026 test rate it highest. Text, smart objects, adjustment layers and styles all survive opening and saving [1][25].

**Affinity Photo/Designer 2 and Affinity 3 (Canva).** PSD support is described as unchanged in v3 [29].
- **Settings:** two General settings control fidelity: "Import PSD text as text rather than bitmap" and "Import PSD smart objects where possible" (as embedded documents). Both default to rasterizing [26][27].
- **Adjustments:** "certain" adjustment layers are mapped [26]. There is an old bug where grayscale Levels values were misread [31†].
- **Blend modes:** Darker/Lighter Colour, Hard Mix, Subtract and Divide exist natively; Dissolve does not [28].
- **Degradation:** import shows an "unknown properties" dialog naming unsupported PSD blocks (e.g. `VectorOriginationData`, `EffectGradientFill`). Staff say some features can't be supported because they sit in proprietary data blocks [30†][29]. Import is one-way: Affinity saves its own document, with an optional "save over imported PSD" setting and PSD export [26][27]. Artboards "become individual layers" [27].
- **Reputation:** reviewers say it has few compatibility issues [29]. TechRadar warns that smart objects may not reproduce exactly and Adobe Fonts won't carry over [32†].

**Pixelmator Pro (now an Apple product; the old Pixelmator blog URL redirects to apple.com).** Version 2.1.3 (2021) rewrote the PSD engine. It added PSB, kept shape layers as vector shapes, kept Hue/Saturation, Exposure and Channel Mixer adjustments, and improved effects and text features [35][36].
- **Smart objects:** there are no Photoshop-style smart objects [37].
- **Reputation:** DPReview says layers, text, shapes and effects "come through" but "don't always translate exactly", and advises against round-tripping a client's PSD [37].
- I found no documented warning UI.

**Krita 5.3 (released Feb 2026).**
- **What 5.3 added:** loading shapes, vector masks and guides, plus text "including text on path and OpenType features" [39]. Fill layers became generators in 5.1 [40].
- **Degradation UI:** the loader asks "Found text objects, do you wish to load them as editable text shapes?" and falls back to pixels if you say no [41]. Pass-through is mapped to Krita's group pass-through flag [41].
- **Clipping and Blend If:** the clipping byte and blending ranges are read but not used. Blending ranges are not written back either [42]. Users on 5.3.1 (July 2026) confirm clipping is lost; PSD-style clipping is still in unmerged merge requests [43][46].
- **Kept for round trip:** duotone data is stored for re-save [41]. The composite is used only when a file has no layers [41].
- **Reputation:** in the 2026 test, smart objects became pixels, adjustment layers were lost, and styles mostly survived [1]. Krita's own manual recommends "any other file format instead of PSD" [38].

**GIMP 3.2 (released March 2026).** This is the most transparent degradation design I found.
- **Compatibility Notice dialog:** it lists each category of unsupported feature with its outcome [47]. Examples: "Adjustment layers are partially supported. Unsupported adjustment layers will show up as empty layers." Fill layers and vector masks are "converted to raster layers". Unsupported effects "will be discarded". Layer comps "will be discarded".
- **Blend-mode table:** each mode is flagged exact or approximate. Approximate ones warn "Results will differ", and unknown keys revert to Normal with a warning [48].
- **Clipping:** emulated by inserting groups [47].
- **Kept for round trip:** unknown image resources are stored as raw attached data ("parasites") for re-saving; slices go the same way [49]. Duotone imports as grayscale, with its data "saved and can be reapplied on export" [47].
- **Composite:** a separate "merged image" loader exists [53].
- **What 3.2 imports:** drop shadow, inner shadow and outer glow as non-destructive filters [50]. Multichannel PSDs work since 3.2.2 [52].
- **Coming in 3.3 dev:** editable text, some adjustment layers and modern styles as GEGL equivalents, and solid shapes as vector layers [51].
- **Reputation:** in the 2026 test, text became pixels and adjustment layers became empty layers [1].

**Corel PHOTO-PAINT and CorelDRAW.** Help pages document their mappings explicitly.
- **Adjustments:** five adjustment types become lenses [54].
- **Blend modes:** Darker/Lighter Color become "If Darker / If Lighter" [54][55].
- **Masks:** density and feather are baked into a clip mask and "you can't change" them afterwards [54].
- **Smart filters:** become a base object plus separate filter effects [54].
- **Text:** stays editable in PHOTO-PAINT [54].
- **Channels:** spot channels are kept (PHOTO-PAINT) or become DeviceN bitmaps (CorelDRAW) [55].
- The pages are copyright 2016–2018 and don't name a version, so current behaviour may differ.

**Paint.NET + PSD plugin 2.5 (third-party, MIT).** Supports 8-bit RGB layers with "the common blend modes" [57].
- **Groups:** the tree is flattened into marker layers, and hidden groups hide their contents [58].
- **Masks:** multiplied into alpha [59].
- **Blend modes:** only 10 are mapped; everything else loads as Normal [60].
- **Bit depth and colour:** 16-bit keeps only the high byte; CMYK uses a profile-free formula [59].
- **No notices.** The composite is used only when there are no layers [58].

**Clip Studio Paint.** Text rasterizes on open [62]. CMYK converts to RGB [61].
- **Layer styles:** "not compatible"; users are told to rasterize them in Photoshop first [63].
- **Adjustments:** they arrive as correction layers but can misbehave. Modern (non-legacy) Brightness/Contrast reads as 0, and Levels colours shift; Celsys support reportedly called this "current specification" [64][65].
- **Bit depth:** a CSP Ask answer from about 2020 says only 8-bit PSDs open [66]. This may be out of date.
- I found no official import table.

**Corel Painter.** Keeps layers, layer masks and alpha channels. Blend modes without an equivalent become Default.
- **Not supported:** layer effects and adjustment layers.
- **Not preserved:** shape layers, clipping paths and clipping groups [67].
- **Prompts:** non-RGB files trigger a convert-to-RGB prompt [67].
- **Advice:** Corel tells users to convert styles to pixels before importing [68].

**Rebelle 8 (Pro feature; July 2025).** A new PSD library [69][70]. Import reads 8/16/32-bit Gray, RGB, CMYK and Lab, and keeps groups, clipping masks, layer and vector masks, ICC, locks and colour labels. Blend modes are kept "except" some (the list is cut off on the page). Export is 8-bit RGBA only. The page says nothing about text, adjustments or smart objects.

**Photoshop Elements.**
- **Warning dialog:** says the file "contains features that your software does not recognize" and offers **Keep Layers / Flatten** [71].
- **16-bit:** layered 16-bit files require converting to 8-bit or flattening [72].
- **Colour:** CMYK converts to RGB [74†]. Blend If doesn't exist in Elements [75†].
- **Simplify:** smart objects, type, shape and fill layers, and "layer group imported from Photoshop" can be simplified to pixels [73†].

**Illustrator (Open/Place).** The decision is made up front in an import dialog [77†].
- **Two modes:** "Convert Photoshop layers to objects and make text editable where possible" or "Flatten to a single image".
- **Other choices:** pick a layer comp; for linked files, choose how visibility updates (keep overrides vs use Photoshop's); optionally import hidden layers and slices.
- **What gets merged:** unsupported features are merged and rasterized to keep the look. That includes adjustment layers, knockout layers and some blend modes over transparency. Hidden layers that would need merging are discarded [77†].
- **Masks:** layer masks and opacity masks convert into each other [78†].

**Sketch / Figma / Inkscape.**
- **Sketch:** imports PSD "only as a flattened layer" [81].
- **Figma:** staff said in February 2024 that PSD import isn't supported natively [82], and an October 2025 request was still open [83†].
- **Inkscape:** I found no native PSD import, only old forum answers and a 2012 third-party extension [84†].

---

## Patterns across importers

1. **What a PSD can fall back on.** Every layer stores its own pixels. Adjustment layers are the exception: their channel data is "undefined (probably all white)" [85]. The merged composite is missing whenever "maximize compatibility" was off [85].
   - Text, shape, fill and smart-object layers therefore always have a pixel fallback. GIMP 3.2, CSP, Krita and Paint.NET all use it (inf.).
   - Adjustment layers and layer styles have no per-layer pixels. They come out empty (GIMP), lost (Krita) or dropped (Paint.NET, CSP), unless the importer renders them itself or takes the composite.
2. **Fallback options in use**, roughly from best to worst:
   - exact map
   - approximate map with a warning (GIMP's exact/approximate flag [48])
   - convert with fixed, non-editable parameters (Corel's baked density/feather [54])
   - rasterize from stored pixels
   - named empty placeholder that keeps the stack (GIMP)
   - silent drop or fallback to Normal (Paint.NET, Painter)
   - composite only (Sketch; Elements and Illustrator "Flatten")
3. **How users are told.**
   - Category list with stated outcomes: GIMP [47].
   - List of raw unknown properties: Affinity [30†].
   - Keep Layers / Flatten choice: Elements [71].
   - Per-feature prompt: Krita text [41]; Painter colour mode [67].
   - Options chosen before import: Illustrator [77†].
   - Nothing documented: Photopea, Paint.NET, Pixelmator, CSP.
4. **Keeping data for the round trip.** GIMP stores unknown resources and duotone data [49][47]; Krita keeps the duotone block [41]; Photopea keeps original font names [11]. Affinity is explicitly one-way [26]. I found no importer that preserves unknown *layer-level* blocks.
5. **Features almost everyone loses:**
   - Knockout: no importer found supports it; Photopea declined [6].
   - Blend If: only Photopea [5].
   - Linked smart objects: Photopea rasterizes them [16].
   - Timeline/animation: no importer found keeps it.
   - Layer comps: only Photopea and Illustrator.
   - Clipping: Krita drops it [43].
   - Adjustment layers: the free tools drop them [1].

## What this suggests for Capy Canvas (my inference)

To reach Photopea-level fidelity, the authored model needs the following. This survey did not confirm the starred items in Capy's model; the [model audit](05-capy-model-audit.md) covers them:
- pass-through groups and clipping
- fill opacity separate from opacity, so styles are unaffected
- per-occurrence mask density and feather
- vector masks
- live-shape parameters
- text with warp and vertical orientation, plus the original font name kept when a substitute is used
- embedded documents with attached filters
- artboards, layer comps, guides and saved paths
- Blend If ranges *
- a knockout flag *
- a way to carry opaque, unrecognized PSD blocks for re-export (the GIMP parasite pattern) *

For the fidelity policy, GIMP's model is the most copyable:
- a per-feature outcome table (editable / approximate / raster / placeholder / dropped)
- a single notice listing which categories occurred
- a pixel fallback that uses stored layer pixels for content layers, and the composite only for whole-document rescue

## Not verified

Most Pixelmator, CSP, CorelDRAW, Rebelle and Elements cells; Photopea's handling of clipping, fill opacity, guides and timeline; Affinity's handling of clipping, Blend If, vector masks, comps and guides; how Krita handles indexed PSDs. The Corel and Painter help pages are dated 2013–2018.

## Sources

1. https://www.heatware.net/tech-tips/open-and-edit-psd-files-free/
2. https://blog.photopea.com/photopea-1-4.html
3. https://www.photopea.com/learn/masks
4. https://github.com/photopea/photopea/issues?q=blend+if (#7551)
5. https://edits101.com/how-to-use-blend-if-in-photopea-quick-guide/
6. https://github.com/photopea/photopea/issues/513
7. https://www.photopea.com/learn/other-layers
8. https://blog.photopea.com/photopea-0-5.html
9. https://github.com/photopea/photopea/issues?q=%22not+supported%22+psd (#8918)
10. https://blog.photopea.com/photopea-3-8-live-shapes-and-new-filters.html
11. https://blog.photopea.com/photopea-0-9.html
12. https://github.com/photopea/photopea/issues?q=warp+text (#6228)
13. https://github.com/photopea/photopea/issues?q=vertical+text
14. https://www.photopea.com/learn/smart-objects
15. https://blog.photopea.com/photopea-2-1.html
16. https://github.com/photopea/photopea/issues/4612
17. https://blog.photopea.com/photopea-5-1-open-affinity-files-and-make-artboards.html
18. https://blog.photopea.com/photopea-5-6-advanced-color-depth.html
19. https://blog.photopea.com/photopea-5-2-color-profiles-and-cmyk-mode.html
20. https://www.gimp-forum.net/Thread-Export-corrupted-psd-file
21. https://www.photopea.com/learn/layer-comps
22. https://www.photopea.com/learn/
23. https://www.photopea.com/learn/slices
24. https://www.photopea.com/learn/animations
25. https://www.xda-developers.com/ways-photopea-outperforms-gimp/
26. https://affinity.help/photo2/en-US.lproj/pages/GetStarted/importAdobe.html
27. https://www.affinity.studio/blog/how-to-open-a-psd-file
28. https://affinity.help/photo2/en-US.lproj/pages/Layers/layerBlendModes.html
29. https://www.beyondphototips.com/affinity-photo-adobe-psd-files/
30. † https://forum.affinity.serif.com/index.php?/topic/142334-psd-import-error-unknown-property-vectororiginationdata/
31. † https://forum.affinity.serif.com/index.php?/topic/177621-adjustment-layer-levels-not-imported-correctly-for-grayscale-psd-files/
32. † https://www.techradar.com/pro/affinity-by-canva-2026-review
33. https://lenscraft.co.uk/photo-editing-tutorials/affinity-photo-blending-ranges/
34. † https://www.tutkit.com/en/text-tutorials/23343-optimal-use-of-layer-effects-in-affinity-photo
35. https://www.mactrast.com/2021/08/pixelmator-team-releases-pixelmator-pro-2-1-3-updates-psd-engine-additional-improvements-and-fixes/amp/
36. https://www.imore.com/pixelmator-pro-gets-massive-psd-upgrade
37. https://www.dpreview.com/reviews/pixelmator-pro-3-4-camelot-review-2023
38. https://docs.krita.org/en/general_concepts/file_formats/file_psd.html
39. https://krita.org/en/release-notes/krita-5-3-release-notes/
40. https://krita.org/en/release-notes/krita-5-1-release-notes/
41. https://invent.kde.org/graphics/krita/-/raw/master/plugins/impex/psd/psd_loader.cpp
42. https://invent.kde.org/graphics/krita/-/raw/master/libs/psd/psd_layer_record.cpp
43. https://krita-artists.org/t/layer-alpha-inheritance-compatibility-with-psds/186638
44. https://bugs.kde.org/show_bug.cgi?id=350114
45. https://docs.krita.org/en/reference_manual/blending_modes.html
46. https://invent.kde.org/graphics/krita/-/merge_requests/1561
47. https://gitlab.gnome.org/GNOME/gimp/-/raw/master/plug-ins/file-psd/psd-load.c
48. https://gitlab.gnome.org/GNOME/gimp/-/raw/master/plug-ins/file-psd/psd-util.c
49. https://gitlab.gnome.org/GNOME/gimp/-/raw/master/plug-ins/file-psd/psd-image-res-load.c
50. https://gimp.org/release-notes/gimp-3.2.html
51. https://www.gimp.org/news/2026/08/16/dev-update-august-2026/
52. https://www.gimp.org/news/2026/03/28/gimp-3-2-2-released/
53. https://gitlab.gnome.org/GNOME/gimp/-/raw/master/plug-ins/file-psd/psd.c
54. https://product.corel.com/help/PHOTO-PAINT/Documentation-Windows/Corel-PHOTO-PAINT-en/Corel-PHOTO-PAINT-Adobe-Photoshop-PSD.html
55. https://product.corel.com/help/CorelDRAW/540223850/Main/EN/Documentation/CorelDRAW-Adobe-Photoshop-PSD.html
56. https://learn.corel.com/tutorials/import-adobe-illustrator-adobe-photoshop-and-pdf-files/
57. https://www.psdplugin.com/
58. https://raw.githubusercontent.com/PsdPlugin/PsdPlugin/master/PhotoShopFileType/PsdLoad.cs
59. https://raw.githubusercontent.com/PsdPlugin/PsdPlugin/master/PhotoShopFileType/ImageDecoderPdn.cs
60. https://raw.githubusercontent.com/PsdPlugin/PsdPlugin/master/PhotoShopFileType/BlendModeMapping.cs
61. https://help.clip-studio.com/en-us/manual_en/210_file/Open_file.htm
62. https://tips.clip-studio.com/en-us/articles/6549
63. https://ask.clip-studio.com/en-us/detail?id=17327
64. https://ask.clip-studio.com/en-us/detail?id=27852
65. https://ask.clip-studio.com/en-us/detail?id=51976
66. https://ask.clip-studio.com/en-us/detail?id=37176%2F1000
67. https://kb.corel.com/127966
68. https://product.corel.com/help/Painter/540111155/Corel-Painter-en/Corel-Painter-Painter-for-Users-of-Photoshop.html
69. https://www.escapemotions.com/blog/rebelle-8-view-at-print-size-paint-selection-advanced-psd-export-and-more
70. https://www.escapemotions.com/products/rebelle/manual/8/creating-opening-saving/
71. https://digitalscrapper.com/blog/mysterybox-keeplayers/
72. https://community.adobe.com/t5/photoshop-elements-discussions/problems-opening-psd-file-in-adobe-elements/m-p/14236089
73. † https://helpx.adobe.com/photoshop-elements/using/editing-layers.html
74. † https://community.adobe.com/t5/photoshop-elements-discussions/how-to-convert-colour-files-to-cmyk-on-photoshop-elements/td-p/12096240
75. † https://community.adobe.com/t5/photoshop-elements-discussions/where-is-the-quot-blend-if-quot-function-in-photoshop-2018/m-p/9494672
76. † https://helpx.adobe.com/photoshop-elements/using/adjustment-fill-layers.html
77. † https://helpx.adobe.com/illustrator/desktop/add-and-import-files/import-from-other-apps/photoshop-import-options.html
78. † https://helpx.adobe.com/bg/illustrator/desktop/manage-colors/apply-transparency-and-blending/about-opacity-masks.html
79. † https://creativepro.com/?p=120556
80. https://www.webucator.com/article/how-to-use-a-photoshop-file-in-adobe-illustrator/
81. https://www.sketch.com/docs/designing/importing-and-exporting/
82. https://forum.figma.com/ask-the-community-7/how-to-import-files-from-photoshop-or-zeplin-25719
83. † https://forum.figma.com/suggest-a-feature-11/ability-to-import-ai-psd-and-other-files-46110
84. † https://alpha.inkscape.org/vectors/www.inkscapeforum.com/viewtopic8161.html?f=5&t=1703
85. https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/
