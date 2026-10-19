use super::*;

fn paint() -> Paint { Paint::color(crate::color::RgbColor::new(Default::default(), [0.1, 0.2, 0.3, 1.]).unwrap()) }
fn path(id: u64, points: &[[f64; 2]], closed: bool, width: f64, knots: Option<Vec<WidthKnot>>) -> PathItem {
    let anchors: Vec<Anchor> = points.iter().map(|p| Anchor::new(*p, [-2., 1.], [2., -1.], AnchorKind::SMOOTH)).collect();
    let mut stroke = Stroke::new(paint(), width);
    stroke.profile = knots.map(|k| Arc::new(WidthProfile::new(k).unwrap()));
    PathItem::new(ItemId(id), Arc::new(PathGeometry::new(vec![Subpath::new(&anchors, closed)]).unwrap()), Arc::new(Style::new(vec![PaintEntry::Stroke(stroke)])))
}
fn tapered(id: u64, points: &[[f64; 2]]) -> PathItem {
    let last = points.len() as u32 - 2;
    path(id, points, false, 4., Some(vec![WidthKnot::new(0, 0., 0.2, 0.4), WidthKnot::new(last, 1., 1., 0.8)]))
}
fn profile(path: &PathItem) -> &WidthProfile { path.style.strokes().next().unwrap().profile.as_deref().unwrap() }
fn points(path: &PathItem) -> Vec<[f64; 2]> { path.geometry.subpaths().iter().flat_map(|s| s.anchors().map(Anchor::point)).collect() }
fn near(a: [f64; 2], b: [f64; 2]) -> bool { (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4 }

#[test]
fn reversing_runs_anchors_handles_and_widths_backwards() {
    let line = tapered(1, &[[0., 0.], [10., 0.], [20., 5.], [30., 0.]]);
    let reversed = line.reversed().unwrap();
    assert_eq!(points(&reversed), points(&line).into_iter().rev().collect::<Vec<_>>());
    let (a, b) = (&line.geometry.subpaths()[0], &reversed.geometry.subpaths()[0]);
    assert_eq!((b.anchor(0).in_handle(), b.anchor(0).out_handle()), (a.anchor(3).out_handle(), a.anchor(3).in_handle()));
    for (segment, t) in [(0, 0.), (1, 0.3), (2, 1.)] {
        let [l, r] = profile(&line).at(segment, t);
        assert!(near(profile(&reversed).at(2 - segment, 1. - t), [r, l]), "sides swap along the reversed line");
    }
    assert!(near(b.segment(0).point(0.25), a.segment(2).point(0.75)));
    assert_eq!(reversed.reversed().unwrap().geometry, line.geometry);
    let square = path(2, &[[0., 0.], [10., 0.], [10., 10.], [0., 10.]], true, 2., None);
    let turned = square.reversed().unwrap();
    for i in 0..4 { assert!(near(turned.geometry.subpaths()[0].segment(i).point(0.4), square.geometry.subpaths()[0].segment([2, 1, 0, 3][i]).point(0.6)), "segment {i}"); }
}

#[test]
fn closing_adds_one_segment_that_eases_between_the_end_widths() {
    let line = tapered(1, &[[0., 0.], [10., 0.], [20., 5.]]);
    let closed = line.closed().unwrap();
    let subpath = &closed.geometry.subpaths()[0];
    assert!(subpath.closed && subpath.segment_count() == 3);
    assert!(near(profile(&closed).at(2, 0.), [1., 0.8]) && near(profile(&closed).at(2, 1.), [0.2, 0.4]));
    profile(&closed).validate_for(3).unwrap();
    assert!(!closed.can_close() && line.can_close());
}

#[test]
fn splitting_at_anchors_keeps_curves_and_widths_in_each_piece() {
    let line = tapered(1, &[[0., 0.], [10., 0.], [20., 5.], [30., 0.]]);
    let pieces = line.split_at(&[AnchorRef { subpath: 0, anchor: 1 }, AnchorRef { subpath: 0, anchor: 2 }].into()).unwrap();
    assert_eq!(pieces.iter().map(points).collect::<Vec<_>>(), [vec![[0., 0.], [10., 0.]], vec![[10., 0.], [20., 5.]], vec![[20., 5.], [30., 0.]]]);
    for (i, piece) in pieces.iter().enumerate() {
        assert!(near(piece.geometry.subpaths()[0].segment(0).point(0.5), line.geometry.subpaths()[0].segment(i).point(0.5)));
        assert!(near(profile(piece).at(0, 0.5), profile(&line).at(i as u32, 0.5)));
    }
    assert_eq!(line.split_at(&[AnchorRef { subpath: 0, anchor: 0 }].into()).unwrap(), vec![line.clone()], "an end is not a cut");
    let square = path(2, &[[0., 0.], [10., 0.], [10., 10.], [0., 10.]], true, 2., None);
    let [opened] = &square.split_at(&[AnchorRef { subpath: 0, anchor: 2 }].into()).unwrap()[..] else { panic!("one opened path") };
    assert_eq!(points(opened), [[10., 10.], [0., 10.], [0., 0.], [10., 0.], [10., 10.]]);
    assert!(!opened.geometry.subpaths()[0].closed);
}

#[test]
fn joining_meets_the_nearest_ends_and_bridges_a_gap() {
    let a = tapered(1, &[[0., 0.], [10., 0.]]);
    let b = path(2, &[[30., 0.], [10., 0.]], false, 8., None);
    let joined = a.joined(&b, 0.5).unwrap();
    assert_eq!(points(&joined), [[0., 0.], [10., 0.], [30., 0.]], "touching ends merge, the far path turns around");
    assert_eq!(joined.id, a.id);
    assert!(near(profile(&joined).at(1, 0.5), [2., 2.]), "the other path keeps its own width");
    let c = path(3, &[[50., 0.], [60., 0.]], false, 4., None);
    let bridged = a.joined(&c, 0.5).unwrap();
    assert_eq!(points(&bridged), [[0., 0.], [10., 0.], [50., 0.], [60., 0.]]);
    assert!(bridged.geometry.subpaths()[0].segment(1).is_line(), "a straight segment bridges the gap");
    assert!(near(profile(&bridged).at(1, 0.), [1., 0.8]) && near(profile(&bridged).at(1, 1.), [1., 1.]));
    assert!(a.joined(&path(4, &[[0., 0.], [5., 5.], [9., 0.]], true, 1., None), 0.5).is_err(), "closed paths do not join");
}

#[test]
fn compound_paths_combine_and_release_into_their_subpaths() {
    let outer = path(1, &[[0., 0.], [100., 0.], [100., 100.], [0., 100.]], true, 2., None);
    let inner = path(2, &[[30., 30.], [70., 30.], [70., 70.], [30., 70.]], true, 4., None);
    let compound = PathItem::compound(&[outer.clone(), inner.clone()]).unwrap();
    assert_eq!((compound.id, compound.fill_rule, compound.geometry.subpaths().len()), (outer.id, FillRule::EvenOdd, 2));
    assert!(near(profile(&compound).at(5, 0.5), [2., 2.]), "the inner outline keeps its width");
    let released = compound.released().unwrap();
    assert_eq!(released.iter().map(points).collect::<Vec<_>>(), [points(&outer), points(&inner)]);
}

#[test]
fn object_operations_replace_paths_in_place_with_fresh_identities() {
    let object = VectorObject::from_records(vec![
        Record::Path(tapered(1, &[[0., 0.], [10., 0.], [20., 0.]])),
        Record::Path(path(2, &[[20., 0.], [40., 0.]], false, 4., None)),
        Record::Path(path(3, &[[0., 50.], [10., 50.]], false, 4., None)),
    ]);
    let (joined, id) = object.join_paths(&[ItemId(1), ItemId(2)].into(), 0.5).unwrap().unwrap();
    assert_eq!((id, joined.path_count()), (ItemId(1), 2));
    assert_eq!(points(joined.record(joined.find(id).unwrap()).unwrap().path().unwrap()).len(), 4);
    let (closed, _) = object.join_paths(&[ItemId(3)].into(), 0.5).unwrap().unwrap();
    assert!(closed.paths().any(|(_, p)| p.id == ItemId(3) && p.geometry.subpaths()[0].closed), "Join closes one open path");
    let (split, selected) = object.split_paths(&[(ItemId(1), [AnchorRef { subpath: 0, anchor: 1 }].into())].into()).unwrap().unwrap();
    assert_eq!((split.path_count(), selected.len()), (4, 2));
    assert!(selected.contains(&ItemId(1)));
    split.validate().unwrap();
    let order: Vec<ItemId> = split.paths().map(|(_, p)| p.id).collect();
    assert_eq!(order[0], ItemId(1), "pieces keep the split path's place");
    let (compound, id) = object.compound_paths(&[ItemId(1), ItemId(3)].into()).unwrap().unwrap();
    assert_eq!((id, compound.path_count()), (ItemId(3), 2), "the compound takes the back-most path's style and place");
    let (released, ids) = compound.release_paths(&[id].into()).unwrap().unwrap();
    assert_eq!((released.path_count(), ids.len()), (3, 2));
    released.validate().unwrap();
    assert!(object.release_paths(&[ItemId(1)].into()).unwrap().is_none());
}
