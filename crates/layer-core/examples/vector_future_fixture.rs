use layer_core::{Editor, EvaluationContext, ImageObject, authored::{Artwork, OpaqueResource, PortableId, PreservedVector, Resource, vector::*},
    color::{RgbColor, SampleDepth, source::{SourceBuilder, SourceChannels, SourceInterpretation}}, package::{ImmutableBacking, vector_records::{ValueClass, encode_chunk, tag_word}}};
use std::{fs::File, io::{BufWriter, Write}, sync::{Arc, atomic::AtomicBool}};

fn line(id: u64, points: &[[f64; 2]], property: UnknownProperty) -> PathItem {
    let anchors: Vec<Anchor> = points.iter().map(|p| Anchor::corner(*p)).collect();
    let mut path = PathItem::new(ItemId(id), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, false)]).unwrap()),
        Arc::new(Style::new(vec![PaintEntry::Stroke(Stroke::new(Paint::color(RgbColor::BLACK), 3.))])));
    path.unknown = Some(Arc::from([property]));
    path
}

fn image(extent: [u32; 2], pixel: impl Fn(u32, u32) -> [u8; 4]) -> Result<layer_core::Image, String> {
    let interpretation = SourceInterpretation { channels: SourceChannels::Rgba, depth: SampleDepth::U8, profile: Default::default(), profile_assumed: false };
    let mut builder = SourceBuilder::new(extent, interpretation, 1 << 20)?;
    for y in 0..extent[1] { builder.push_row(&(0..extent[0]).flat_map(|x| pixel(x, y)).collect::<Vec<_>>())?; }
    Ok(Arc::new(builder.finish()?).into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("OUTPUT.capy is required")?;
    let texture = image([16, 12], |x, y| [x as u8 * 16, y as u8 * 20, 200, 255])?;
    let fallback = image([64, 48], |x, y| [40, x as u8 * 4, y as u8 * 5, 255])?;
    let font: Arc<[u8]> = b"capy future font program".as_slice().into();
    let resource = Arc::new(OpaqueResource { id: PortableId::random(), kind: "example.font/1".into(), data: serde_json::json!({"family": "Future"}),
        encoding: "raw".into(), extra_fields: Default::default(), backing: ImmutableBacking::new(Arc::new(font.clone()))?,
        offset: 0, length: font.len() as u64, crc32: crc32fast::hash(&font) });
    let readable = line(1, &[[8., 8.], [120., 90.]], UnknownProperty { tag: tag_word(1000, IGNORABLE | COPY_SAFE, ValueClass::Slots), value: Arc::from([0u8, 1].as_slice()) });
    let current = VectorObject::with_slots(VectorObject::from_records(vec![Record::Path(readable)]).chunks().to_vec(),
        Arc::from([Slot::Image(texture.clone()), Slot::Resource(resource.clone())]));
    let required = line(2, &[[20., 100.], [200., 150.]], UnknownProperty { tag: tag_word(1001, 0, ValueClass::Slots), value: Arc::from([1u8].as_slice()) });
    let items: Arc<[u8]> = encode_chunk(&VectorChunk::new(vec![Record::Path(required)])).into();
    let mut newer = ImageObject::new(fallback);
    newer.affine = layer_core::authored::Affine64([2., 0., 0., 2., 30., 20.]);
    newer.vector = Some(PreservedVector(Arc::from([(serde_json::json!({"records": 1}), Resource::new(items))]),
        Arc::from([Slot::Resource(resource), Slot::Image(texture)])));
    let mut editor = Editor::new(layer_core::Document::from_artwork(Artwork::new([256, 192])?)?);
    let (_, edit) = editor.document().create_object_layer_edit("Newer", newer, None, 0)?;
    editor.perform(edit)?;
    let (_, edit) = editor.document().create_vector_layer_edit("Slots", current, None, 0)?;
    editor.perform(edit)?;
    let capture = editor.capture(0, EvaluationContext::default())?;
    let cancelled = AtomicBool::new(false);
    let prepared = layer_core::package::codec::PreparedPackage::prepare(&capture, None, &cancelled)?;
    let mut writer = BufWriter::new(File::create(output)?);
    prepared.write(&mut writer, &cancelled)?;
    writer.flush()?;
    Ok(())
}
