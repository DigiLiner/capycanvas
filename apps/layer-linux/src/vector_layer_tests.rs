//! GTK vector layers: G-Pen lines with actual compositor contacts, then
//! selecting, dragging, undoing and deleting paths with the Move tool.
use super::*;
use super::selection_tools::{canvas_drag, canvas_point};
use layer_core::authored::vector::PathItem;

fn paths(d: &Driver) -> Vec<PathItem> {
    let session = ui_session(&d.w);
    let doc = session.engine().document();
    doc.working.occurrence.and_then(|layer| doc.scene().vector_layer(layer)).map_or_else(Vec::new, |object| object.paths().map(|(_, p)| p.clone()).collect())
}
fn until_paths(d: &Driver, what: &str, done: impl Fn(&[PathItem]) -> bool) -> Vec<PathItem> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        pump(20);
        let paths = paths(d);
        if done(&paths) { return paths; }
        assert!(Instant::now() < deadline, "{what} timed out: {}", d.w.status.text());
    }
}
fn start(path: &PathItem) -> [f32; 2] { path.geometry.subpaths()[0].anchor(0).point().map(|v| v as f32) }

#[test]
#[ignore = "isolated native-input.js --native-test=native_vector_draw_move_and_delete --tablet"]
fn native_vector_draw_move_and_delete() {
    let mut d = Driver::new("art.capycanvas.VectorMove");
    d.w.dispatch(UiAction::Layer { action: LayerAction::NewVector });
    d.w.dispatch(UiAction::SelectBrush { id: layer_core::DefaultBrushPreset::GPen as u32 });
    pump(300);
    for row in 0..2 {
        let points: Vec<[f32; 2]> = (0..48).map(|i| canvas_point(&d, [500. + i as f32 * 14., 600. + row as f32 * 260. + (i as f32 * 0.25).sin() * 90.])).collect();
        let mut script = vec![serde_json::json!({"pen":"move","point":points[0]}), serde_json::json!({"pen":"down","point":points[0]})];
        script.extend(points[1..].iter().map(|p| serde_json::json!({"pen":"move","point":p})));
        script.extend([serde_json::json!({"pen":"up"}), serde_json::json!({"pen":"leave"})]);
        d.input.perform(serde_json::Value::Array(script));
        until_paths(&d, "a fitted G-Pen line", |paths| paths.len() == row + 1);
    }
    d.capture_canvas("vector-lines.png");
    d.w.dispatch(UiAction::Invoke { command: CommandId::Move });
    pump(200);
    let before = paths(&d);
    let front = before[0].clone();
    let [x, y] = start(&front);
    canvas_drag(&mut d, [x, y], [x + 150., y + 90.], true);
    let moved = until_paths(&d, "the dragged path", |paths| paths.iter().any(|p| p.id == front.id && (start(p)[0] - x - 150.).abs() < 2. && (start(p)[1] - y - 90.).abs() < 2.));
    assert!(moved.iter().any(|p| p.id == before[1].id && start(p) == start(&before[1])), "the other line stays");
    assert!(ui_session(&d.w).engine().document().working.vector_items.as_ref().is_some_and(|s| s.items.contains(&front.id)));
    d.capture_canvas("vector-moved.png");
    d.w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    until_paths(&d, "undoing the move", |paths| paths.iter().any(|p| p.id == front.id && start(p) == [x, y]));
    let [x0, y0, x1, y1] = before[1].bounds().map(|v| v as f32);
    canvas_drag(&mut d, [x0 - 20., y0 - 20.], [x1 + 20., y1 + 20.], true);
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ui_session(&d.w).engine().document().working.vector_items.as_ref().is_some_and(|s| s.items.contains(&before[1].id)) {
        pump(20);
        assert!(Instant::now() < deadline, "marquee selection timed out");
    }
    d.capture_canvas("vector-marquee.png");
    d.w.dispatch(UiAction::Invoke { command: CommandId::ClearSelected });
    until_paths(&d, "deleting the selected line", |paths| paths.iter().all(|p| p.id != before[1].id));
    d.capture_canvas("vector-deleted.png");
    d.finish();
}
