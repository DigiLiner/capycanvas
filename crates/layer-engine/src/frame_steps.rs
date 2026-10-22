use layer_core::{BrushExecution, Document, RasterOperationKind, Point, Rect};
use layer_render::{Dab, DabBatch, DabBatchKind};
use std::borrow::Cow;

pub(super) const MATERIAL_PAGE_JOBS_PER_FRAME: u32 = 512;

#[derive(Clone, Copy, Default)]
pub(super) struct FrameSteps {
    batch: usize,
    cursor: Option<[u32; 2]>,
}

impl FrameSteps {
    const SIDE: u32 = 1024;

    pub fn new(document: &Document, batches: &[DabBatch], dabs: &[Dab]) -> Option<Self> {
        let bake = dabs.is_empty() && batches.iter().any(|batch| Self::is_bake(document, batch) && (
            batch.damage.max.x - batch.damage.min.x > Self::SIDE as f32
                || batch.damage.max.y - batch.damage.min.y > Self::SIDE as f32
        ));
        (bake || material_page_jobs(batches) > 2 * MATERIAL_PAGE_JOBS_PER_FRAME).then(Self::default)
    }

    fn is_bake(document: &Document, batch: &DabBatch) -> bool {
        let DabBatchKind::RasterOperation(index) = batch.kind else { return false; };
        document.target_operations(batch.target)
            .and_then(|operations| operations.get(index as usize)).is_some_and(|operation| matches!(operation.kind,
                RasterOperationKind::Bake { .. } | RasterOperationKind::FrequencyDetail { .. }))
    }
    fn following(end: usize, batches: &[DabBatch]) -> Option<Self> {
        (end < batches.len()).then_some(Self { batch: end, cursor: None })
    }

    pub fn next<'a>(self, document: &Document, batches: &'a [DabBatch]) -> (Cow<'a, [DabBatch]>, Option<Self>) {
        let mut batch = batches[self.batch].clone();
        if !Self::is_bake(document, &batch) {
            let end = self.window_end(batches);
            return (Cow::Borrowed(&batches[self.batch..end]), Self::following(end, batches));
        }
        let extent = document.target_extent(batch.target);
        let min = [batch.damage.min.x, batch.damage.min.y].map(|v| v.max(0.) as u32 / 256 * 256);
        let max = [batch.damage.max.x.ceil() as u32, batch.damage.max.y.ceil() as u32];
        let max = [max[0].div_ceil(256).saturating_mul(256).min(extent[0]), max[1].div_ceil(256).saturating_mul(256).min(extent[1])];
        let [x, y] = self.cursor.unwrap_or(min);
        let end = [x.saturating_add(Self::SIDE).min(max[0]), y.saturating_add(Self::SIDE).min(max[1])];
        batch.damage = Rect { min: Point { x: x as f32, y: y as f32 }, max: Point { x: end[0] as f32, y: end[1] as f32 } };
        let next = if end[0] < max[0] {
            Some(Self { cursor: Some([end[0], y]), ..self })
        } else if end[1] < max[1] {
            Some(Self { cursor: Some([min[0], end[1]]), ..self })
        } else { Self::following(self.batch + 1, batches) };
        (Cow::Owned(vec![batch]), next)
    }

    fn window_end(self, batches: &[DabBatch]) -> usize {
        if matches!(batches[self.batch].kind, DabBatchKind::RasterOperation(_)) {
            return self.batch + 1;
        }
        let mut jobs = 0;
        let mut end = self.batch;
        while let Some(batch) = batches.get(end).filter(|batch| !matches!(batch.kind, DabBatchKind::RasterOperation(_))) {
            let more = material_page_jobs(std::slice::from_ref(batch));
            if end > self.batch && jobs + more > MATERIAL_PAGE_JOBS_PER_FRAME && !same_watercolor_update(&batches[end - 1], batch) {
                break;
            }
            jobs += more;
            end += 1;
        }
        end
    }
}

pub(super) fn material_page_jobs(batches: &[DabBatch]) -> u32 {
    batches
        .iter()
        .filter(|batch| batch.kind == DabBatchKind::Persistent && batch.style.execution != BrushExecution::Dry
            && !batch.style.execution.retouches())
        .map(|batch| covered_tiles(batch.damage).saturating_mul(if batch.style.execution == BrushExecution::Smudge
            && batch.style.blender.kind == layer_core::BlenderKind::Dither {
                batch.dab_count.div_ceil(super::MAX_SMUDGE_DABS_PER_BATCH as u32)
            } else { 1 }))
        .sum()
}

pub(super) fn covered_tiles(rect: Rect) -> u32 {
    if rect.is_empty() {
        return 0;
    }
    let tile = layer_core::raster::TILE_SIZE as f32;
    let span = |min: f32, max: f32| ((max / tile).ceil() - (min.max(0.) / tile).floor()).max(0.) as u32;
    span(rect.min.x, rect.max.x) * span(rect.min.y, rect.max.y)
}

fn same_watercolor_update(earlier: &DabBatch, later: &DabBatch) -> bool {
    earlier.style.execution == BrushExecution::Watercolor
        && later.style.execution == BrushExecution::Watercolor
        && earlier.kind == later.kind
        && earlier.stroke_id == later.stroke_id
        && earlier.target == later.target
        && earlier.material_update == later.material_update
}
