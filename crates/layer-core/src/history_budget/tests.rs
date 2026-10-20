use super::*;
use color::{ColorProfile, RgbSpace, SampleDepth, source::*};
fn document(size: [u32; 2]) -> Document {
    Document::new(
        PortableId::random(),
        size[0],
        size[1],
        DocumentNames {
            paint: "Current ink".into(),
            paper: "Paper".into(),
        },
    )
}
fn selection_edit(document: &Document, selection: Option<Selection>) -> Edit {
    let mut working = document.working.clone();
    working.selection = selection;
    Edit::Working(working)
}
fn paint_handle(document: &Document) -> PaintHandle {
    let SourceTarget::Paint(h) = document.working.target.unwrap() else {
        panic!()
    };
    h
}
fn paint_change(document: &Document, h: PaintHandle, source: Option<Arc<SourceImage>>) -> Edit {
    let mut s = document.artwork.paint.get(h).unwrap().clone();
    s.base = source.map(|source|PaintBase::new(source.into()));
    Edit::Paint(RecordChange::replace(&document.artwork.paint, h, Some(s)).unwrap())
}
#[test]
fn image_and_proof_history_share_one_profile_allocation() {
    let profile=ColorProfile::Icc(vec![17;512].into());
    let mut source=(*rgba8_source([1,1],|_,_|[0,0,0,255])).clone();source.interpretation.profile=profile.clone();
    let source=Arc::new(source);let mut doc=document([256;2]);let h=paint_handle(&doc);
    doc.artwork.paint.get_mut(h).unwrap().base=Some(PaintBase::new(source.clone().into()));
    let mut output=doc.artwork.outputs.get(doc.artwork.default_output).unwrap().clone();output.proof=Some(color::ProofRecipe::new("Proof".into(),profile.clone()));
    let edit=Edit::Output(RecordChange::replace(&doc.artwork.outputs,doc.artwork.default_output,Some(output)).unwrap());
    let entry=HistoryEntry::new(edit,0);assert_eq!(Accounting::new(&doc).charge(&entry),entry.metadata_bytes);
    let mut accounting=Accounting::default();assert_eq!(accounting.charge_profile(&profile),512);
    assert_eq!(accounting.sources.charge(&source),std::mem::size_of::<SourceImage>()+96+source.resident_bytes());
    assert_eq!(accounting.charge_profile(&profile),0);
}
#[test]
fn selection_history_charges_shared_coverage_once_and_rejects_oversized_edits() {
    let mask = Selection::pixels(Arc::new(
        SelectionPixels::bytes([1024, 1], [0, 0, 1024, 1], vec![u32::MAX; 256]).unwrap(),
    ));
    let mut d = document([1024, 1]);
    d.working.selection = Some(mask.clone());
    let entry = HistoryEntry::new(selection_edit(&d, Some(mask.clone())), 0);
    assert!(
        entry.metadata_bytes < 1024,
        "coverage is not serialized into history metadata"
    );
    assert_eq!(Accounting::new(&d).charge(&entry), entry.metadata_bytes);
    let mut accounting = Accounting::default();
    assert_eq!(accounting.charge(&entry), entry.metadata_bytes + 1024);
    assert_eq!(accounting.charge(&entry), entry.metadata_bytes);
    let mut editor = Editor::new(document([1024, 1]));
    let before = editor.document.clone();
    let edit = selection_edit(editor.document(), Some(mask));
    assert!(editor.perform_with_history_budget(edit, 1023).is_err());
    assert_eq!(editor.document, before);
    assert!(!editor.can_undo());
}
#[test]
fn retained_selection_inventory_counts_binary_ownership_across_history_and_saved_selections() {
    let extent = [9504, 6336];
    let bytes = extent[0] as usize * extent[1] as usize;
    let selection = Selection::pixels(Arc::new(
        SelectionPixels::bytes(
            extent,
            [0, 0, extent[0], extent[1]],
            vec![u32::MAX; bytes / 4],
        )
        .unwrap(),
    ));
    let mut d = document(extent);
    d.working.selection = Some(selection.clone());
    let saved = RecordChange::insert(
        &d.artwork.selections,
        SavedSelection {
            selection: selection.clone(),
        },
    );
    let saved_handle = saved.handle;
    d.apply(Edit::SavedSelection(saved)).unwrap();
    let mut editor = Editor::new(d);
    let retained = editor.retained_tiles().metadata_bytes;
    assert!(
        (bytes..bytes + 32 * 1024).contains(&retained),
        "Shared 61 MP coverage is one 60 MB allocation: {retained}"
    );
    editor
        .perform(selection_edit(editor.document(), None))
        .unwrap();
    let retained = editor.retained_tiles().metadata_bytes;
    assert!((bytes..bytes + 32 * 1024).contains(&retained));
    let s = editor
        .document()
        .artwork
        .selections
        .get(saved_handle)
        .unwrap()
        .clone();
    let entry = HistoryEntry::new(
        Edit::SavedSelection(
            RecordChange::replace(
                &editor.document().artwork.selections,
                saved_handle,
                Some(s),
            )
            .unwrap(),
        ),
        0,
    );
    assert!(
        entry.metadata_bytes < 16 * 1024,
        "Saved selection pixels stay out of history metadata"
    );
    let mut accounting = Accounting::default();
    assert_eq!(accounting.charge(&entry), bytes + entry.metadata_bytes);
    assert_eq!(accounting.charge(&entry), entry.metadata_bytes);
}
#[test]
fn admitted_native_output_reservation_is_shared_until_tile_publication() {
    let d = document([256; 2]);
    let revision = raster::RasterRevision::pending();
    let h = paint_handle(&d);
    let mut s = d.artwork.paint.get(h).unwrap().clone();
    s.raster = revision.clone();
    let entry = HistoryEntry::new(
        Edit::Paint(RecordChange::replace(&d.artwork.paint, h, Some(s)).unwrap()),
        0,
    );
    let charge = || Accounting::new(&d).charge(&entry);
    let initial = charge();
    revision.clone().reserve_pending_bytes(480 * 1024 * 1024);
    assert_eq!(
        charge() - initial,
        480 * 1024 * 1024 - raster::MAX_CAPTURE_BYTES as usize
    );
    revision.reserve_pending_bytes(1);
    assert_eq!(
        charge() - initial,
        480 * 1024 * 1024 - raster::MAX_CAPTURE_BYTES as usize
    );
    revision.publish(Ok(raster::RasterData::default())).unwrap();
    assert_eq!(charge(), initial - raster::MAX_CAPTURE_BYTES as usize);
}
fn source() -> Arc<SourceImage> {
    let mut builder = SourceBuilder::new(
        [1, 1],
        SourceInterpretation {
            channels: SourceChannels::Rgba,
            depth: SampleDepth::U8,
            profile: ColorProfile::Icc(vec![19; 16 * 1024].into()),
            profile_assumed: false,
        },
        256 * 256 * 4,
    )
    .unwrap();
    builder.push_row(&[31, 127, 200, 255]).unwrap();
    Arc::new(builder.finish().unwrap())
}
#[test]
fn over_budget_source_changes_reject_atomically_in_both_directions() {
    for remove in [false, true] {
        let mut d = document([256; 2]);
        let h = paint_handle(&d);
        let original = source();
        if remove {
            d.artwork.paint.get_mut(h).unwrap().base = Some(PaintBase::new(original.clone().into()));
        }
        let mut editor = Editor::new(d);
        let occurrence = editor.document().working.occurrence.unwrap();
        let mut value = editor
            .document()
            .artwork
            .occurrences
            .get(occurrence)
            .unwrap()
            .clone();
        value.opacity = 0.7;
        editor
            .perform(Edit::Occurrence(
                RecordChange::replace(
                    &editor.document().artwork.occurrences,
                    occurrence,
                    Some(value),
                )
                .unwrap(),
            ))
            .unwrap();
        editor.undo().unwrap();
        let before = editor.document.clone();
        let checkpoint = editor.checkpoint();
        let next = editor.next_checkpoint;
        let edit = paint_change(editor.document(), h, (!remove).then_some(original));
        let error = editor
            .perform_with_history_budget(edit.clone(), 8192)
            .unwrap_err();
        assert!(error.to_string().contains("Undo/Redo memory limit"));
        assert_eq!(editor.document, before);
        assert_eq!(editor.checkpoint(), checkpoint);
        assert_eq!(editor.next_checkpoint, next);
        assert!(editor.can_redo());
        assert!(!editor.can_undo());
        editor.perform_with_history_budget(edit, 65536).unwrap();
        let after = editor.document.clone();
        assert!(editor.can_undo());
        assert!(!editor.can_redo());
        editor.undo().unwrap();
        assert_eq!(editor.document.artwork, before.artwork);
        editor.redo().unwrap();
        assert_eq!(editor.document.artwork, after.artwork);
    }
}
#[test]
fn reinterpretation_charges_shared_tiles_once_and_new_profile_ownership() {
    let mut d = document([256; 2]);
    let h = paint_handle(&d);
    let original = source();
    d.artwork.paint.get_mut(h).unwrap().base = Some(PaintBase::new(original.clone().into()));
    let mut repaired = original.as_ref().clone();
    repaired.interpretation.profile = ColorProfile::Builtin(RgbSpace::DisplayP3);
    let mut editor = Editor::new(d);
    let edit = paint_change(editor.document(), h, Some(Arc::new(repaired)));
    assert!(
        editor
            .perform_with_history_budget(edit.clone(), 8192)
            .is_err()
    );
    editor.perform_with_history_budget(edit, 65536).unwrap();
    let repaired = editor
        .document()
        .artwork
        .paint
        .get(h)
        .unwrap()
        .base
        .as_ref()
        .unwrap().image.storage();
    assert!(Arc::ptr_eq(
        &repaired.tiles[&[0, 0]],
        &original.tiles[&[0, 0]]
    ));
    editor.undo().unwrap();
    assert!(Arc::ptr_eq(
        editor
            .document()
            .artwork
            .paint
            .get(h)
            .unwrap()
            .base
            .as_ref()
            .unwrap().image.storage(),
        &original
    ));
}

fn hatched_layer(count: u64) -> (Editor, OccurrenceHandle) {
    use authored::vector::*;
    let records = (1..=count).map(|id| {
        let [x, y] = [(id % 150) as f64 * 40., (id / 150) as f64 * 30.];
        let anchors = [Anchor::corner([x, y]), Anchor::corner([x + 25., y + 18.]), Anchor::corner([x + 31., y + 4.])];
        let stroke = Stroke::new(Paint::color(color::RgbColor::BLACK), 3.);
        Record::Path(PathItem::new(ItemId(id), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, false)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)]))))
    }).collect();
    let doc = document([6000, 4000]);
    let (layer, edit) = doc.create_vector_layer_edit("Ink", VectorObject::from_records(records), None, 0).unwrap();
    let mut editor = Editor::new(doc);
    editor.perform(edit).unwrap();
    (editor, layer)
}
fn move_paths(editor: &Editor, layer: OccurrenceHandle, ids: &[u64], by: f64) -> Edit {
    let object = editor.document().scene().vector_layer(layer).unwrap();
    let ids = ids.iter().map(|id| authored::vector::ItemId(*id)).collect();
    editor.document().vector_object_edit(layer, object.transform_items(&ids, authored::Affine64([1., 0., 0., 1., by, by * 0.5]), true).unwrap()).unwrap()
}
fn walked(editor: &mut Editor) -> usize { editor.ledger.get_mut().unwrap().walked }

#[test]
fn one_path_edits_charge_their_own_chunks_not_the_layers() {
    let (mut editor, layer) = hatched_layer(20_000);
    let chunks = editor.document().scene().vector_layer(layer).unwrap().chunks().len();
    editor.perform(move_paths(&editor, layer, &[7], 2.)).unwrap();
    for (step, id) in [5_000, 12_345, 19_999].into_iter().enumerate() {
        let before = walked(&mut editor);
        editor.perform(move_paths(&editor, layer, &[id], 3.)).unwrap();
        let visited = walked(&mut editor) - before;
        assert!(visited <= 2 + 2 * (step + 3), "a one-path edit visited {visited} of {chunks} chunks");
    }
}

#[test]
fn ledger_charges_match_a_full_recount_through_undo_redo_amends_and_previews() {
    let (mut editor, layer) = hatched_layer(3_000);
    let recount = |editor: &mut Editor| -> [Vec<usize>; 2] {
        let mut full = Accounting::new(&editor.document);
        let full = editor.undo.iter().rev().chain(editor.redo.iter().rev()).map(|entry| {
            let mut inventory = RootInventory::default();
            entry.edit.roots(&mut inventory);
            entry.metadata_bytes + full.charge_inventory(inventory, &[])
        }).collect();
        let ledger = editor.ledger.get_mut().unwrap();
        ledger.sync(&editor.document);
        let mut ledgered = Accounting::with_ledger(&editor.document, ledger, false);
        let ledgered = editor.undo.iter().rev().chain(editor.redo.iter().rev()).map(|entry| ledgered.charge(entry)).collect();
        [full, ledgered]
    };
    let check = |editor: &mut Editor, what: &str| { let [full, ledgered] = recount(editor); assert_eq!(full, ledgered, "{what}"); };
    for (i, ids) in [&[1u64][..], &[40, 41, 900], &(1..400).collect::<Vec<_>>(), &[2_999]].into_iter().enumerate() {
        editor.perform(move_paths(&editor, layer, ids, 1. + i as f64)).unwrap();
        check(&mut editor, "after an edit");
    }
    editor.undo().unwrap();
    editor.undo().unwrap();
    check(&mut editor, "after undo");
    editor.redo().unwrap();
    check(&mut editor, "after redo");
    editor.perform(move_paths(&editor, layer, &[2_998], 4.)).unwrap();
    editor.amend(move_paths(&editor, layer, &[12], 5.)).unwrap();
    check(&mut editor, "after an amend");
    editor.preview(move_paths(&editor, layer, &[13, 14], 6.)).unwrap();
    check(&mut editor, "while previewing");
    editor.perform(move_paths(&editor, layer, &[15], 7.)).unwrap();
    check(&mut editor, "after an edit over a preview");
    let (copy, edit) = editor.document().create_vector_layer_edit("Copy", editor.document().scene().vector_layer(layer).unwrap().clone(), None, 0).unwrap();
    editor.perform(edit).unwrap();
    editor.perform(editor.document().delete_layers_edit(&[copy]).unwrap()).unwrap();
    check(&mut editor, "after sharing and deleting a layer");
}
