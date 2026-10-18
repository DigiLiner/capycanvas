//! Hatching under a translucent Tones layer, with the active "Ink" layer between.
//! Usage: crowded_vector_fixture OUTPUT.capy WIDTH HEIGHT PATHS [--paint]
use layer_core::{Edit, Editor, EvaluationContext, Occurrence, OccurrenceContent, PaintSource, RecordChange, authored::{PortableId, vector::*}, color::RgbColor};
use std::{fs::File, io::{BufWriter, Write}, sync::{Arc, atomic::AtomicBool}};

struct Random(u64);
impl Random {
    fn next(&mut self) -> f64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; (self.0 >> 11) as f64 / (1u64 << 53) as f64 }
    fn range(&mut self, a: f64, b: f64) -> f64 { a + (b - a) * self.next() }
}
fn paint(rgba: [f32; 4]) -> Paint { Paint::color(RgbColor::new(Default::default(), rgba).unwrap()) }
fn hatching(extent: [f64; 2], count: u64) -> Vec<Record> {
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    (0..count).map(|i| {
        let start = [random.range(0., extent[0]), random.range(0., extent[1])];
        let (angle, length, bend) = (random.range(0.4, 0.9), random.range(40., 220.), random.range(-20., 20.));
        let points: Vec<[f64; 2]> = (0..4).map(|k| { let t = f64::from(k) / 3.; [start[0] + angle.cos() * length * t - angle.sin() * bend * t * (1. - t) * 4., start[1] + angle.sin() * length * t + angle.cos() * bend * t * (1. - t) * 4.] }).collect();
        let anchors: Vec<Anchor> = points.iter().map(|p| Anchor::corner(*p)).collect();
        let mut stroke = Stroke::new(paint([random.next() as f32 * 0.3, random.next() as f32 * 0.3, random.next() as f32 * 0.3, 1.]), random.range(2., 6.));
        (stroke.cap, stroke.join) = (Cap::Round, Join::Round);
        stroke.profile = Some(Arc::new(WidthProfile::new(vec![WidthKnot::new(0, 0., 0.1, 0.1), WidthKnot::new(1, 0.5, 1., 1.), WidthKnot::new(2, 1., 0.05, 0.05)]).unwrap()));
        Record::Path(PathItem::new(ItemId(i + 1), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)]))))
    }).collect()
}
fn tones(extent: [f64; 2]) -> Vec<Record> {
    (0..12u64).map(|i| {
        let x = extent[0] * i as f64 / 12.;
        let corners = [[x, 0.], [x + extent[0] / 20., 0.], [x + extent[0] / 20., extent[1]], [x, extent[1]]].map(Anchor::corner);
        Record::Path(PathItem::new(ItemId(i + 1), Arc::new(PathGeometry::new(vec![Subpath::new(&corners, true)]).unwrap()),
            Arc::new(Style::new(vec![PaintEntry::Fill(Fill { paint: paint([0.2, 0.4, 0.9, 0.25]), unknown: None })]))))
    }).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [output, width, height, count] = [1, 2, 3, 4].map(|i| args.get(i).cloned().ok_or("Usage: crowded_vector_fixture OUTPUT.capy WIDTH HEIGHT PATHS [--paint]"));
    let (output, width, height, count): (String, u32, u32, u64) = (output?, width?.parse()?, height?.parse()?, count?.parse()?);
    let raster_ink = args.iter().any(|arg| arg == "--paint");
    let extent = [f64::from(width), f64::from(height)];
    let mut document = layer_core::Document::new(PortableId::random(), width, height, layer_core::DocumentNames { paint: "Base".into(), paper: "Paper".into() });
    let (hatched, edit) = document.create_vector_layer_edit(if raster_ink { "Hatching" } else { "Ink" }, VectorObject::from_records(hatching(extent, count)), None, 0)?;
    document.apply(edit)?;
    let ink = if raster_ink {
        let source = RecordChange::insert(&document.artwork.paint, PaintSource { color_mode: Default::default(), domain: [width, height], raster: Default::default(), base: None, operations: Default::default() });
        let occurrence = RecordChange::insert(&document.artwork.occurrences, Occurrence::new(OccurrenceContent::Paint(source.handle), "Ink"));
        let stack = document.composition().result;
        let mut entries = document.artwork.stacks.get(stack).unwrap().clone();
        let at = entries.entries.iter().position(|h| *h == hatched).ok_or("The hatching layer is missing")?;
        entries.entries.insert(at, occurrence.handle);
        let membership = RecordChange::replace(&document.artwork.stacks, stack, Some(entries))?;
        let handle = occurrence.handle;
        document.apply(Edit::Batch(vec![Edit::Paint(source), Edit::Occurrence(occurrence), Edit::Stack(membership)]))?;
        handle
    } else { hatched };
    let (_, edit) = document.create_vector_layer_edit("Tones", VectorObject::from_records(tones(extent)), None, 0)?;
    document.apply(edit)?;
    document.apply(document.select_occurrence_edit(ink)?)?;
    let capture = Editor::new(document).capture(0, EvaluationContext::default())?;
    let cancelled = AtomicBool::new(false);
    let prepared = layer_core::package::codec::PreparedPackage::prepare(&capture, None, &cancelled)?;
    let mut writer = BufWriter::new(File::create(output)?);
    prepared.write(&mut writer, &cancelled)?;
    writer.flush()?;
    Ok(())
}
