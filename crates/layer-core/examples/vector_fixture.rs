use layer_core::{Editor, EvaluationContext, authored::{Artwork, vector::*}, color::RgbColor};
use std::{fs::File, io::{BufWriter, Write}, sync::{Arc, atomic::AtomicBool}};

fn paint(rgba: [f32; 4]) -> Paint { Paint::color(RgbColor::new(Default::default(), rgba).unwrap()) }
fn stroke(rgba: [f32; 4], width: f64, cap: Cap, join: Join) -> PaintEntry {
    let mut stroke = Stroke::new(paint(rgba), width);
    stroke.cap = cap;
    stroke.join = join;
    PaintEntry::Stroke(stroke)
}
fn fill(rgba: [f32; 4]) -> PaintEntry { PaintEntry::Fill(Fill { paint: paint(rgba), unknown: None }) }
fn path(id: u64, subpaths: Vec<Subpath>, entries: Vec<PaintEntry>) -> PathItem {
    PathItem::new(ItemId(id), Arc::new(PathGeometry::new(subpaths).unwrap()), Arc::new(Style::new(entries)))
}
fn open(points: &[[f64; 2]]) -> Subpath { Subpath::new(&points.iter().map(|p| Anchor::corner(*p)).collect::<Vec<_>>(), false) }
fn closed(points: &[[f64; 2]]) -> Subpath { Subpath::new(&points.iter().map(|p| Anchor::corner(*p)).collect::<Vec<_>>(), true) }
fn curve(points: &[[f64; 2]], tangent: [f64; 2]) -> Subpath {
    Subpath::new(&points.iter().map(|p| Anchor::new(*p, [-tangent[0], -tangent[1]], tangent, AnchorKind::SMOOTH)).collect::<Vec<_>>(), false)
}
fn profile(knots: &[(u32, f64, f64, f64)]) -> Option<Arc<WidthProfile>> {
    Some(Arc::new(WidthProfile::new(knots.iter().map(|&(s, t, l, r)| WidthKnot::new(s, t, l, r)).collect()).unwrap()))
}

fn shapes() -> Vec<Record> {
    let ink = [0.08, 0.1, 0.16, 1.];
    let mut records = Vec::new();
    let mut rounded = path(1, vec![open(&[[0., 0.]])], vec![stroke([0.9, 0.4, 0.1, 1.], 6., Cap::Butt, Join::Miter), fill([0.95, 0.8, 0.3, 0.9])]);
    rounded = rounded.with_shape(LiveShape { kind: ShapeKind::Rectangle { radii: [0., 10., 18., 4.] }, center: [48., 40.], size: [72., 48.], rotation: 0.2 }).unwrap();
    rounded.name = Some("Badge".into());
    records.push(rounded);
    records.push(path(2, vec![open(&[[0., 0.]])], vec![fill([0.3, 0.6, 0.9, 0.7])])
        .with_shape(LiveShape { kind: ShapeKind::Ellipse, center: [136., 40.], size: [64., 40.], rotation: -0.4 }).unwrap());
    let mut even_odd = path(3, vec![closed(&[[176., 12.], [244., 12.], [244., 72.], [176., 72.]]), closed(&[[192., 28.], [228., 28.], [228., 56.], [192., 56.]])], vec![fill([0.5, 0.2, 0.6, 1.])]);
    even_odd.fill_rule = FillRule::EvenOdd;
    records.push(even_odd);
    records.push(path(4, vec![closed(&[[12., 92.], [60., 92.], [60., 140.], [12., 140.]]), closed(&[[24., 104.], [48., 104.], [48., 128.], [24., 128.]])], vec![fill([0.2, 0.5, 0.3, 1.])]));
    records.push(path(5, vec![open(&[[76., 96.], [108., 132.], [140., 96.]])], vec![stroke(ink, 8., Cap::Butt, Join::Miter)]));
    records.push(path(6, vec![open(&[[156., 96.], [188., 132.], [220., 96.]])], vec![stroke(ink, 8., Cap::Square, Join::Bevel)]));
    let mut symmetric = path(7, vec![curve(&[[20., 160.], [70., 176.], [120., 156.]], [16., 6.])], vec![stroke(ink, 10., Cap::Round, Join::Round)]);
    if let PaintEntry::Stroke(stroke) = &mut Arc::make_mut(&mut symmetric.style).entries[0] { stroke.profile = profile(&[(0, 0., 0.1, 0.1), (0, 0.6, 1., 1.), (1, 1., 0.2, 0.2)]); }
    records.push(symmetric);
    let mut sided = path(8, vec![curve(&[[136., 160.], [186., 180.], [236., 160.]], [14., -4.])], vec![stroke(ink, 12., Cap::Round, Join::Round)]);
    if let PaintEntry::Stroke(stroke) = &mut Arc::make_mut(&mut sided.style).entries[0] { stroke.profile = profile(&[(0, 0., 0., 1.), (1, 1., 1., 0.2)]); }
    records.push(sided);
    records.push(path(9, vec![open(&[[244., 150.]])], vec![stroke(ink, 9., Cap::Round, Join::Round)]));
    let mut hidden = path(10, vec![closed(&[[0., 0.], [256., 0.], [256., 192.]])], vec![fill([1., 0., 0., 1.])]);
    hidden.hidden = true;
    records.push(hidden);
    let mut locked = path(11, vec![open(&[[8., 184.], [248., 184.]])], vec![stroke([0.6, 0.6, 0.6, 1.], 2., Cap::Butt, Join::Miter)]);
    locked.locked = true;
    locked.name = Some("Baseline".into());
    records.push(locked);
    records.into_iter().map(Record::Path).collect()
}
fn hatching() -> Vec<Record> {
    (0..65u64).map(|i| {
        let x = 8. + (i % 16) as f64 * 15.;
        let y = 8. + (i / 16) as f64 * 36.;
        Record::Path(path(100 + i, vec![open(&[[x, y], [x + 10., y + 28.]])], vec![stroke([0.1, 0.1, 0.1, 0.6], 2. + (i % 3) as f64, Cap::Round, Join::Round)]))
    }).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("OUTPUT.capy is required")?;
    let mut editor = Editor::new(layer_core::Document::from_artwork(Artwork::new([256, 192])?)?);
    for (name, records) in [("Empty", Vec::new()), ("Hatching", hatching()), ("Shapes", shapes())] {
        let (_, edit) = editor.document().create_vector_layer_edit(name, VectorObject::from_records(records), None, 0)?;
        editor.perform(edit)?;
    }
    let capture = editor.capture(0, EvaluationContext::default())?;
    let cancelled = AtomicBool::new(false);
    let prepared = layer_core::package::codec::PreparedPackage::prepare(&capture, None, &cancelled)?;
    let mut writer = BufWriter::new(File::create(output)?);
    prepared.write(&mut writer, &cancelled)?;
    writer.flush()?;
    Ok(())
}
