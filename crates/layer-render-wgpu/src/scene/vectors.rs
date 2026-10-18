use super::*;
use crate::vector::{Batch, Layer, Pool, tiles::{self, Frame, LevelGeometry, TILE}};
use layer_core::authored::{VectorHandle, vector::{ItemId, PaintEntry, PathItem, Record, VectorObject}};
use std::collections::{HashMap, HashSet};

const CELL: f64 = 256.;
const POOL_BYTES: u64 = 128 << 20;
const DERIVED_BYTES: usize = 256 << 20;

struct Derived { level: u32, offset: [i64; 2], geometry: Arc<LevelGeometry>, base: Option<u32>, used: u64 }
struct Item { path: PathItem, chunk: usize, index: u32, bounds: [f64; 4], cells: [i32; 4], colors: Arc<[[f32; 4]]>, derived: Vec<Derived>, seen: u64 }

#[derive(Default)]
struct LayerCache {
    root: VectorObject,
    items: Vec<Option<Item>>,
    ids: HashMap<ItemId, u32>,
    free: Vec<u32>,
    grid: HashMap<[i32; 2], Vec<u32>>,
    chunks: HashMap<usize, u32>,
    palette: Option<(layer_core::color::RgbSpace, layer_core::BlendSpace)>,
    appended: Option<Vec<u32>>,
}

fn chunk_key(chunk: &layer_core::authored::Resource<layer_core::authored::vector::VectorChunk>) -> usize { Arc::as_ptr(chunk.storage()) as usize }
fn cells(bounds: [f64; 4]) -> [i32; 4] {
    [(bounds[0] / CELL).floor() as i32, (bounds[1] / CELL).floor() as i32, (bounds[2] / CELL).floor() as i32 + 1, (bounds[3] / CELL).floor() as i32 + 1]
}
fn same(a: &PathItem, b: &PathItem) -> bool {
    Arc::ptr_eq(&a.geometry, &b.geometry) && Arc::ptr_eq(&a.style, &b.style) && a.fill_rule == b.fill_rule && a.hidden == b.hidden
}
fn colors(path: &PathItem, palette: (layer_core::color::RgbSpace, layer_core::BlendSpace)) -> Arc<[[f32; 4]]> {
    path.style.entries.iter().map(|entry| {
        let paint = match entry { PaintEntry::Fill(f) => f.paint.rgb(), PaintEntry::Stroke(s) => s.paint.rgb(), PaintEntry::Opaque(_) => None };
        paint.and_then(|c| c.linear_in(palette.0).ok()).map_or([0.; 4], |[r, g, b, a]| palette.1.composite(palette.0, [r * a, g * a, b * a, a]))
    }).collect()
}

impl LayerCache {
    fn insert(&mut self, path: PathItem, chunk: usize, index: u32, palette: (layer_core::color::RgbSpace, layer_core::BlendSpace)) -> [f64; 4] {
        let bounds = path.bounds();
        let item = Item { colors: colors(&path, palette), cells: cells(bounds), bounds, chunk, index, derived: Vec::new(), seen: 0, path };
        let slot = self.free.pop().unwrap_or_else(|| { self.items.push(None); self.items.len() as u32 - 1 });
        let [x0, y0, x1, y1] = item.cells;
        for y in y0..y1 { for x in x0..x1 { self.grid.entry([x, y]).or_default().push(slot); } }
        self.ids.insert(item.path.id, slot);
        self.items[slot as usize] = Some(item);
        bounds
    }
    fn remove(&mut self, slot: u32, pool: &mut Pool) -> Item {
        let item = self.items[slot as usize].take().unwrap();
        let [x0, y0, x1, y1] = item.cells;
        for y in y0..y1 { for x in x0..x1 {
            if let Some(list) = self.grid.get_mut(&[x, y]) { if let Some(i) = list.iter().position(|s| *s == slot) { list.swap_remove(i); } if list.is_empty() { self.grid.remove(&[x, y]); } }
        }}
        for derived in &item.derived { if let Some(base) = derived.base { pool.release(base, &derived.geometry); } }
        self.ids.remove(&item.path.id);
        self.free.push(slot);
        item
    }
    fn update(&mut self, object: &VectorObject, palette: (layer_core::color::RgbSpace, layer_core::BlendSpace), pool: &mut Pool) -> Vec<[f64; 4]> {
        if self.root.same_root(object) && self.palette == Some(palette) { return Vec::new(); }
        self.appended = None;
        let first = self.root.chunks().is_empty() && self.items.iter().all(Option::is_none);
        let mut added = Vec::new();
        let mut other = !first && self.palette.is_some() && self.palette != Some(palette);
        let mut damage = Vec::new();
        if self.palette != Some(palette) {
            self.palette = Some(palette);
            for item in self.items.iter_mut().flatten() { item.colors = colors(&item.path, palette); damage.push(item.bounds); }
        }
        let previous_root = std::mem::take(&mut self.root);
        let old: HashSet<usize> = previous_root.chunks().iter().map(chunk_key).collect();
        let new: HashSet<usize> = object.chunks().iter().map(chunk_key).collect();
        let mut previous: HashMap<ItemId, (&PathItem, usize)> = HashMap::new();
        let mut rank = 0usize;
        for chunk in previous_root.chunks().iter().filter(|c| !new.contains(&chunk_key(c))) {
            for record in chunk.records() { if let Record::Path(path) = record { previous.insert(path.id, (path, rank)); rank += 1; } }
        }
        self.chunks = object.chunks().iter().enumerate().map(|(i, c)| (chunk_key(c), i as u32)).collect();
        let mut common = Vec::new();
        let mut seen = HashSet::new();
        for (chunk, records) in object.chunks().iter().filter(|c| !old.contains(&chunk_key(c))).map(|c| (chunk_key(c), c.records())) {
            for (index, record) in records.iter().enumerate() {
                let Record::Path(path) = record else { continue };
                seen.insert(path.id);
                match (previous.get(&path.id), self.ids.get(&path.id).copied()) {
                    (Some((before, rank)), Some(slot)) if same(before, path) => {
                        let item = self.items[slot as usize].as_mut().unwrap();
                        item.chunk = chunk; item.index = index as u32; item.path = path.clone();
                        common.push((*rank, slot));
                    }
                    (_, Some(slot)) => {
                        other = true;
                        damage.push(self.remove(slot, pool).bounds);
                        damage.push(self.insert(path.clone(), chunk, index as u32, palette));
                    }
                    (_, None) => { damage.push(self.insert(path.clone(), chunk, index as u32, palette)); added.push(path.id); }
                }
            }
        }
        for id in previous.keys().filter(|id| !seen.contains(*id)) {
            if let Some(slot) = self.ids.get(id).copied() { other = true; damage.push(self.remove(slot, pool).bounds); }
        }
        let mut ranks: Vec<usize> = common.iter().map(|(rank, _)| *rank).collect();
        if !ranks.is_sorted() {
            other = true;
            ranks.sort_unstable();
            for ((rank, slot), expected) in common.iter().zip(ranks) { if *rank != expected { damage.push(self.items[*slot as usize].as_ref().unwrap().bounds); } }
        }
        if !first && !other && !added.is_empty() {
            let set: HashSet<ItemId> = added.iter().copied().collect();
            let leading = object.records().take_while(|(_, record)| record.id().is_some_and(|id| set.contains(&id))).count();
            if leading == added.len() { self.appended = Some(added.iter().filter_map(|id| self.ids.get(id).copied()).collect()); }
        }
        self.root = object.clone();
        damage
    }
    fn order(&self, item: &Item) -> u64 { u64::from(self.chunks.get(&item.chunk).copied().unwrap_or(0)) << 32 | u64::from(item.index) }
}

pub(super) fn coalesce(rects: Vec<PixelRect>) -> Vec<PixelRect> {
    if rects.len() <= 32 { return rects; }
    let mut pages = std::collections::BTreeSet::new();
    for rect in rects.iter().filter(|r| !r.is_empty()) { pages.extend(page_coordinates(*rect).map(|[x, y]| [y, x])); }
    let mut spans: Vec<PixelRect> = Vec::new();
    for [y, x] in pages {
        let page = page_rect([x, y]);
        match spans.last_mut() {
            Some(last) if last.min_y() == page.min_y() && last.max_x() == page.min_x() => *last = last.union(page),
            _ => spans.push(page),
        }
    }
    spans
}
pub(crate) fn preview_bounds(scene: SceneView<'_>, preview: &layer_render::VectorPreview) -> DocRect {
    let offset = scene.occurrence_offset64(preview.owner);
    let [x0, y0, x1, y1] = preview.path.bounds();
    DocRect { min: [(x0 + offset[0]).floor() as i64 - 16, (y0 + offset[1]).floor() as i64 - 16], max: [(x1 + offset[0]).ceil() as i64 + 16, (y1 + offset[1]).ceil() as i64 + 16] }
}
fn half_widths(path: &PathItem) -> Option<(f64, &layer_core::authored::vector::Subpath, Option<&layer_core::authored::vector::WidthProfile>)> {
    let stroke = path.style.strokes().next()?;
    Some((stroke.width * 0.5, path.geometry.subpaths().first()?, stroke.profile.as_deref()))
}
pub(crate) fn preview_damage(scene: SceneView<'_>, old: Option<&layer_render::VectorPreview>, new: Option<&layer_render::VectorPreview>) -> Vec<DocRect> {
    let whole = |p: &layer_render::VectorPreview| preview_bounds(scene, p);
    let (Some(old), Some(new)) = (old, new) else { return old.into_iter().chain(new).map(whole).collect() };
    let changed_style = old.owner != new.owner || old.path.style.entries.len() != new.path.style.entries.len()
        || old.path.style.entries.iter().zip(new.path.style.entries.iter()).any(|(a, b)| match (a, b) {
            (PaintEntry::Stroke(a), PaintEntry::Stroke(b)) => a.paint != b.paint || a.cap != b.cap || a.join != b.join,
            (a, b) => a != b,
        });
    let (Some((old_half, old_line, old_profile)), Some((new_half, new_line, new_profile)), false) = (half_widths(&old.path), half_widths(&new.path), changed_style) else { return vec![whole(old), whole(new)] };
    let width = |half: f64, profile: Option<&layer_core::authored::vector::WidthProfile>, i: usize| profile.map_or([half; 2], |p| p.at(i as u32, 0.).map(|f| f * half));
    let shared = old_line.len().min(new_line.len());
    let first = (0..shared).find(|&i| old_line.anchor(i) != new_line.anchor(i) || width(old_half, old_profile, i) != width(new_half, new_profile, i)).unwrap_or(shared).saturating_sub(1);
    let offset = scene.occurrence_offset64(new.owner);
    let mut bounds: Option<[f64; 4]> = None;
    for (line, half, profile) in [(old_line, old_half, old_profile), (new_line, new_half, new_profile)] {
        for i in first..line.len() {
            let p = line.anchor(i).point();
            let r = width(half, profile, i).into_iter().fold(0., f64::max) + 2.;
            bounds = Some(layer_core::authored::vector::union(bounds, [p[0] - r, p[1] - r, p[0] + r, p[1] + r]));
        }
    }
    bounds.map(|[x0, y0, x1, y1]| DocRect { min: [(x0 + offset[0]).floor() as i64 - 1, (y0 + offset[1]).floor() as i64 - 1], max: [(x1 + offset[0]).ceil() as i64 + 1, (y1 + offset[1]).ceil() as i64 + 1] }).into_iter().collect()
}
pub(super) fn edit_damage(before: &VectorObject, after: &VectorObject) -> Vec<[f64; 4]> {
    if before.same_root(after) { return Vec::new(); }
    let old: HashSet<usize> = before.chunks().iter().map(chunk_key).collect();
    let new: HashSet<usize> = after.chunks().iter().map(chunk_key).collect();
    let mut previous: HashMap<ItemId, (&PathItem, usize)> = HashMap::new();
    for (rank, path) in before.chunks().iter().filter(|c| !new.contains(&chunk_key(c))).flat_map(|c| c.records()).filter_map(Record::path).enumerate() { previous.insert(path.id, (path, rank)); }
    let mut damage = Vec::new();
    let mut ranks = Vec::new();
    let mut seen = HashSet::new();
    for path in after.chunks().iter().filter(|c| !old.contains(&chunk_key(c))).flat_map(|c| c.records()).filter_map(Record::path) {
        seen.insert(path.id);
        match previous.get(&path.id) {
            Some((before, rank)) if same(before, path) => ranks.push((*rank, path.bounds())),
            Some((before, _)) => { damage.push(before.bounds()); damage.push(path.bounds()); }
            None => damage.push(path.bounds()),
        }
    }
    damage.extend(previous.iter().filter(|(id, _)| !seen.contains(*id)).map(|(_, (path, _))| path.bounds()));
    if !ranks.is_sorted_by_key(|(rank, _)| *rank) { damage.extend(ranks.into_iter().map(|(_, bounds)| bounds)); }
    damage
}

#[derive(Default)]
pub(super) struct Vectors {
    layers: HashMap<VectorHandle, LayerCache>,
    pool: Option<Pool>,
    frame: u64,
    stamp: u64,
    preview: Option<((usize, u32, [u64; 2]), u32, Arc<LevelGeometry>, Arc<[[f32; 4]]>)>,
    derived_bytes: usize,
    buffers: [Option<wgpu::Buffer>; 4],
}
#[derive(Clone)]
pub(super) struct VectorJob { pub batch: Batch, pub output: wgpu::TextureView, pub base: Option<(wgpu::TextureView, [i32; 2])> }

impl Scene {
    pub(super) fn vector_palette(r: &WgpuRasterizer, packet: FramePacket<'_>) -> (layer_core::color::RgbSpace, layer_core::BlendSpace) {
        (r.device.working_space(), packet.blend_space)
    }
    pub(super) fn vector_tile(&mut self, r: &WgpuRasterizer, packet: FramePacket<'_>, owner: OccurrenceHandle, coordinate: [u32; 2]) -> usize {
        let origin = coordinate.map(|v| i64::from(v * PAGE_SIZE));
        let preview = r.vector_preview.as_ref().filter(|p| self.object_display && p.owner == owner).map(|p| p.path.clone());
        let batch = self.vectors.batch_with(packet.scene, owner, origin, 0, [PAGE_SIZE; 2], packet.document_extent, Self::vector_palette(r, packet), preview.as_ref());
        let output = self.alloc(r, wgpu::Color::TRANSPARENT);
        if batch.tile_count() > 0 { self.jobs.push(Job::Vector(Box::new(VectorJob { batch, output: self.pool[output].view.clone(), base: None }))); }
        output
    }
    pub(super) fn vector_thumbnail(&mut self, r: &mut WgpuRasterizer, packet: FramePacket<'_>, owner: OccurrenceHandle, grid: display_mips::Plan,
        encoder: &mut crate::submission::CommandEncoder) -> Result<(wgpu::Texture, wgpu::TextureView), GpuRasterError> {
        let target = create_color_target(&r.device, [PAGE_SIZE; 2], "vector layer thumbnail");
        r.encode_clear(encoder, &target.1, "vector layer thumbnail");
        let batch = self.vectors.batch(packet.scene, owner, [0, 0], grid.level, grid.size, packet.document_extent, (r.device.working_space(), layer_core::BlendSpace::Linear));
        if batch.tile_count() > 0 { self.vectors.encode(r, encoder, &VectorJob { batch, output: target.1.clone(), base: None })?; }
        Ok(target)
    }
    pub(super) fn vector_damage(&mut self, r: &WgpuRasterizer, packet: FramePacket<'_>) {
        self.scale_sources.vector_preview = r.vector_preview.as_ref().zip(r.vector_preview_shown).map(|(p, (_, _, bounds))| (p.owner, p.path.clone(), bounds));
        self.scale_sources.preview_damage = r.vector_preview_damage.clone();
        self.vectors.begin_frame();
        self.vectors.retain(packet.scene);
        let palette = Self::vector_palette(r, packet);
        for &owner in packet.scene.order() {
            let rects = self.vectors.damage(packet.scene, owner, palette);
            if rects.is_empty() { continue; }
            let extent = packet.document_extent;
            let rects = coalesce(rects.into_iter().map(|rect| rect.in_frame(extent)).filter(|rect| !rect.is_empty()).collect());
            let damage = self.scale_sources.object_damage.entry(Some(owner)).or_insert(scale::Damage::EMPTY);
            damage.regions.extend(rects.into_iter().map(|rect| rect.intersect(PixelRect::full(extent))));
        }
        self.vectors.trim();
    }
}

impl Vectors {
    pub fn begin_frame(&mut self) { self.frame += 1; }
    pub fn retain(&mut self, scene: SceneView<'_>) {
        let live: HashSet<VectorHandle> = scene.order().iter().filter_map(|h| scene.vector_handle(*h)).collect();
        let pool = self.pool.get_or_insert_with(|| Pool::new(POOL_BYTES));
        self.layers.retain(|handle, layer| {
            let keep = live.contains(handle);
            if !keep { for item in layer.items.iter().flatten() { for d in &item.derived { if let Some(b) = d.base { pool.release(b, &d.geometry); } } } }
            keep
        });
    }
    pub fn damage(&mut self, scene: SceneView<'_>, owner: OccurrenceHandle, palette: (layer_core::color::RgbSpace, layer_core::BlendSpace)) -> Vec<DocRect> {
        let Some(handle) = scene.vector_handle(owner) else { return Vec::new() };
        let Some(object) = scene.vector(handle) else { return Vec::new() };
        let offset = scene.occurrence_offset64(owner);
        let pool = self.pool.get_or_insert_with(|| Pool::new(POOL_BYTES));
        let layer = self.layers.entry(handle).or_default();
        let rects = layer.update(object, palette, pool);
        rects.into_iter().map(|[x0, y0, x1, y1]| DocRect {
            min: [(x0 + offset[0]).floor() as i64 - 16, (y0 + offset[1]).floor() as i64 - 16],
            max: [(x1 + offset[0]).ceil() as i64 + 16, (y1 + offset[1]).ceil() as i64 + 16],
        }).collect()
    }
    pub fn bounds(scene: SceneView<'_>, owner: OccurrenceHandle) -> DocRect {
        let Some(object) = scene.vector_layer(owner) else { return DocRect::default() };
        let offset = scene.occurrence_offset64(owner);
        object.bounds().map_or(DocRect::default(), |[x0, y0, x1, y1]| DocRect {
            min: [(x0 + offset[0]).floor() as i64 - 16, (y0 + offset[1]).floor() as i64 - 16],
            max: [(x1 + offset[0]).ceil() as i64 + 16, (y1 + offset[1]).ceil() as i64 + 16],
        })
    }
    #[expect(clippy::too_many_arguments, reason = "A window names its layer, placement, level, extent and color domain")]
    pub fn batch(&mut self, scene: SceneView<'_>, owner: OccurrenceHandle, origin: [i64; 2], level: u32, size: [u32; 2], extent: [u32; 2], palette: (layer_core::color::RgbSpace, layer_core::BlendSpace)) -> Batch {
        self.batch_with(scene, owner, origin, level, size, extent, palette, None)
    }
    #[expect(clippy::too_many_arguments, reason = "A window names its layer, placement, level, extent and color domain")]
    pub fn append_batch(&mut self, scene: SceneView<'_>, owner: OccurrenceHandle, origin: [i64; 2], level: u32, size: [u32; 2], extent: [u32; 2], palette: (layer_core::color::RgbSpace, layer_core::BlendSpace)) -> Option<Batch> {
        let handle = scene.vector_handle(owner)?;
        let object = scene.vector(handle)?;
        let pool = self.pool.get_or_insert_with(|| Pool::new(POOL_BYTES));
        let layer = self.layers.get_mut(&handle)?;
        layer.update(object, palette, pool);
        let slots = layer.appended.clone()?;
        let offset = scene.occurrence_offset64(owner);
        let side = 1i64 << level;
        let tiles = extent.map(|v| (i64::from(v).div_euclid(side) as i32).div_euclid(TILE) + 1);
        let frame = Frame { scale: 1. / side as f64, origin: offset, clip: [0, 0, tiles[0], tiles[1]] };
        let integer_offset = offset.map(|v| v as i64);
        let mut keyed: Vec<(u64, u32)> = slots.iter().map(|s| (layer.order(layer.items[*s as usize].as_ref().unwrap()), *s)).collect();
        keyed.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        let mut entries = Vec::with_capacity(keyed.len());
        for (_, slot) in keyed {
            let item = layer.items[slot as usize].as_mut()?;
            let at = match item.derived.iter().position(|d| d.level == level && d.offset == integer_offset) {
                Some(at) => at,
                None => {
                    let geometry = Arc::new(tiles::derive(&item.path, &frame));
                    self.derived_bytes += geometry.bytes();
                    item.derived.push(Derived { level, offset: integer_offset, geometry, base: None, used: 0 });
                    item.derived.len() - 1
                }
            };
            let derived = &mut item.derived[at];
            derived.used = self.frame;
            if derived.base.is_none() { derived.base = pool.allocate(&derived.geometry); }
            entries.push((derived.geometry.clone(), derived.base?, item.colors.clone()));
        }
        let layers: Vec<Layer> = entries.iter().map(|(geometry, base, colors)| Layer { geometry, base: *base, colors }).collect();
        let mut batch = Batch::default();
        batch.build(&layers, [origin[0].div_euclid(side), origin[1].div_euclid(side)], size);
        Some(batch)
    }
    #[expect(clippy::too_many_arguments, reason = "A window names its layer, placement, level, extent, color domain and provisional path")]
    fn batch_with(&mut self, scene: SceneView<'_>, owner: OccurrenceHandle, origin: [i64; 2], level: u32, size: [u32; 2], extent: [u32; 2], palette: (layer_core::color::RgbSpace, layer_core::BlendSpace), preview: Option<&Arc<PathItem>>) -> Batch {
        let mut batch = Batch::default();
        let Some(handle) = scene.vector_handle(owner) else { return batch };
        let Some(object) = scene.vector(handle) else { return batch };
        let offset = scene.occurrence_offset64(owner);
        let frame_number = self.frame;
        self.stamp += 1;
        let stamp = self.stamp;
        let side = 1i64 << level;
        let level_origin = [origin[0].div_euclid(side), origin[1].div_euclid(side)];
        let pool = self.pool.get_or_insert_with(|| Pool::new(POOL_BYTES));
        let layer = self.layers.entry(handle).or_default();
        layer.update(object, palette, pool);
        let window = [(origin[0] - offset[0].floor() as i64) as f64, (origin[1] - offset[1].floor() as i64) as f64];
        let span = [f64::from(size[0]) * side as f64, f64::from(size[1]) * side as f64];
        let [cx0, cy0, cx1, cy1] = cells([window[0] - 16., window[1] - 16., window[0] + span[0] + 16., window[1] + span[1] + 16.]);
        let mut slots = Vec::new();
        for y in cy0..cy1 { for x in cx0..cx1 {
            for slot in layer.grid.get(&[x, y]).into_iter().flatten() {
                let item = layer.items[*slot as usize].as_mut().unwrap();
                if item.seen == stamp { continue; }
                item.seen = stamp;
                let b = item.bounds;
                if b[2] < window[0] - 16. || b[0] > window[0] + span[0] + 16. || b[3] < window[1] - 16. || b[1] > window[1] + span[1] + 16. { continue; }
                slots.push(*slot);
            }
        }}
        let mut keyed: Vec<(u64, u32)> = slots.into_iter().map(|s| (layer.order(layer.items[s as usize].as_ref().unwrap()), s)).collect();
        keyed.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        let tiles = extent.map(|v| (i64::from(v).div_euclid(side) as i32).div_euclid(TILE) + 1);
        let frame = Frame { scale: 1. / side as f64, origin: offset, clip: [0, 0, tiles[0], tiles[1]] };
        let mut entries: Vec<(Arc<LevelGeometry>, u32, Arc<[[f32; 4]]>)> = Vec::with_capacity(keyed.len());
        for (_, slot) in keyed {
            let item = layer.items[slot as usize].as_mut().unwrap();
            let integer_offset = offset.map(|v| v as i64);
            let at = match item.derived.iter().position(|d| d.level == level && d.offset == integer_offset) {
                Some(at) => at,
                None => {
                    let geometry = Arc::new(tiles::derive(&item.path, &frame));
                    self.derived_bytes += geometry.bytes();
                    item.derived.push(Derived { level, offset: integer_offset, geometry, base: None, used: 0 });
                    item.derived.len() - 1
                }
            };
            let derived = &mut item.derived[at];
            derived.used = frame_number;
            if derived.base.is_none() { derived.base = pool.allocate(&derived.geometry); }
            if derived.base.is_none() {
                let geometry = derived.geometry.clone();
                for other in layer.items.iter_mut().flatten() {
                    for d in other.derived.iter_mut().filter(|d| d.used < frame_number) { if let Some(b) = d.base.take() { pool.release(b, &d.geometry); } }
                }
                let item = layer.items[slot as usize].as_mut().unwrap();
                item.derived[at].base = pool.allocate(&geometry);
            }
            let item = layer.items[slot as usize].as_mut().unwrap();
            let derived = &item.derived[at];
            let Some(base) = derived.base else { continue };
            entries.push((derived.geometry.clone(), base, item.colors.clone()));
        }
        if let Some(path) = preview {
            let key = (Arc::as_ptr(path) as usize, level, offset.map(f64::to_bits));
            if self.preview.as_ref().is_none_or(|p| p.0 != key) {
                if let Some(previous) = self.preview.take() { pool.release(previous.1, &previous.2); }
                let geometry = Arc::new(tiles::derive(path, &frame));
                if let Some(base) = pool.allocate(&geometry) { self.preview = Some((key, base, geometry, colors(path, palette))); }
            }
            if let Some((_, base, geometry, colors)) = &self.preview { entries.push((geometry.clone(), *base, colors.clone())); }
        }
        let layers: Vec<Layer> = entries.iter().map(|(geometry, base, colors)| Layer { geometry, base: *base, colors }).collect();
        batch.build(&layers, level_origin, size);
        batch
    }
    #[expect(clippy::too_many_arguments, reason = "A preview window names its path, placement, level, extent and color domain")]
    pub fn preview_batch(&mut self, path: &Arc<PathItem>, offset: [f64; 2], origin: [i64; 2], level: u32, size: [u32; 2], extent: [u32; 2], palette: (layer_core::color::RgbSpace, layer_core::BlendSpace)) -> Batch {
        let side = 1i64 << level;
        let key = (Arc::as_ptr(path) as usize, level, offset.map(f64::to_bits));
        let pool = self.pool.get_or_insert_with(|| Pool::new(POOL_BYTES));
        if self.preview.as_ref().is_none_or(|p| p.0 != key) {
            if let Some(previous) = self.preview.take() { pool.release(previous.1, &previous.2); }
            let tiles = extent.map(|v| (i64::from(v).div_euclid(side) as i32).div_euclid(TILE) + 1);
            let geometry = Arc::new(tiles::derive(path, &Frame { scale: 1. / side as f64, origin: offset, clip: [0, 0, tiles[0], tiles[1]] }));
            let Some(base) = pool.allocate(&geometry) else { return Batch::default() };
            self.preview = Some((key, base, geometry, colors(path, palette)));
        }
        let (_, base, geometry, colors) = self.preview.as_ref().unwrap();
        let mut batch = Batch::default();
        batch.build(&[Layer { geometry, base: *base, colors }], [origin[0].div_euclid(side), origin[1].div_euclid(side)], size);
        batch
    }
    pub fn encode(&mut self, r: &mut WgpuRasterizer, encoder: &mut wgpu::CommandEncoder, job: &VectorJob) -> Result<(), GpuRasterError> {
        let pool = self.pool.get_or_insert_with(|| Pool::new(POOL_BYTES));
        let buffer = pool.flush(&r.device, encoder, &mut r.uploads)?;
        r.scene_pipelines.vector.encode(&r.device, encoder, &mut r.uploads, &mut self.buffers, &job.batch, &buffer, job.base.as_ref().map(|(view, origin)| (view, *origin)), &r.empty_view, &job.output)
    }
    pub fn trim(&mut self) {
        if self.derived_bytes <= DERIVED_BYTES { return; }
        let frame = self.frame;
        let pool = self.pool.get_or_insert_with(|| Pool::new(POOL_BYTES));
        let mut freed = 0usize;
        for layer in self.layers.values_mut() {
            for item in layer.items.iter_mut().flatten() {
                item.derived.retain(|d| {
                    let keep = d.used + 2 >= frame;
                    if !keep { freed += d.geometry.bytes(); if let Some(b) = d.base { pool.release(b, &d.geometry); } }
                    keep
                });
            }
        }
        self.derived_bytes = self.derived_bytes.saturating_sub(freed);
    }
}

#[cfg(test)]
#[path = "vector_tests.rs"]
mod tests;
