use super::values::DecodeError;
use super::vector_records::*;
use crate::authored::vector::*;
use crate::color::RgbColor;
use std::{collections::BTreeSet, sync::Arc};
use crate::authored::Affine64;

fn color(r: f32) -> Paint { Paint::color(RgbColor::new(Default::default(), [r, 0.25, 0.5, 1.]).unwrap()) }
fn line(id: u64, points: &[[f64; 2]], width: f64) -> Record {
    let anchors: Vec<_> = points.iter().map(|p| Anchor::new(*p, [-1., 0.5], [1., -0.5], AnchorKind::SMOOTH)).collect();
    let geometry = PathGeometry::new(vec![Subpath::new(&anchors, false)]).unwrap();
    let mut stroke = Stroke::new(color(0.1), width);
    stroke.cap = Cap::Round;
    stroke.join = Join::Round;
    stroke.profile = Some(Arc::new(WidthProfile::new(vec![
        WidthKnot::new(0, 0., 0., 0.),
        WidthKnot::new(0, 0.5, 1., 0.5),
        WidthKnot::new(points.len().saturating_sub(2) as u32, 1., 0.2, 0.2),
    ]).unwrap()));
    Record::Path(PathItem::new(ItemId(id), Arc::new(geometry), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)]))))
}
fn shape(id: u64) -> Record {
    let square = [[0., 0.], [10., 0.], [10., 10.], [0., 10.]].map(Anchor::corner);
    let hole = [[3., 3.], [3., 7.], [7., 7.], [7., 3.]].map(Anchor::corner);
    let geometry = PathGeometry::new(vec![Subpath::new(&square, true), Subpath::new(&hole, true)]).unwrap();
    let mut stroke = Stroke::new(color(0.9), 2.);
    stroke.miter_limit = 10.;
    let mut path = PathItem::new(ItemId(id), Arc::new(geometry), Arc::new(Style::new(vec![PaintEntry::Fill(Fill { paint: color(0.4), unknown: None }), PaintEntry::Stroke(stroke)])));
    path.fill_rule = FillRule::EvenOdd;
    path.name = Some("Badge".into());
    path.hidden = true;
    path.locked = true;
    Record::Path(path)
}
fn chunk(records: Vec<Record>) -> VectorChunk { VectorChunk::new(records) }
fn round_trip(records: Vec<Record>) -> (Vec<u8>, VectorChunk) {
    let original = chunk(records);
    let bytes = encode_chunk(&original);
    let decoded = decode_chunk(&bytes, original.records().len(), &mut 0).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(encode_chunk(&decoded), bytes);
    (bytes, decoded)
}
fn record_bytes(kind: u64, id: u64, properties: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let mut props = Vec::new();
    for (tag, value) in properties { put_varuint(&mut props, *tag); put_varuint(&mut props, value.len() as u64); props.extend_from_slice(value); }
    let mut out = Vec::new();
    put_varuint(&mut out, kind);
    if kind != 0 { out.extend_from_slice(&id.to_le_bytes()); put_varuint(&mut out, props.len() as u64); out.extend(props); }
    out
}
fn geometry_bytes() -> Vec<u8> {
    let Record::Path(path) = line(1, &[[0., 0.], [5., 5.]], 3.) else { unreachable!() };
    encode_geometry(&path.geometry)
}
fn is_unsupported(result: Result<VectorChunk, DecodeError>) -> bool { matches!(result, Err(DecodeError::Unsupported(_))) }
fn is_invalid(result: Result<VectorChunk, DecodeError>) -> bool { matches!(result, Err(DecodeError::Invalid(_))) }

#[test]
fn every_v0_property_round_trips_canonically() {
    let dot = Record::Path(PathItem::new(ItemId(3), Arc::new(PathGeometry::new(vec![Subpath::new(&[Anchor::corner([4., 4.])], false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(Stroke { cap: Cap::Square, join: Join::Bevel, ..Stroke::new(color(0.), 6.) })]))));
    let empty = Record::Path(PathItem::new(ItemId(4), Arc::new(PathGeometry::new(vec![Subpath::new(&[Anchor::corner([1., 2.]), Anchor::corner([3., 4.])], false)]).unwrap()), Arc::default()));
    let linear = Record::Path(PathItem::new(ItemId(5), Arc::new(PathGeometry::new(vec![Subpath::new(&[Anchor::corner([1., 2.])], false)]).unwrap()),
        Arc::new(Style::new(vec![PaintEntry::Fill(Fill { paint: Paint::color(RgbColor::from_linear(Default::default(), [0.123_456_7, 0.5, 0.25, 0.75]).unwrap()), unknown: None })]))));
    round_trip(vec![line(1, &[[0., 0.], [10., 4.], [20., 0.]], 4.), shape(2), dot, empty, linear]);
}

#[test]
fn defaults_are_omitted_and_framing_is_minimal() {
    let (bytes, _) = round_trip(vec![Record::Path(PathItem::new(ItemId(1), Arc::new(PathGeometry::new(vec![Subpath::new(&[Anchor::corner([0., 0.])], false)]).unwrap()), Arc::default()))]);
    assert_eq!(bytes.len(), 1 + 8 + 1 + 2 + 3 + 25);
}

#[test]
fn long_paths_keep_runs_of_256_and_edit_one_run() {
    let anchors: Vec<_> = (0..1000).map(|i| Anchor::corner([f64::from(i), 0.])).collect();
    let subpath = Subpath::new(&anchors, false);
    assert_eq!(subpath.runs().iter().map(|r| r.len()).collect::<Vec<_>>(), vec![256, 256, 256, 232]);
    let edited = subpath.with_anchor(600, Anchor::corner([-1., -1.]));
    for (index, (a, b)) in subpath.runs().iter().zip(edited.runs()).enumerate() { assert_eq!(Arc::ptr_eq(a, b), index != 2); }
    assert_eq!(edited.anchor(600).point(), [-1., -1.]);
    assert_eq!(edited.anchors().rev().nth(999).unwrap().point(), [0., 0.]);
    round_trip(vec![Record::Path(PathItem::new(ItemId(1), Arc::new(PathGeometry::new(vec![edited]).unwrap()), Arc::default()))]);
}

#[test]
fn unknown_ignorable_data_survives_and_required_data_is_unsupported() {
    let geometry = geometry_bytes();
    let keep = tag_word(900, IGNORABLE | COPY_SAFE, ValueClass::Data);
    let volatile = tag_word(901, IGNORABLE, ValueClass::Nested);
    let nested = record_bytes(kind_word(77) | IGNORABLE, 0, &[(tag_word(3, IGNORABLE, ValueClass::Items), 42u64.to_le_bytes().to_vec())]);
    let bytes = record_bytes(PATH, 8, &[(GEOMETRY, geometry.clone()), (keep, vec![1, 2, 3]), (volatile, nested.clone())]);
    let chunk = decode_chunk(&bytes, 1, &mut 0).unwrap();
    assert_eq!(encode_chunk(&chunk), bytes);
    let Record::Path(path) = &chunk.records()[0] else { panic!() };
    let moved = path.with_geometry(path.geometry.clone());
    assert_eq!(moved.unknown.as_ref().unwrap().iter().map(|p| p.tag).collect::<Vec<_>>(), vec![keep]);
    let renamed = PathItem { name: Some("Renamed".into()), ..path.clone() };
    assert_eq!(renamed.unknown, path.unknown);

    assert!(is_unsupported(decode_chunk(&record_bytes(PATH, 8, &[(GEOMETRY, geometry.clone()), (tag_word(902, 0, ValueClass::Data), vec![])]), 1, &mut 0)));
    assert!(is_unsupported(decode_chunk(&record_bytes(PATH, 8, &[(GEOMETRY, geometry.clone()), (tag_word(902, IGNORABLE | 16, ValueClass::Data), vec![])]), 1, &mut 0)));
    assert!(is_unsupported(decode_chunk(&record_bytes(PATH, 8, &[(GEOMETRY, geometry.clone()), (WIDTH, 1f64.to_le_bytes().to_vec())]), 1, &mut 0)));
    assert!(is_unsupported(decode_chunk(&record_bytes(kind_word(50), 3, &[]), 1, &mut 0)));
    assert!(is_unsupported(decode_chunk(&record_bytes(kind_word(50) | IGNORABLE | KIND_PARENT | 8, 3, &[]), 1, &mut 0)));
    assert!(is_unsupported(decode_chunk(&record_bytes(kind_word(50) | COPY_SAFE, 3, &[]), 1, &mut 0)));
    assert!(is_unsupported(decode_chunk(&record_bytes(PATH, 8, &[(GEOMETRY, geometry.clone()), (FILL_RULE, vec![7])]), 1, &mut 0)));
    assert!(is_invalid(decode_chunk(&record_bytes(PATH, 8, &[(GEOMETRY, geometry), (FILL_RULE, vec![0, 0])]), 1, &mut 0)));
}

#[test]
fn unknown_parents_are_atomic_and_may_span_chunks() {
    let parent = kind_word(60) | IGNORABLE | COPY_SAFE | KIND_PARENT;
    let mut first = record_bytes(parent, 11, &[]);
    first.extend(encode_chunk(&chunk(vec![line(12, &[[0., 0.], [1., 1.]], 1.)])));
    let mut second = record_bytes(0, 0, &[]);
    second.extend(encode_chunk(&chunk(vec![line(13, &[[2., 2.], [3., 3.]], 1.)])));
    let chunks = decode_object(&[(&first, 2), (&second, 2)]).unwrap();
    assert!(matches!(chunks[0].records()[0], Record::Opaque(_)));
    assert!(matches!(chunks[0].records()[1], Record::Opaque(_)), "children of an unknown parent stay uninterpreted");
    assert!(matches!(chunks[1].records()[0], Record::End));
    assert!(matches!(chunks[1].records()[1], Record::Path(_)));
    assert_eq!(encode_chunk(&chunks[0]), first);
    assert_eq!(encode_chunk(&chunks[1]), second);
    assert!(matches!(decode_object(&[(&first, 2)]), Err(DecodeError::Invalid(_))));
    assert!(matches!(decode_object(&[(&second, 2)]), Err(DecodeError::Invalid(_))));
}

#[test]
fn malformed_framing_is_invalid() {
    let good = encode_chunk(&chunk(vec![line(1, &[[0., 0.], [3., 1.]], 2.)]));
    for cut in 1..good.len() { assert!(decode_chunk(&good[..cut], 1, &mut 0).is_err(), "truncated at {cut}"); }
    assert!(is_invalid(decode_chunk(&good, 2, &mut 0)));
    let mut padded = good.clone();
    padded.push(0);
    assert!(is_invalid(decode_chunk(&padded, 1, &mut 0)));
    let mut overlong = vec![PATH as u8 | 0x80, 0];
    overlong.extend_from_slice(&good[1..]);
    assert!(is_invalid(decode_chunk(&overlong, 1, &mut 0)));
    let geometry = geometry_bytes();
    assert!(is_invalid(decode_chunk(&record_bytes(PATH, 8, &[(FILL_RULE, vec![1]), (GEOMETRY, geometry.clone())]), 1, &mut 0)));
    assert!(is_invalid(decode_chunk(&record_bytes(PATH, 8, &[(GEOMETRY, geometry.clone()), (GEOMETRY, geometry.clone())]), 1, &mut 0)));
    assert!(is_invalid(decode_chunk(&record_bytes(PATH, 0, &[(GEOMETRY, geometry.clone())]), 1, &mut 0)));
    assert!(is_invalid(decode_chunk(&record_bytes(PATH, 8, &[]), 1, &mut 0)));
    assert!(is_invalid(decode_chunk(&record_bytes(PATH | IGNORABLE, 8, &[(GEOMETRY, geometry.clone())]), 1, &mut 0)));
    let mut nan = geometry.clone();
    let at = nan.len() - 4;
    nan[at..].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(is_invalid(decode_chunk(&record_bytes(PATH, 8, &[(GEOMETRY, nan)]), 1, &mut 0)));
    let two = [encode_chunk(&chunk(vec![line(1, &[[0., 0.], [1., 1.]], 1.)])), encode_chunk(&chunk(vec![line(1, &[[0., 0.], [1., 1.]], 1.)]))];
    assert!(matches!(decode_object(&[(&two[0], 1), (&two[1], 1)]), Err(DecodeError::Invalid(_))));
}

#[test]
fn seeded_byte_mutations_never_panic_and_decode_to_stable_records() {
    let records = vec![line(1, &[[0., 0.], [3., 1.], [9., -4.]], 2.), shape(2), Record::Path(live(3, ShapeKind::Rectangle { radii: [4.; 4] })), Record::Path(live(4, ShapeKind::Ellipse))];
    let good = encode_chunk(&chunk(records.clone()));
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
    let mut decoded = 0;
    for _ in 0..20_000 {
        let mut bytes = good.clone();
        for _ in 0..1 + next() % 4 {
            let at = (next() as usize) % bytes.len().max(1);
            match next() % 5 {
                0 => bytes[at] ^= 1 << (next() % 8),
                1 => bytes[at] = 0xff,
                2 => { let end = (at + 1 + (next() as usize) % 12).min(bytes.len()); bytes.drain(at..end); }
                3 => { let end = (at + 1 + (next() as usize) % 12).min(bytes.len()); let copy = bytes[at..end].to_vec(); bytes.splice(at..at, copy); }
                _ => bytes.splice(at..at, [0xff, 0xff, 0xff, 0xff, 0x0f]).for_each(drop),
            }
            if bytes.is_empty() { break; }
        }
        let Ok(chunk) = decode_chunk(&bytes, records.len(), &mut 0) else { continue };
        decoded += 1;
        let again = encode_chunk(&chunk);
        assert_eq!(decode_chunk(&again, chunk.records().len(), &mut 0).as_ref(), Ok(&chunk));
        assert_eq!(encode_chunk(&decode_chunk(&again, chunk.records().len(), &mut 0).unwrap()), again);
    }
    assert!(decoded > 0, "some mutations land in values that stay valid");
}

#[test]
fn registry_numbers_are_unique_and_listed() {
    let listed: Vec<String> = include_str!("vector_numbers.tsv").lines().filter(|l| !l.starts_with('#')).map(str::to_owned).collect();
    let mut generated = Vec::new();
    for (space, table, shift) in [("kind", KINDS, 4), ("tag", TAGS, 5)] {
        for (index, number) in table.iter().enumerate() {
            assert_eq!(number.word >> shift, index as u64 + 1, "{space} numbers are dense and ordered");
            generated.push(format!("{space}\t{}\t{}\t{}\t{}", number.word >> shift, number.word & ((1 << shift) - 1), number.name, number.since));
        }
    }
    assert_eq!(generated, listed, "update vector_numbers.tsv by appending; never renumber");
}

#[test]
fn seeded_unknown_properties_survive_every_position() {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
    for _ in 0..200 {
        let number = 1000 + next() % 1000;
        let flags = next() % 4;
        let class = [ValueClass::Data, ValueClass::Items, ValueClass::Nested][(next() % 3) as usize];
        let value = match class {
            ValueClass::Items => (0..next() % 3).flat_map(|_| next().to_le_bytes()).collect(),
            ValueClass::Nested => record_bytes(kind_word(500 + next() % 50) | IGNORABLE, next(), &[]),
            _ => (0..next() % 9).map(|_| next() as u8).collect(),
        };
        let tag = tag_word(number, flags, class);
        let bytes = record_bytes(PATH, 1 + next() % 1000, &[(GEOMETRY, geometry_bytes()), (tag, value)]);
        let decoded = decode_chunk(&bytes, 1, &mut 0);
        if flags & IGNORABLE == 0 { assert!(is_unsupported(decoded)); continue; }
        let chunk = decoded.unwrap();
        assert_eq!(encode_chunk(&chunk), bytes);
        let Record::Path(path) = &chunk.records()[0] else { panic!() };
        let kept = path.with_style(path.style.clone()).unknown.is_some();
        assert_eq!(kept, flags & COPY_SAFE != 0);
    }
}

#[test]
fn chunk_edits_rewrite_only_touched_chunks() {
    let object = VectorObject::from_records((1..=200).map(|i| line(i, &[[0., 0.], [i as f64, 1.]], 1.)).collect());
    assert!(object.chunks().iter().all(|c| c.records().len() <= CHUNK_RECORDS));
    assert_eq!(object.path_count(), 200);
    let target = object.find(ItemId(150)).unwrap();
    let edited = object.replace([(target, Some(line(150, &[[5., 5.], [6., 6.]], 3.)))]);
    for (index, (a, b)) in object.chunks().iter().zip(edited.chunks()).enumerate() { assert_eq!(a.same_owner(b), index != target.chunk as usize); }
    let removed = object.replace([(target, None)]);
    assert_eq!(removed.path_count(), 199);
    assert!(removed.find(ItemId(150)).is_none());
    let mut drawn = VectorObject::default();
    for i in 0..300 { drawn = drawn.insert_front(vec![line(1000 + i, &[[0., 0.], [1., 1.]], 1.)]); }
    assert_eq!(drawn.paths().next().unwrap().1.id, ItemId(1299));
    assert!(drawn.chunks().iter().all(|c| (1..=CHUNK_RECORDS).contains(&c.records().len())));
    let before = drawn.clone();
    let after = drawn.insert_front(vec![line(5000, &[[0., 0.], [1., 1.]], 1.)]);
    assert!(before.chunks()[1..].iter().zip(&after.chunks()[after.chunks().len() - (before.chunks().len() - 1)..]).all(|(a, b)| a.same_owner(b)));
}

fn document_with_paths(count: u64) -> (crate::Editor, crate::OccurrenceHandle) {
    let artwork = crate::Artwork::new([640, 480]).unwrap();
    let mut editor = crate::Editor::new(crate::Document::from_artwork(artwork).unwrap());
    let records = (1..=count).map(|i| if i % 7 == 0 { shape(i) } else { line(i, &[[i as f64, 3.], [i as f64 + 20., 40.], [i as f64 + 5., 90.]], 2. + (i % 5) as f64) }).collect();
    let (layer, edit) = editor.document().create_vector_layer_edit("Ink", VectorObject::from_records(records), None, 0).unwrap();
    editor.perform(edit).unwrap();
    (editor, layer)
}
fn save(editor: &crate::Editor) -> Vec<u8> {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let capture = editor.capture(1, Default::default()).unwrap();
    let mut bytes = Vec::new();
    super::codec::PreparedPackage::prepare(&capture, None, &cancel).unwrap().write(&mut bytes, &cancel).unwrap();
    bytes
}
fn open(bytes: Vec<u8>) -> crate::Artwork {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let backing = super::ImmutableBacking::new(Arc::new(Arc::<[u8]>::from(bytes))).unwrap();
    match super::codec::open(backing, Default::default(), &cancel).unwrap() {
        super::codec::OpenOutcome::Candidate { artwork, .. } => artwork,
        outcome => panic!("{outcome:?}"),
    }
}
fn vector(artwork: &crate::Artwork) -> &VectorObject { artwork.vectors.iter().next().unwrap().2 }

#[test]
fn vector_edits_reject_identities_and_markers_that_break_the_object() {
    let (mut editor, layer) = document_with_paths(300);
    let object = editor.document().scene().vector_layer(layer).unwrap().clone();
    assert!(object.chunks().len() > 3);
    let far = object.find(ItemId(290)).unwrap();
    assert!(far.chunk > 0);
    let rejected = |editor: &mut crate::Editor, next: VectorObject| {
        let edit = editor.document().vector_object_edit(layer, next).unwrap();
        assert!(editor.perform(edit).is_err());
    };
    rejected(&mut editor, object.insert_front(vec![line(290, &[[0., 0.], [1., 1.]], 1.)]));
    rejected(&mut editor, object.replace([(object.find(ItemId(2)).unwrap(), Some(line(290, &[[0., 0.], [1., 1.]], 1.)))]));
    rejected(&mut editor, object.insert_front(vec![Record::End]));
    let id = object.allocate_id();
    assert!(!object.contains(id) && object.contains(ItemId(290)));
    let drawn = object.insert_front(vec![line(id.0, &[[0., 0.], [1., 1.]], 1.)]);
    editor.perform(editor.document().vector_object_edit(layer, drawn).unwrap()).unwrap();
    assert_eq!(editor.document().scene().vector_layer(layer).unwrap().find(id), Some(Position { chunk: 0, index: 0 }));
}

#[test]
fn vector_layers_round_trip_through_packages_and_keep_unchanged_chunks() {
    let (mut editor, layer) = document_with_paths(300);
    let original = editor.document().artwork.clone();
    let reopened = open(save(&editor));
    assert_eq!(vector(&reopened), vector(&original));
    let occurrence = reopened.occurrences.iter().find(|(_, _, o)| matches!(o.content, crate::OccurrenceContent::Vector(_))).unwrap().2;
    assert_eq!(occurrence.name.as_ref(), "Ink");
    let ids: Vec<_> = vector(&reopened).chunks().iter().map(|c| c.id()).collect();
    assert_eq!(ids, vector(&original).chunks().iter().map(|c| c.id()).collect::<Vec<_>>());

    let mut reloaded = crate::Editor::new(crate::Document::from_artwork(reopened).unwrap());
    let reloaded_layer = reloaded.document().scene().order().iter().copied().find(|h| reloaded.document().scene().vector_handle(*h).is_some()).unwrap();
    let object = reloaded.document().scene().vector_layer(reloaded_layer).unwrap().clone();
    let target = object.find(ItemId(150)).unwrap();
    let edited = object.replace([(target, Some(line(150, &[[1., 1.], [2., 2.]], 9.)))]);
    reloaded.perform(reloaded.document().vector_object_edit(reloaded_layer, edited).unwrap()).unwrap();
    let saved = open(save(&reloaded));
    let before = vector(&original);
    for (index, chunk) in vector(&saved).chunks().iter().enumerate() {
        assert_eq!(chunk.id() == before.chunks()[index].id(), index != target.chunk as usize);
    }
    assert!(reloaded.undo().unwrap());
    assert_eq!(reloaded.document().scene().vector_layer(reloaded_layer).unwrap(), before);

    editor.perform(editor.document().vector_object_edit(layer, VectorObject::default()).unwrap()).unwrap();
    assert_eq!(vector(&open(save(&editor))).record_count(), 0);
}

#[test]
fn vector_history_survives_sessions_and_worker_transfer() {
    use super::{session::PreparedSession, session_transfer::{PreparedSessionTransfer, SessionTransferReceiver}};
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let limits = crate::ProjectLimits::default();
    let (mut editor, layer) = document_with_paths(130);
    let first = editor.document().artwork.clone();
    let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![line(9000, &[[0., 0.], [50., 50.]], 4.)]);
    editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
    let drawn = editor.document().artwork.clone();
    editor.undo().unwrap();
    let capture = editor.capture_session(editor.capture(1, Default::default()).unwrap()).unwrap();
    let session = PreparedSession::prepare(&capture, serde_json::json!({}), &cancel).unwrap();
    let mut pack = Vec::new();
    std::io::Read::read_to_end(&mut session.resources().reader(&cancel), &mut pack).unwrap();
    let mut loaded = super::session::open_parts(session.metadata(), super::ImmutableBacking::new(Arc::new(Arc::<[u8]>::from(pack))).unwrap(), limits, &cancel).unwrap();
    assert_eq!(vector(&loaded.editor.document().artwork), vector(&first));
    loaded.editor.redo().unwrap();
    assert_eq!(vector(&loaded.editor.document().artwork), vector(&drawn));

    let transfer = PreparedSessionTransfer::capture(&capture, serde_json::json!({}), &cancel).unwrap();
    let descriptor = serde_json::from_slice(&serde_json::to_vec(transfer.descriptor()).unwrap()).unwrap();
    let mut receiver = SessionTransferReceiver::new(descriptor, limits).unwrap();
    for index in receiver.missing_payloads() {
        let length = transfer.payload_len(index).unwrap();
        let mut offset = 0;
        while offset < length {
            let count = (length - offset).min(super::MAX_RANGE_BYTES as u64) as usize;
            receiver.push_chunk(index, &transfer.read_chunk(index, offset, count).unwrap()).unwrap();
            offset += count as u64;
        }
    }
    let mut adopted = receiver.finish().unwrap().adopt_verified(limits, &cancel).unwrap();
    assert_eq!(vector(&adopted.editor.document().artwork), vector(&first));
    adopted.editor.redo().unwrap();
    assert_eq!(vector(&adopted.editor.document().artwork), vector(&drawn));
}

#[test]
fn session_history_shares_unchanged_runs_of_vector_chunk_lists() {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let (mut editor, layer) = document_with_paths(128 * CHUNK_RECORDS as u64);
    let first = editor.document().artwork.clone();
    let strokes = 40;
    for i in 0..strokes {
        let object = editor.document().scene().vector_layer(layer).unwrap().insert_front(vec![line(100_000 + i, &[[0., 0.], [50., i as f64]], 4.)]);
        editor.perform(editor.document().vector_object_edit(layer, object).unwrap()).unwrap();
    }
    let chunks = vector(&editor.document().artwork).chunks().len();
    let capture = editor.capture_session(editor.capture(1, Default::default()).unwrap()).unwrap();
    let session = super::session::PreparedSession::prepare(&capture, serde_json::json!({}), &cancel).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(session.metadata()).unwrap();
    let listed: usize = metadata["index_chunks"].as_array().unwrap().iter().map(|chunk| chunk.as_array().unwrap().len()).sum();
    assert!(listed < (strokes as usize + 1) * chunks / 4, "{listed} chunk references for {chunks} chunks");
    let mut pack = Vec::new();
    std::io::Read::read_to_end(&mut session.resources().reader(&cancel), &mut pack).unwrap();
    let mut loaded = super::session::open_parts(session.metadata(), super::ImmutableBacking::new(Arc::new(Arc::<[u8]>::from(pack))).unwrap(), crate::ProjectLimits::default(), &cancel).unwrap();
    assert_eq!(vector(&loaded.editor.document().artwork), vector(&editor.document().artwork));
    for _ in 0..strokes { loaded.editor.undo().unwrap(); }
    assert_eq!(vector(&loaded.editor.document().artwork), vector(&first));
}

fn plain_line(id: u64, from: [f64; 2], to: [f64; 2], width: f64) -> PathItem {
    let geometry = PathGeometry::new(vec![Subpath::new(&[Anchor::corner(from), Anchor::corner(to)], false)]).unwrap();
    PathItem::new(ItemId(id), Arc::new(geometry), Arc::new(Style::new(vec![PaintEntry::Stroke(Stroke::new(color(0.1), width))])))
}

#[test]
fn hits_find_the_frontmost_visible_unlocked_path_under_the_point() {
    let Record::Path(mut badge) = shape(3) else { unreachable!() };
    (badge.hidden, badge.locked) = (false, false);
    let mut locked = plain_line(4, [0., 50.], [100., 50.], 6.);
    locked.locked = true;
    let object = VectorObject::from_records(vec![Record::Path(plain_line(1, [0., 0.], [100., 0.], 4.)), Record::Path(plain_line(2, [50., -20.], [50., 20.], 2.)), Record::Path(badge), Record::Path(locked)]);
    let mut index = VectorIndex::default();
    assert_eq!(index.hit(&object, [50., 0.], 0.), Some(ItemId(1)), "the front path wins where paths overlap");
    assert_eq!(index.hit(&object, [50., 10.], 0.), Some(ItemId(2)));
    assert_eq!(index.hit(&object, [20., 1.9], 0.), Some(ItemId(1)));
    assert_eq!(index.hit(&object, [20., 2.5], 0.), None);
    assert_eq!(index.hit(&object, [20., 2.5], 1.), Some(ItemId(1)));
    assert_eq!(index.hit(&object, [1.5, 5.], 0.), Some(ItemId(3)), "filled area");
    assert_eq!(index.hit(&object, [5., 5.], 0.), None, "even-odd hole");
    assert_eq!(index.hit(&object, [20., 50.], 0.), None, "locked paths are not picked");
}

#[test]
fn transforming_items_rewrites_only_their_chunks_and_scales_stroke_widths() {
    let object = VectorObject::from_records((1..=300).map(|i| Record::Path(plain_line(i, [i as f64, 0.], [i as f64, 10.], 2.))).collect());
    let few: BTreeSet<ItemId> = [ItemId(5), ItemId(250)].into();
    let many: BTreeSet<ItemId> = (1..=300).filter(|i| i % 3 == 0).map(ItemId).collect();
    for ids in [&few, &many] {
        let expected: BTreeSet<_> = ids.iter().map(|id| object.find(*id).unwrap()).collect();
        let found: BTreeSet<_> = object.positions(ids).into_iter().flat_map(|(c, v)| v.into_iter().map(move |i| Position { chunk: c as u32, index: i as u32 })).collect();
        assert_eq!(found, expected);
    }
    let moved = object.transform_items(&few, Affine64([2., 0., 0., 2., 1000., 0.])).unwrap();
    moved.validate().unwrap();
    let touched: BTreeSet<_> = few.iter().map(|id| object.find(*id).unwrap().chunk as usize).collect();
    for (index, (a, b)) in object.chunks().iter().zip(moved.chunks()).enumerate() { assert_eq!(a.same_owner(b), !touched.contains(&index)); }
    let path = moved.record(moved.find(ItemId(5)).unwrap()).unwrap().path().unwrap();
    assert_eq!(path.geometry.subpaths()[0].anchor(1).point(), [1010., 20.]);
    assert_eq!(path.style.strokes().next().unwrap().width, 4.);
    assert_eq!(moved.items_bounds(&few), Some([1002., -8., 1508., 28.]));
}

#[test]
fn path_selections_keep_only_paths_that_still_exist() {
    let (mut editor, layer) = document_with_paths(200);
    let mut working = editor.document().working.clone();
    working.vector_items = Some(crate::VectorItemSelection { layer, items: Arc::new([ItemId(3), ItemId(150), ItemId(9999)].into()) });
    editor.perform(crate::Edit::Working(working)).unwrap();
    let selected = |editor: &crate::Editor| editor.document().working.vector_items.as_ref().map(|s| s.items.iter().copied().collect::<Vec<_>>());
    assert_eq!(selected(&editor), Some(vec![ItemId(3), ItemId(150)]), "missing paths drop out of the selection");
    let object = editor.document().scene().vector_layer(layer).unwrap().clone();
    let removed = object.replace([(object.find(ItemId(150)).unwrap(), None)]);
    editor.perform(editor.document().vector_object_edit(layer, removed).unwrap()).unwrap();
    assert_eq!(selected(&editor), Some(vec![ItemId(3)]));
    let object = editor.document().scene().vector_layer(layer).unwrap().clone();
    let removed = object.replace([(object.find(ItemId(3)).unwrap(), None)]);
    editor.perform(editor.document().vector_object_edit(layer, removed).unwrap()).unwrap();
    assert_eq!(selected(&editor), None, "an empty path selection clears");
}

fn live(id: u64, kind: ShapeKind) -> PathItem { rotated(id, kind, 0.3) }
fn rotated(id: u64, kind: ShapeKind, rotation: f64) -> PathItem {
    let shape = LiveShape { kind, center: [120., 80.], size: [90., 40.], rotation };
    let base = PathItem::new(ItemId(id), Arc::new(PathGeometry::new(vec![Subpath::new(&[Anchor::corner([0.; 2])], false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Fill(Fill { paint: color(0.4), unknown: None })])));
    base.with_shape(shape).unwrap()
}
fn near(a: &PathGeometry, b: &PathGeometry) -> bool {
    a.anchor_count() == b.anchor_count() && a.subpaths().iter().zip(b.subpaths()).all(|(a, b)| a.anchors().zip(b.anchors()).all(|(a, b)|
        a.bits().iter().zip(b.bits()).all(|(x, y)| (x - y).abs() < 2e-3)))
}

#[test]
fn live_shapes_round_trip_and_detach_when_a_foreign_writer_changed_their_geometry() {
    let shapes = vec![Record::Path(live(1, ShapeKind::Rectangle { radii: [0., 6., 12., 3.] })), Record::Path(live(2, ShapeKind::Ellipse))];
    let (bytes, decoded) = round_trip(shapes.clone());
    for (record, original) in decoded.records().iter().zip(&shapes) {
        let (path, original) = (record.path().unwrap(), original.path().unwrap());
        assert_eq!(path.shape, original.shape);
        assert!(near(&path.geometry, &path.shape.as_ref().unwrap().geometry().unwrap()));
    }
    assert!(!bytes.is_empty());
    let Record::Path(edited) = &shapes[0] else { unreachable!() };
    let recipe = encode_shape(edited.shape.as_ref().unwrap(), fingerprint(edited));
    let moved = PathGeometry::new(vec![Subpath::new(&[Anchor::corner([0., 0.]), Anchor::corner([5., 5.])], false)]).unwrap();
    let stale = record_bytes(PATH, 1, &[(GEOMETRY, encode_geometry(&moved)), (STYLE, encode_style(&edited.style)), (LIVE_SHAPE, recipe)]);
    let reopened = decode_chunk(&stale, 1, &mut 0).unwrap();
    let path = reopened.records()[0].path().unwrap();
    assert_eq!(path.shape, None, "a recipe whose fingerprint no longer matches detaches");
    assert_eq!(*path.geometry, moved, "and leaves the geometry unchanged");
}

#[test]
fn transforms_keep_live_shapes_they_can_express_and_drop_the_rest() {
    let rounded = live(1, ShapeKind::Rectangle { radii: [4., 0., 8., 2.] });
    for affine in [Affine64([1., 0., 0., 1., 30., -12.]), Affine64([0.8, 0.6, -0.6, 0.8, 5., 9.]), Affine64([-1.5, 0., 0., 1.5, 400., 0.])] {
        let moved = rounded.transformed(affine).unwrap();
        let shape = moved.shape.as_ref().expect("similarities keep the recipe");
        assert!(near(&normalized(&moved.geometry), &normalized(&shape.geometry().unwrap())), "{affine:?}");
    }
    let stretched = rounded.transformed(Affine64([2., 0., 0., 1., 0., 0.])).unwrap();
    assert_eq!(stretched.shape, None, "uneven scale cannot keep rounded corners");
    let plain = rotated(2, ShapeKind::Rectangle { radii: [0.; 4] }, 0.).transformed(Affine64([2., 0., 0., 1., 0., 0.])).unwrap();
    assert!(plain.shape.is_some(), "a square-cornered rectangle stretches along its own axes");
    assert!(live(3, ShapeKind::Ellipse).transformed(Affine64([2., 0., 0., 1., 0., 0.])).unwrap().shape.is_none(), "stretching across a rotated frame shears it");
    assert!(rounded.with_style(Arc::new(Style::default())).shape.is_some(), "restyling keeps the recipe");
    assert!(rounded.with_geometry(rounded.geometry.clone()).shape.is_none(), "editing geometry detaches it");
}
fn normalized(geometry: &PathGeometry) -> PathGeometry {
    let mut points: Vec<[f64; 2]> = geometry.subpaths()[0].anchors().map(|a| a.point()).collect();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    PathGeometry::new(vec![Subpath::new(&points.into_iter().map(Anchor::corner).collect::<Vec<_>>(), true)]).unwrap()
}

#[test]
fn vector_fixture_reopens_every_feature_and_saves_byte_for_byte() {
    let bytes = include_bytes!("codec/fixtures/vector-objects.capy").to_vec();
    let editor = crate::Editor::new(crate::Document::from_artwork(open(bytes.clone())).unwrap());
    let scene = editor.document().scene();
    let layer = |name: &str| scene.order().iter().find(|h| scene.occurrence(**h).unwrap().name.as_ref() == name).and_then(|h| scene.vector_layer(*h)).unwrap().clone();
    let shapes = layer("Shapes");
    let paths: Vec<PathItem> = shapes.paths().map(|(_, p)| p.clone()).collect();
    assert_eq!(paths.len(), 11);
    assert!(matches!(paths[0].shape.as_deref(), Some(LiveShape { kind: ShapeKind::Rectangle { .. }, .. })) && paths[0].name.as_deref() == Some("Badge"));
    assert!(matches!(paths[0].style.entries[..], [PaintEntry::Stroke(_), PaintEntry::Fill(_)]), "a stroke below a fill");
    assert!(matches!(paths[1].shape.as_deref(), Some(LiveShape { kind: ShapeKind::Ellipse, .. })));
    assert_eq!(paths[2].fill_rule, FillRule::EvenOdd);
    assert_eq!(paths[2].geometry.subpaths().len(), 2);
    let strokes: Vec<&Stroke> = paths.iter().flat_map(|p| p.style.strokes()).collect();
    for cap in [Cap::Butt, Cap::Round, Cap::Square] { assert!(strokes.iter().any(|s| s.cap == cap)); }
    for join in [Join::Miter, Join::Round, Join::Bevel] { assert!(strokes.iter().any(|s| s.join == join)); }
    assert!(strokes.iter().filter_map(|s| s.profile.as_ref()).any(|p| p.knots().iter().any(|k| k.left() != k.right())), "a one-sided width profile");
    assert!(paths.iter().any(|p| p.geometry.anchor_count() == 1), "a dot");
    assert!(paths.iter().any(|p| p.hidden) && paths.iter().any(|p| p.locked));
    let hatching = layer("Hatching");
    assert_eq!((hatching.path_count(), hatching.chunks().len()), (65, 2), "a full chunk and one more");
    assert_eq!(layer("Empty").path_count(), 0);
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let capture = editor.capture(0, Default::default()).unwrap();
    let mut saved = Vec::new();
    super::codec::PreparedPackage::prepare(&capture, None, &cancel).unwrap().write(&mut saved, &cancel).unwrap();
    assert!(saved == bytes, "a save without edits reproduces the fixture byte for byte");
}

#[test]
fn seeded_edit_scripts_keep_unknown_data_by_its_rules_and_never_reissue_ids() {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || { state ^= state << 13; state ^= state >> 7; state ^= state << 17; state };
    let unknown = |number: u64, flags: u64| UnknownProperty { tag: tag_word(number, flags, ValueClass::Data), value: Arc::from(&number.to_le_bytes()[..]) };
    let decorated = |id: u64| {
        let Record::Path(mut path) = line(id, &[[id as f64, 0.], [id as f64 + 9., 20.]], 2.) else { unreachable!() };
        path.unknown = Some(Arc::from(vec![unknown(900, IGNORABLE | COPY_SAFE), unknown(901, IGNORABLE)]));
        if let PaintEntry::Stroke(stroke) = &mut Arc::make_mut(&mut path.style).entries[0] { stroke.unknown = Some(Arc::from(vec![unknown(902, IGNORABLE | COPY_SAFE)])); }
        Record::Path(path)
    };
    let mut object = VectorObject::from_records((1..=150).map(decorated).collect());
    let mut issued: BTreeSet<ItemId> = object.paths().map(|(_, p)| p.id).collect();
    let mut history = vec![object.clone()];
    let pick = |object: &VectorObject, at: u64| object.paths().nth(at as usize % object.path_count().max(1)).map(|(_, p)| p.id);
    for _ in 0..400 {
        let before = object.clone();
        let step = next() % 7;
        object = match step {
            0 => {
                let ids: BTreeSet<ItemId> = (0..1 + next() % 5).filter_map(|_| pick(&object, next())).collect();
                let moved = object.transform_items(&ids, Affine64([1., 0., 0., 1., (next() % 40) as f64, 3.])).unwrap();
                for id in &ids {
                    let path = moved.record(moved.find(*id).unwrap()).unwrap().path().unwrap();
                    assert!(path.unknown.iter().flat_map(|u| u.iter()).all(|p| p.tag & COPY_SAFE != 0), "moving drops data that is not copy-safe");
                    assert!(path.unknown.iter().flat_map(|u| u.iter()).any(|p| p.tag & COPY_SAFE != 0), "and keeps copy-safe data");
                }
                moved
            }
            1 | 2 => {
                let Some(id) = pick(&object, next()) else { continue };
                let at = object.find(id).unwrap();
                let path = object.record(at).unwrap().path().unwrap().clone();
                let edited = if step == 1 { PathItem { name: Some(format!("n{}", next() % 100).into()), ..path.clone() } } else { path.with_style(Arc::new(Style::new(vec![PaintEntry::Fill(Fill { paint: color(0.7), unknown: None })]))) };
                let copy_safe: Vec<_> = path.unknown.iter().flat_map(|u| u.iter()).filter(|p| p.tag & COPY_SAFE != 0).cloned().collect();
                let kept: Vec<_> = edited.unknown.iter().flat_map(|u| u.iter()).cloned().collect();
                if step == 1 { assert_eq!(edited.unknown, path.unknown, "renaming keeps every property"); } else { assert_eq!(kept, copy_safe, "restyling keeps copy-safe properties"); }
                object.replace([(at, Some(Record::Path(edited)))])
            }
            3 => match pick(&object, next()) { Some(id) => object.replace([(object.find(id).unwrap(), None)]), None => continue },
            4 => {
                let id = object.allocate_id();
                assert!(issued.insert(id), "a new path never reuses an issued identity");
                object.insert_front(vec![decorated(id.0)])
            }
            5 if history.len() > 1 => { history.pop(); history.last().unwrap().clone() }
            _ => continue,
        };
        if step != 5 { history.push(before); history.push(object.clone()); history.swap_remove(history.len() - 2); }
        object.validate().unwrap();
        let encoded: Vec<(Vec<u8>, usize)> = object.chunks().iter().map(|c| (encode_chunk(c), c.records().len())).collect();
        let decoded = decode_object(&encoded.iter().map(|(b, n)| (&b[..], *n)).collect::<Vec<_>>()).unwrap();
        assert!(decoded.iter().zip(object.chunks()).all(|(a, b)| a.records() == b.records()) && decoded.len() == object.chunks().len(), "every state round-trips exactly");
    }
}

#[test]
fn arranging_paths_moves_them_in_stacking_order_and_rewrites_only_their_chunks() {
    let object = VectorObject::from_records((1..=200).map(|i| Record::Path(plain_line(i, [i as f64, 0.], [i as f64, 9.], 1.))).collect());
    let order = |object: &VectorObject| object.paths().map(|(_, p)| p.id.0).collect::<Vec<_>>();
    let ids = |list: &[u64]| list.iter().copied().map(ItemId).collect::<BTreeSet<_>>();
    let same = |a: &VectorObject, b: &VectorObject| a.chunks().iter().zip(b.chunks()).filter(|(x, y)| Arc::ptr_eq(x.storage(), y.storage())).count();

    let front = object.arrange(&ids(&[150, 5]), Arrange::Front);
    assert_eq!(order(&front)[..3], [5, 150, 1], "selected paths keep their relative order at the front");
    assert_eq!(order(&front).len(), 200);
    assert!(front.arrange(&ids(&[150, 5]), Arrange::Front).same_root(&front), "already in front is unchanged");
    let back = object.arrange(&ids(&[150, 5]), Arrange::Back);
    assert_eq!(order(&back)[197..], [200, 5, 150]);
    assert!(back.arrange(&ids(&[150, 5]), Arrange::Back).same_root(&back));

    let forward = object.arrange(&ids(&[10, 11, 70]), Arrange::Forward);
    assert_eq!(order(&forward)[7..12], [8, 10, 11, 9, 12], "a run moves past one path");
    assert_eq!(order(&forward)[67..71], [68, 70, 69, 71]);
    assert!(same(&object, &forward) >= object.chunks().len() - 3, "only the touched chunks change");
    let backward = object.arrange(&ids(&[64, 65, 200]), Arrange::Backward);
    assert_eq!(order(&backward)[62..67], [63, 66, 64, 65, 67], "a run crossing a chunk boundary moves together");
    assert_eq!(order(&backward)[199], 200, "the last path cannot move further back");
    assert!(object.arrange(&ids(&[1]), Arrange::Forward).same_root(&object), "the front path cannot move further forward");

    let opaque = Record::Opaque(OpaqueRecord { kind: 999 << 4 | IGNORABLE, id: 900, properties: Arc::from([]) });
    let walled = VectorObject::from_records(vec![Record::Path(plain_line(1, [0., 0.], [1., 1.], 1.)), opaque, Record::Path(plain_line(2, [0., 0.], [1., 1.], 1.))]);
    assert!(walled.arrange(&ids(&[2]), Arrange::Forward).same_root(&walled), "unknown records are not stepped over");
    assert_eq!(order(&walled.arrange(&ids(&[2]), Arrange::Front)), [2, 1]);
    walled.arrange(&ids(&[2]), Arrange::Front).validate().unwrap();
}

fn ink(id: u64, anchors: Vec<Anchor>, closed: bool, profile: Option<WidthProfile>) -> PathItem {
    let mut stroke = Stroke::new(color(0.2), 8.);
    (stroke.cap, stroke.join, stroke.profile) = (Cap::Round, Join::Round, profile.map(Arc::new));
    PathItem::new(ItemId(id), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, closed)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)])))
}
fn samples(path: &PathItem) -> Vec<([f64; 2], [f64; 2])> {
    let profile = path.style.strokes().next().unwrap().profile.clone();
    let mut out = Vec::new();
    let mut base = 0u32;
    for subpath in path.geometry.subpaths() {
        for (i, segment) in subpath.segments().enumerate() {
            for k in 0..=200 { let t = f64::from(k) / 200.; out.push((segment.point(t), profile.as_ref().map_or([1.; 2], |p| p.at(base + i as u32, t)))); }
        }
        base += subpath.segment_count() as u32;
    }
    out
}

#[test]
fn erasing_cuts_centerlines_into_pieces_that_keep_their_shape_and_widths() {
    let curve = vec![
        Anchor::new([0., 0.], [0.; 2], [30., -40.], AnchorKind::CORNER),
        Anchor::new([100., 0.], [-30., -30.], [30., 30.], AnchorKind::SYMMETRIC),
        Anchor::new([200., 40.], [-20., 0.], [0.; 2], AnchorKind::CORNER),
    ];
    let profile = WidthProfile::new(vec![WidthKnot::new(0, 0., 0.1, 0.2), WidthKnot::new(0, 0.7, 1., 0.8), WidthKnot::new(1, 0.4, 0.5, 1.), WidthKnot::new(1, 1., 0.2, 0.1)]).unwrap();
    let path = ink(1, curve, false, Some(profile));
    let original = samples(&path);
    let eraser = Eraser { from: [100., -40.], to: [100., 40.], radius: 6. };
    let pieces = path.erased(&eraser).expect("the eraser crosses the path");
    assert_eq!(pieces.len(), 2, "one crossing leaves two pieces");
    for piece in &pieces {
        piece.validate().unwrap();
        for (point, width) in samples(piece) {
            let (nearest, expected) = original.iter().min_by(|a, b| { let d = |q: &[f64; 2]| (q[0] - point[0]).hypot(q[1] - point[1]); d(&a.0).total_cmp(&d(&b.0)) }).unwrap();
            assert!((nearest[0] - point[0]).hypot(nearest[1] - point[1]) < 0.5, "piece points lie on the original curve");
            assert!((width[0] - expected[0]).abs() < 0.02 && (width[1] - expected[1]).abs() < 0.02, "widths follow the original profile: {width:?} vs {expected:?}");
            assert!(eraser.from[0] - point[0] >= 6. - 1e-3 || point[0] - eraser.from[0] >= 6. - 1e-3, "nothing inside the eraser remains");
        }
    }
    assert!(path.erased(&Eraser { from: [400., 0.], to: [400., 10.], radius: 6. }).is_none(), "a miss leaves the path alone");
    assert_eq!(path.erased(&Eraser { from: [-10., 0.], to: [210., 20.], radius: 80. }), Some(Vec::new()), "covering everything removes the path");

    let square = ink(2, [[0., 0.], [50., 0.], [50., 50.], [0., 50.]].map(Anchor::corner).to_vec(), true, None);
    let opened = square.erased(&Eraser { from: [25., -5.], to: [25., 5.], radius: 3. }).unwrap();
    assert_eq!(opened.len(), 1, "cutting a closed path once opens it");
    let subpath = &opened[0].geometry.subpaths()[0];
    assert!(!subpath.closed);
    assert_eq!(subpath.len(), 6, "the open piece runs from the cut through every corner back to the cut");
    assert!((subpath.anchor(0).point()[0] - 28.).abs() < 1e-3 && (subpath.anchor(5).point()[0] - 22.).abs() < 1e-3);

    let compound = PathItem::new(ItemId(3), Arc::new(PathGeometry::new(vec![
        Subpath::new(&[Anchor::corner([0., 0.]), Anchor::corner([100., 0.])], false),
        Subpath::new(&[Anchor::corner([0., 50.]), Anchor::corner([100., 50.])], false),
    ]).unwrap()), square.style.clone());
    let cut = compound.erased(&Eraser { from: [50., -5.], to: [50., 5.], radius: 3. }).unwrap();
    assert_eq!(cut.len(), 1, "a compound path stays one path");
    assert_eq!(cut[0].geometry.subpaths().len(), 3);
}

#[test]
fn erasing_a_layer_splits_or_removes_only_touched_paths_with_fresh_ids() {
    let object = VectorObject::from_records((1..=100).map(|i| Record::Path(plain_line(i, [0., i as f64 * 10.], [200., i as f64 * 10.], 2.))).collect());
    let mut index = VectorIndex::default();
    let eraser = Eraser { from: [100., 5.], to: [100., 52.], radius: 4. };
    let near = index.near(&object, eraser.bounds());
    let split = object.erase(&near, &eraser, false).unwrap();
    split.validate().unwrap();
    assert_eq!(split.path_count(), 105, "five crossed lines split in two");
    let order: Vec<u64> = split.paths().map(|(_, p)| p.id.0).take(11).collect();
    assert_eq!(order[..2], [1, order[1]], "a split keeps the first piece's identity in place");
    assert!(order[1] > 100, "the second piece gets a new identity");
    let whole = object.erase(&index.near(&object, eraser.bounds()), &eraser, true).unwrap();
    assert_eq!(whole.path_count(), 95, "Whole Path removes every touched path");
    assert!(object.erase(&index.near(&object, [500., 500., 510., 510.]), &Eraser { from: [505., 505.], to: [505., 505.], radius: 5. }, false).is_none());
}

#[test]
fn newer_vector_layers_open_from_their_fallback_locked_and_save_back_unchanged() {
    let image: crate::Image = crate::color::source::rgba8_source([32, 24], |x, y| [x as u8 * 8, y as u8 * 10, 90, 255]).into();
    let unknown = record_bytes(900 << 4, 7, &[(1 << 5, vec![1, 2, 3])]);
    let readable = encode_chunk(&chunk(vec![line(3, &[[0., 0.], [10., 10.]], 2.)]));
    let layer = |bytes: &[u8], records: u64| {
        let mut object = crate::ImageObject::new(image.clone());
        object.affine = crate::authored::Affine64([1., 0., 0., 1., 40., 30.]);
        object.vector = Some(crate::authored::PreservedVector(Arc::from([(serde_json::json!({"records": records}), crate::authored::Resource::new(Arc::<[u8]>::from(bytes)))])));
        let mut editor = crate::Editor::new(crate::Document::from_artwork(crate::Artwork::new([200, 150]).unwrap()).unwrap());
        let (_, edit) = editor.document().create_object_layer_edit("Newer", object, None, 0).unwrap();
        editor.perform(edit).unwrap();
        editor
    };
    let saved = save(&layer(&unknown, 1));
    let reopened = open(saved.clone());
    assert!(reopened.vectors.is_empty(), "this reader cannot read the paths");
    let (_, _, object) = reopened.objects.iter().next().expect("the layer shows its fallback image");
    assert_eq!((object.image.extent, object.affine.0[4]), ([32, 24], 40.));
    let preserved = object.vector.as_ref().expect("the unreadable paths are kept");
    assert_eq!(preserved.0.len(), 1);
    assert_eq!(&**preserved.0[0].1.storage(), &unknown[..], "item records keep their bytes");
    let document = crate::Document::from_artwork(reopened.clone()).unwrap();
    let occurrence = *document.scene().order().iter().find(|h| document.scene().object_layer(**h).is_some()).unwrap();
    assert!(document.is_locked(occurrence), "a layer from a newer version cannot be edited");
    let again = open(save(&crate::Editor::new(document)));
    assert_eq!(&**again.objects.iter().next().unwrap().2.vector.as_ref().unwrap().0[0].1.storage(), &unknown[..], "saving keeps the paths for the newer version");

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let editor = crate::Editor::new(crate::Document::from_artwork(open(saved)).unwrap());
    let capture = editor.capture_session(editor.capture(1, Default::default()).unwrap()).unwrap();
    let session = super::session::PreparedSession::prepare(&capture, serde_json::json!({}), &cancel).unwrap();
    let mut pack = Vec::new();
    std::io::Read::read_to_end(&mut session.resources().reader(&cancel), &mut pack).unwrap();
    let loaded = super::session::open_parts(session.metadata(), super::ImmutableBacking::new(Arc::new(Arc::<[u8]>::from(pack))).unwrap(), crate::ProjectLimits::default(), &cancel).unwrap();
    let restored = loaded.editor.document().artwork.objects.iter().next().unwrap().2;
    assert_eq!(&**restored.vector.as_ref().unwrap().0[0].1.storage(), &unknown[..], "sessions keep the paths too");

    let current = open(save(&layer(&readable, 1)));
    assert!(current.objects.is_empty() && vector(&current).path_count() == 1, "a reader that knows every record opens the paths and drops the fallback");
}
