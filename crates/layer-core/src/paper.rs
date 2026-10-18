use crate::authored::{Affine64, Composition, EffectApplication, EffectSpatialReference, OccurrenceHandle, SceneView, SourceTarget};
use crate::{EffectProgram, EffectValue, ImageResolution, ResourceLabel, TextureSource};
use std::sync::Arc;

pub const PAPER_PROGRAM: &str = "paper";
pub const DEFAULT_PAPER: &str = "drawing_paper";
pub const DEFAULT_PAPER_PPI: f64 = 300.;
const MINIMUM_PRINT_PPI: f64 = 150.;
const MM_PER_INCH: f64 = 25.4;

pub fn paper_ppi(resolution: Option<ImageResolution>) -> [f64; 2] {
    let ppi = resolution.map_or([DEFAULT_PAPER_PPI; 2], ImageResolution::pixels_per_inch);
    if ppi.iter().all(|v| v.is_finite() && *v >= MINIMUM_PRINT_PPI) { ppi } else { [DEFAULT_PAPER_PPI; 2] }
}

pub fn is_paper(program: &EffectProgram) -> bool {
    program.id.as_ref() == PAPER_PROGRAM && program.kind == crate::EffectKind::Generator
}

pub fn is_paper_layer(scene: SceneView<'_>, occurrence: OccurrenceHandle) -> bool {
    scene.effect(occurrence).is_some_and(|effect| is_paper(effect.program))
}

mod import;
pub use import::{CMYK, decode_png, encode_png, image_preview, paper_image};

pub const IMAGE_RELIEF_UM: f32 = 100.;
pub const PREVIEW_SIZE: usize = 96;
pub const PREVIEW_MM: f64 = 16.;
const PREVIEW_EXAGGERATION: f32 = 3.;
const BUILT_IN_PREVIEWS: &[u8] = include_bytes!("../../../assets/filters/paper_previews.gray");

pub struct BuiltInPaper {
    pub id: Arc<str>,
    pub label: ResourceLabel,
    pub texture: TextureSource,
    pub preview: Arc<[u8]>,
}

pub fn built_in_papers() -> &'static [BuiltInPaper] {
    static PAPERS: std::sync::OnceLock<Vec<BuiltInPaper>> = std::sync::OnceLock::new();
    PAPERS.get_or_init(|| crate::bundled_texture_catalog().filters().iter()
        .flat_map(|definition| definition.presets.iter().map(move |preset| (definition, preset)))
        .zip(BUILT_IN_PREVIEWS.as_chunks::<{ PREVIEW_SIZE * PREVIEW_SIZE }>().0)
        .map(|((definition, preset), preview)| BuiltInPaper {
            id: preset.id.clone(),
            label: preset.label.clone(),
            texture: TextureSource::Program(definition.preset(preset).expect("bundled paper")),
            preview: Arc::from(&preview[..]),
        })
        .collect())
}

pub fn preview(heights: &[f32]) -> Vec<u8> {
    let size = 2 * PREVIEW_SIZE;
    assert_eq!(heights.len(), size * size);
    let texel_um = (PREVIEW_MM * 1000. / size as f64) as f32;
    let at = |x: usize, y: usize| heights[y.min(size - 1) * size + x.min(size - 1)];
    let light = { let l = [-0.75f32, -0.45, 0.48]; let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt(); l.map(|v| v / n) };
    let shade = |x: usize, y: usize| {
        let scale = PREVIEW_EXAGGERATION / (2. * texel_um);
        let sx = (at(x + 1, y) - at(x.saturating_sub(1), y)) * scale;
        let sy = (at(x, y + 1) - at(x, y.saturating_sub(1))) * scale;
        let lit = (-sx * light[0] - sy * light[1] + light[2]) / (sx * sx + sy * sy + 1.).sqrt();
        (0.8 * lit / light[2]).clamp(0., 1.)
    };
    (0..PREVIEW_SIZE * PREVIEW_SIZE).map(|i| {
        let (x, y) = (2 * (i % PREVIEW_SIZE), 2 * (i / PREVIEW_SIZE));
        let sum = shade(x, y) + shade(x + 1, y) + shade(x, y + 1) + shade(x + 1, y + 1);
        (sum * 0.25 * 255.).round() as u8
    }).collect()
}

pub fn built_in_paper(id: &str) -> Option<&'static BuiltInPaper> {
    built_in_papers().iter().find(|paper| paper.id.as_ref() == id)
}

pub fn default_texture() -> TextureSource {
    built_in_paper(DEFAULT_PAPER).expect("bundled default paper").texture.clone()
}

pub fn reference(size: [u32; 2], ppi: [f64; 2]) -> EffectSpatialReference {
    let per_mm = ppi.map(|v| v / MM_PER_INCH);
    EffectSpatialReference {
        mapping: Affine64([per_mm[0], 0., 0., per_mm[1], 0., 0.]),
        extent: [f64::from(size[0]) / per_mm[0], f64::from(size[1]) / per_mm[1]],
    }
}

fn column_lengths(mapping: Affine64) -> [f64; 2] {
    let [a, b, c, d, _, _] = mapping.0;
    [a.hypot(b), c.hypot(d)]
}

pub fn scale(spatial: &EffectSpatialReference, ppi: [f64; 2]) -> f64 {
    let [x, y] = column_lengths(spatial.mapping);
    (x * y).sqrt() / ((ppi[0] * ppi[1]).sqrt() / MM_PER_INCH)
}

pub fn with_scale(spatial: &EffectSpatialReference, scale: f64, ppi: [f64; 2]) -> EffectSpatialReference {
    let [a, b, c, d, x, y] = spatial.mapping.0;
    let [first, second] = column_lengths(spatial.mapping);
    let per_mm = scale * (ppi[0] * ppi[1]).sqrt() / MM_PER_INCH;
    let size = [spatial.extent[0] * first, spatial.extent[1] * second];
    EffectSpatialReference {
        mapping: Affine64([a / first * per_mm, b / first * per_mm, c / second * per_mm, d / second * per_mm, x, y]),
        extent: size.map(|v| v / per_mm),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaperSurface {
    pub texture: TextureSource,
    pub mapping: Affine64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaperBinding {
    pub surface: Arc<PaperSurface>,
    pub target_offset: [i64; 2],
}

fn texture(application: &EffectApplication) -> Option<&TextureSource> {
    if !is_paper(&application.program) { return None; }
    application.values.iter().find_map(|value| match value { EffectValue::Texture(texture) => Some(texture), _ => None })
}

pub fn paper_occurrence(scene: SceneView<'_>, from: Option<OccurrenceHandle>) -> Option<OccurrenceHandle> {
    let mut current = from;
    loop {
        let parent = current.and_then(|h| scene.parent(h));
        let siblings = scene.children(parent);
        let start = current.and_then(|h| siblings.iter().position(|s| *s == h)).unwrap_or(0);
        if let Some(found) = siblings[start..].iter().find(|h| scene.effect_application(**h).is_some_and(|a| is_paper(&a.program))) {
            return Some(*found);
        }
        current = Some(parent?);
    }
}

impl crate::Document {
    pub fn bottom_paper(&self) -> Option<OccurrenceHandle> {
        self.scene().children(None).last().copied().filter(|h| is_paper_layer(self.scene(), *h))
    }
    pub(crate) fn validate_papers(&self, previous: &Self) -> Result<(), crate::DocumentError> {
        let scene = self.scene();
        let bottom = self.bottom_paper();
        if let Some(paper) = previous.bottom_paper()
            && scene.includes(paper) && is_paper_layer(scene, paper) && bottom != Some(paper) {
            return Err(crate::DocumentError::ProtectedOccurrence(paper));
        }
        let mask = |document: &Self, h| document.scene().occurrence(h).and_then(|o| o.mask.as_ref()).map(|mask| mask.source);
        for &h in scene.order().iter().filter(|h| is_paper_layer(scene, **h)) {
            let kept = previous.scene().position(h).is_some() && is_paper_layer(previous.scene(), h);
            if !kept && bottom != Some(h) { return Err(crate::DocumentError::ProtectedOccurrence(h)); }
            if mask(self, h).is_some() && (!kept || mask(self, h) != mask(previous, h)) { return Err("Paper can't have a mask".into()); }
        }
        Ok(())
    }
    pub fn paper(&self, target: Option<SourceTarget>) -> PaperSurface {
        let scene = self.scene();
        paper_occurrence(scene, target.and_then(|t| scene.source_owner(t))).and_then(|h| self.paper_surface(h)).unwrap_or_else(|| PaperSurface {
            texture: default_texture(),
            mapping: reference(self.composition().size, paper_ppi(self.composition().resolution)).mapping,
        })
    }
    pub fn paper_surface(&self, occurrence: OccurrenceHandle) -> Option<PaperSurface> {
        let application = self.scene().effect_application(occurrence)?;
        Some(PaperSurface { texture: texture(application)?.clone(), mapping: application.spatial?.mapping })
    }
    pub fn paper_binding(&self, target: SourceTarget) -> PaperBinding {
        PaperBinding { surface: Arc::new(self.paper(Some(target))), target_offset: self.target_offset(target) }
    }
    pub fn set_paper_texture(&mut self, texture: TextureSource) {
        let Some(handle) = paper_occurrence(self.scene(), None).and_then(|h| self.scene().effect_handle(h)) else { return };
        for value in &mut self.artwork.effects.get_mut(handle).expect("listed paper").values {
            if let EffectValue::Texture(current) = value { *current = texture.clone(); }
        }
    }
    pub fn fit_papers_to_resolution(&mut self) {
        let ppi = paper_ppi(self.composition().resolution);
        let papers: Vec<_> = self.artwork.effects.iter().filter(|(_, _, a)| is_paper(&a.program)).map(|(h, _, _)| h).collect();
        for handle in papers {
            if let Some(spatial) = &mut self.artwork.effects.get_mut(handle).expect("listed paper").spatial {
                *spatial = with_scale(spatial, 1., ppi);
            }
        }
    }
}

impl EffectApplication {
    pub fn in_composition(program: Arc<EffectProgram>, values: Vec<EffectValue>, composition: &Composition) -> Self {
        let mut application = Self::new(program, values, composition.size);
        if is_paper(&application.program) {
            application.spatial = Some(reference(composition.size, paper_ppi(composition.resolution)));
        }
        application
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, DocumentNames, ResolutionUnit, authored::PortableId};
    fn document() -> Document {
        Document::new(PortableId::random(), 300, 200, DocumentNames { paint: "Ink".into(), paper: "Paper".into() })
    }
    #[test]
    fn screen_and_missing_resolutions_count_as_print_resolution() {
        assert_eq!(paper_ppi(None), [300.; 2]);
        assert_eq!(paper_ppi(Some(ImageResolution::ppi(72))), [300.; 2]);
        assert_eq!(paper_ppi(Some(ImageResolution::ppi(96))), [300.; 2]);
        assert_eq!(paper_ppi(Some(ImageResolution::ppi(150))), [150.; 2]);
        assert_eq!(paper_ppi(Some(ImageResolution::ppi(600))), [600.; 2]);
        let metric = ImageResolution { unit: ResolutionUnit::Centimetre, density: [[118, 1]; 2] };
        assert!((paper_ppi(Some(metric))[0] - 299.72).abs() < 1e-9);
    }
    #[test]
    fn scale_reads_the_mapping_against_the_paper_resolution() {
        let spatial = reference([300, 200], [300.; 2]);
        assert!((scale(&spatial, [300.; 2]) - 1.).abs() < 1e-12);
        assert!((scale(&spatial, [600.; 2]) - 0.5).abs() < 1e-12);
        let resampled = EffectSpatialReference { mapping: Affine64([2., 0., 0., 2., 0., 0.]).compose(spatial.mapping), ..spatial };
        assert!((scale(&resampled, [300.; 2]) - 2.).abs() < 1e-12);
        let stretched = EffectSpatialReference { mapping: Affine64([4., 0., 0., 1., 0., 0.]).compose(spatial.mapping), ..spatial };
        assert!((scale(&stretched, [300.; 2]) - 2.).abs() < 1e-12);
        let uniform = with_scale(&stretched, 1.5, [300.; 2]);
        assert!((scale(&uniform, [300.; 2]) - 1.5).abs() < 1e-12);
        let [a, _, _, d, _, _] = uniform.mapping.0;
        assert!((a - d).abs() < 1e-12);
    }
    #[test]
    fn scale_keeps_quarter_turns_flips_and_origin() {
        let spatial = EffectSpatialReference { mapping: Affine64([0., -2., 3., 0., 7., 9.]), extent: [10., 20.] };
        let scaled = with_scale(&spatial, 1., [254.; 2]);
        assert_eq!(scaled.mapping.0, [0., -10., 10., 0., 7., 9.]);
        assert_eq!(scaled.extent, [2., 6.]);
    }
    #[test]
    fn new_drawings_start_on_the_default_paper_at_real_size() {
        let mut doc = document();
        let surface = doc.paper(doc.working.target);
        assert_eq!(surface.texture, default_texture());
        assert!((surface.mapping.0[0] - 300. / 25.4).abs() < 1e-12);
        doc.artwork.compositions.get_mut(doc.artwork.root).unwrap().resolution = Some(ImageResolution::ppi(600));
        doc.fit_papers_to_resolution();
        assert!((doc.paper(doc.working.target).mapping.0[0] - 600. / 25.4).abs() < 1e-12);
    }
    #[test]
    fn paper_order_is_protected_across_layer_edits_and_history() {
        use crate::{Edit, LayerBlend, authored::{RecordChange, Stack}};
        let mut doc = document();
        let paper = doc.bottom_paper().unwrap();
        let ink = doc.scene().order()[0];
        let original = doc.clone();
        assert!(doc.move_occurrence_edit(paper, 0).is_err());
        assert!(doc.group_layers_edit(&[ink, paper], LayerBlend::Normal, "Group").is_err());
        assert!(doc.validate_duplicate_layers(&[paper]).is_err());
        assert!(doc.duplicate_layers_edit(&[ink, paper]).is_err());
        let root = doc.composition().result;
        let reorder = Edit::Stack(RecordChange::replace(&doc.artwork.stacks, root, Some(Stack { entries: vec![paper, ink] })).unwrap());
        assert!(doc.apply(reorder).is_err());
        assert_eq!(doc.artwork, original.artwork);
        let undo = doc.apply(doc.delete_layers_edit(&[paper]).unwrap()).unwrap();
        assert_eq!(doc.bottom_paper(), None);
        let redo = doc.apply(undo).unwrap();
        assert_eq!(doc.artwork, original.artwork);
        doc.apply(redo).unwrap();
        assert_eq!(doc.bottom_paper(), None);
    }
    #[test]
    fn edits_never_give_paper_a_mask() {
        use crate::{CoverageSnapshot, Edit, authored::{MaskUse, RecordChange}, operation_test_support as f};
        let mut doc = f::document([32; 2], &["Ink"]);
        let masked = |doc: &Document, owner| {
            let snapshot = CoverageSnapshot::reveal_all(doc.artwork.coverage.next_handle(), [32; 2], [0; 2]);
            let coverage = RecordChange::insert(&doc.artwork.coverage, snapshot.source);
            let mut occurrence = doc.scene().occurrence(owner).unwrap().clone();
            occurrence.mask = Some(MaskUse { source: coverage.handle, ..snapshot.use_ });
            Edit::Batch(vec![Edit::Coverage(coverage), Edit::Occurrence(RecordChange::replace(&doc.artwork.occurrences, owner, Some(occurrence)).unwrap())])
        };
        let original = doc.artwork.clone();
        let paper = doc.bottom_paper().unwrap();
        assert_eq!(doc.apply(masked(&doc, paper)), Err(crate::DocumentError::InvalidLayerOperation("Paper can't have a mask")));
        assert_eq!(doc.artwork, original);
        doc.apply(masked(&doc, f::id(&doc, "Ink"))).unwrap();
    }
    #[test]
    fn unmasked_fills_draw_above_without_entering_groups_and_own_masks_take_priority() {
        use crate::{DrawingRefusal, LayerBlend, operation_test_support as f};
        for program in ["paper", "solid_color", "gradient_fill"] {
            let mut doc = f::document([32; 2], &["Top", "Nearest"]);
            let fill = doc.bottom_paper().unwrap();
            if program != "paper" { f::effect(&mut doc, "Paper", program); }
            let nearest = f::id(&doc, "Nearest");
            doc.apply(doc.select_occurrence_edit(fill).unwrap()).unwrap();
            assert_eq!(doc.drawing_target(), doc.scene().source_target(nearest), "{program}");
            assert_eq!(doc.working.occurrence, Some(fill));
            doc.artwork.occurrences.get_mut(nearest).unwrap().locked = true;
            assert_eq!(doc.try_drawing_target(), Err(DrawingRefusal::BaseLocked));
            doc.artwork.occurrences.get_mut(nearest).unwrap().locked = false;
            let undo = doc.apply(doc.group_layers_edit(&[nearest], LayerBlend::Normal, "Group").unwrap()).unwrap();
            assert_eq!(doc.try_drawing_target(), Err(DrawingRefusal::Group));
            doc.apply(undo).unwrap();
            assert_eq!(doc.drawing_target(), doc.scene().source_target(nearest));
            let undo = doc.apply(doc.delete_layers_edit(&[f::id(&doc, "Top"), nearest]).unwrap()).unwrap();
            assert_eq!(doc.try_drawing_target(), Err(DrawingRefusal::NoLayer));
            if program == "paper" { continue; }
            let coverage = f::add_mask(&mut doc, fill, [32; 2], [0; 2]);
            assert_eq!(doc.drawing_target(), Some(SourceTarget::Coverage(coverage)));
            doc.apply(undo).unwrap();
            assert_eq!(doc.drawing_target(), Some(SourceTarget::Coverage(coverage)));
        }
    }
    #[test]
    fn strokes_use_the_nearest_paper_at_or_below_their_layer() {
        let doc = document();
        let scene = doc.scene();
        let [ink, paper] = [scene.order()[0], scene.order()[1]];
        assert_eq!(paper_occurrence(scene, Some(ink)), Some(paper));
        assert_eq!(paper_occurrence(scene, Some(paper)), Some(paper));
        assert_eq!(paper_occurrence(scene, None), Some(paper));
    }
}
