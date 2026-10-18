use super::*;
use super::super::test_support::*;
use layer_engine::SampleFlags;
use std::sync::Arc;

fn held_session() -> UiSession<Recorder> {
    let mut s = session(Platform::Gtk);
    s.frame(1, 1).unwrap();
    s.renderer_mut().painting_unready = true;
    s
}

fn settle(s: &mut UiSession<Recorder>) {
    for frame in 2..40 {
        s.frame(frame * 10_000_000, frame * 10_000_000).unwrap();
    }
}

fn predicted(s: &UiSession<Recorder>, sequence: u64) -> PenEvent {
    PenEvent { flags: SampleFlags(SampleFlags::PRIMARY.0 | SampleFlags::PREDICTED.0), ..event(s, sequence, PenPhase::Move, 0.5) }
}

#[test]
fn a_contact_held_for_painting_paints_before_the_edits_that_follow_it() {
    let mut s = held_session();
    let committed = s.engine.metrics().committed_strokes;
    let diameter = s.state.brush.diameter;
    for (sequence, phase) in [(1, PenPhase::Down), (2, PenPhase::Move), (3, PenPhase::Up)] {
        s.pen(event(&s, sequence, phase, 0.5));
    }
    s.pen(predicted(&s, 4));
    assert!(s.holding_input());
    assert_eq!(s.deferred_edits.len(), 3, "predictions of a held contact are dropped");
    s.dispatch(UiAction::SetBrushSize { value: diameter * 4. }).unwrap();
    assert_eq!(s.state.brush.diameter, diameter, "a later brush change waits behind the held contact");
    assert!(!s.command(CommandId::SaveDocument).enabled);
    assert!(!s.can_park_document());
    settle(&mut s);
    assert!(s.holding_input(), "the contact waits while painting is not ready");
    assert_eq!(s.engine.metrics().committed_strokes, committed);
    s.renderer_mut().painting_unready = false;
    settle(&mut s);
    assert!(!s.holding_input() && s.deferred_edits.is_empty());
    assert_eq!(s.engine.metrics().committed_strokes, committed + 1);
    assert_eq!(s.state.brush.diameter, diameter * 4.);
    let radius = s.renderer_mut().recorded_dabs.iter().map(|dab| dab.radii[0]).fold(0., f32::max);
    assert!(radius > 0. && radius <= diameter * 0.5 + 0.01, "the stroke used the brush of its contact: {radius}");
}

#[test]
fn a_held_contact_and_the_tool_chosen_after_it_keep_their_order() {
    let mut s = held_session();
    let committed = s.engine.metrics().committed_strokes;
    s.pen(event(&s, 1, PenPhase::Down, 0.5));
    s.pen(event(&s, 2, PenPhase::Move, 0.5));
    invoke(&mut s, CommandId::Eraser);
    assert_ne!(s.state.brush.tool, Tool::Eraser, "the tool change waits behind the held stroke");
    s.pen(event(&s, 3, PenPhase::Up, 0.5));
    s.renderer_mut().painting_unready = false;
    settle(&mut s);
    assert_eq!(s.engine.metrics().committed_strokes, committed + 1);
    assert_eq!(s.renderer_mut().last_style.as_ref().unwrap().mode, layer_render::DabMode::Paint);
    assert_eq!(s.state.brush.tool, Tool::Eraser);
}

#[test]
fn cancelled_expired_and_oversized_held_contacts_are_discarded() {
    let mut s = held_session();
    let committed = s.engine.metrics().committed_strokes;
    s.pen(event(&s, 1, PenPhase::Down, 0.5));
    s.pen(event(&s, 2, PenPhase::Cancel, 0.5));
    assert!(!s.holding_input(), "a cancelled held contact is dropped");

    s.pen(event(&s, 3, PenPhase::Down, 0.5));
    s.deferred_edits[0].held_since = Some(Instant::now() - HOLD - Duration::from_millis(1));
    s.frame(2, 2).unwrap();
    assert!(!s.holding_input(), "a contact never ready in time is dropped");
    s.pen(event(&s, 4, PenPhase::Move, 0.5));
    s.pen(event(&s, 5, PenPhase::Up, 0.5));
    assert!(!s.holding_input(), "the rest of it is ignored");

    s.pen(event(&s, 6, PenPhase::Down, 0.5));
    for sequence in 7..8 + HELD_SAMPLES as u64 {
        s.pen(event(&s, sequence, PenPhase::Move, 0.5));
    }
    assert!(!s.holding_input(), "an oversized contact is dropped");
    s.pen(event(&s, 9 + HELD_SAMPLES as u64, PenPhase::Up, 0.5));
    s.renderer_mut().painting_unready = false;
    settle(&mut s);
    assert!(!s.holding_input());
    assert_eq!(s.engine.metrics().committed_strokes, committed);
    s.pen(event(&s, 10 + HELD_SAMPLES as u64, PenPhase::Down, 0.5));
    s.pen(event(&s, 11 + HELD_SAMPLES as u64, PenPhase::Up, 0.5));
    settle(&mut s);
    assert_eq!(s.engine.metrics().committed_strokes, committed + 1, "the device paints again after its discarded contact ends");
}

#[test]
fn suspension_retires_held_input_with_its_queue() {
    let mut s = held_session();
    s.pen(event(&s, 1, PenPhase::Down, 0.5));
    s.pen(event(&s, 2, PenPhase::Up, 0.5));
    assert!(s.require_raster_snapshot().is_err());
    s.suspend_renderer().unwrap();
    assert!(!s.holding_input());
    assert!(s.require_raster_snapshot().is_ok());
}

fn paper_source() -> layer_core::color::source::SourceImage {
    use layer_core::color::source::{SourceBuilder, SourceChannels, SourceInterpretation};
    let interpretation = SourceInterpretation { channels: SourceChannels::Gray, depth: layer_core::color::SampleDepth::U8,
        profile: Default::default(), profile_assumed: false };
    let mut builder = SourceBuilder::new([4, 4], interpretation, 1 << 20).unwrap();
    for row in 0..4u8 { builder.push_row(&[row * 40; 4]).unwrap(); }
    let mut source = builder.finish().unwrap();
    source.resolution = Some(layer_core::ImageResolution::ppi(300));
    source
}

#[test]
fn a_paper_that_loads_during_a_stroke_applies_after_it() {
    let mut s = session(Platform::Gtk);
    s.frame(1, 1).unwrap();
    let paper = crate::occurrence_token(s.engine.document().bottom_paper().unwrap());
    let id = "01".repeat(32);
    s.papers.pending = Some(crate::paper_library::PendingPaper { id: id.clone(), layer: paper, epoch: s.state.document_file.epoch, image: None });
    s.pen(event(&s, 1, PenPhase::Down, 0.5));
    s.frame(2, 2).unwrap();
    s.paper_loaded(&id, paper_source()).unwrap();
    assert!(s.papers.pending.is_some(), "the chosen paper waits for the stroke");
    assert!(s.paper_image(paper).is_none());
    s.pen(event(&s, 2, PenPhase::Up, 0.5));
    settle(&mut s);
    assert!(s.papers.pending.is_none());
    assert!(s.paper_image(paper).is_some(), "the chosen paper applies once the stroke ends");
}

#[test]
fn a_paste_lands_before_the_undo_queued_behind_its_request() {
    let mut s = session(Platform::Gtk);
    s.renderer_mut().tiled_sources = true;
    s.frame(1, 1).unwrap();
    let checkpoint = s.engine.checkpoint();
    invoke(&mut s, CommandId::PasteImage);
    let id = s.state.requests.iter().find_map(|request| matches!(request.kind, HostRequestKind::Document { .. }).then_some(request.id)).unwrap();
    invoke(&mut s, CommandId::Undo);
    assert_eq!(s.deferred_edits.len(), 1, "Undo waits behind the Paste request");
    let source = Arc::new(Arc::unwrap_or_clone(layer_core::color::source::rgba8_source([8, 8], |_, _| [90, 120, 200, 255])));
    let clip = PixelClip { nonce: "clip".into(), name: "Ink".into(), source, policy: layer_core::PaintBasePolicy::WorkingPixels,
        origin: [0, 0], color: s.engine.document().composition().color, blend: layer_core::BlendSpace::Linear,
        png: Arc::from(&b"png"[..]), layers: None };
    s.paste_clip(&clip, PasteMode::Paste).unwrap();
    s.complete_document_request(id, Ok(true)).unwrap();
    assert_ne!(s.engine.checkpoint(), checkpoint);
    settle(&mut s);
    assert!(s.deferred_edits.is_empty());
    assert_eq!(s.engine.checkpoint(), checkpoint, "the queued Undo removes the paste that preceded it");
    s.suspend_renderer().unwrap();
    assert!(s.paste_clip(&clip, PasteMode::Paste).is_err(), "a completion cannot leave raster work without a renderer");
}
