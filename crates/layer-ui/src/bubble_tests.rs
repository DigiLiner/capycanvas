use super::*;
use layer_core::DefaultBrushPreset;

const AREA: Bounds = Bounds { x: 6., y: 48., width: 988., height: 746. };

fn layout(centers: &[[f32; 2]]) -> BubbleLayout {
    BubbleLayout {
        bubbles: centers
            .iter()
            .enumerate()
            .map(|(i, &center)| ToolBubble {
                id: i as u32 + 1,
                source: BubbleSource::Medium { group: ToolGroup::ALL[i] },
                center,
                options: BubbleOptions::default(),
            })
            .collect(),
        rings: Vec::new(),
    }
}

fn desk(centers: &[[f32; 2]]) -> Desk {
    let mut desk = Desk::default();
    desk.sync(&layout(centers), AREA);
    desk
}

fn run(desk: &mut Desk, from: u64, frames: u64, frame_ns: u64) -> u64 {
    let mut now = from;
    for _ in 0..frames {
        now += frame_ns;
        desk.advance(now);
    }
    now
}

fn apart(desk: &Desk) -> bool {
    let p = desk.placements();
    p.iter().enumerate().all(|(i, a)| {
        p[i + 1..].iter().all(|b| {
            (a.center[0] - b.center[0]).hypot(a.center[1] - b.center[1]) >= TOOL_BUBBLE + WORKSPACE_SPACING - 0.05
        })
    })
}

#[test]
fn halo_arcs_split_settings_and_grow_upward_or_rightward() {
    for count in 1..=MAX_RING_SETTINGS {
        let arcs = halo_arcs(count, TOOL_BUBBLE);
        assert_eq!(arcs.len(), count);
        for (i, arc) in arcs.iter().enumerate() {
            assert!(arc.radius - arc.width * 0.5 > TOOL_BUBBLE * 0.5, "{count}: track clears the face");
            for f in [0., 0.25, 0.5, 0.75, 1.] {
                assert!((arc.fraction(arc.point(f)) - f).abs() < 1e-3, "{count}/{i}: {f}");
            }
            let middle = arc.point(0.5);
            assert!(arc.contains(middle));
            assert!(arcs.iter().enumerate().all(|(j, other)| j == i || !other.contains(middle)), "{count}/{i} overlaps");
            let [start, end] = [arc.point(0.), arc.point(1.)];
            if arc.sweep.abs() > 200. {
                assert!(start[0] < 0. && end[0] > 0. && arc.point(0.5)[1] < 0., "the gauge passes over the top");
            } else if middle[1] < -arc.radius * 0.7 {
                assert!(end[0] > start[0], "{count}/{i}: top arcs grow rightward");
            } else {
                assert!(end[1] < start[1], "{count}/{i}: side arcs grow upward");
            }
        }
        let outer = arcs.iter().map(|a| a.radius).fold(0., f32::max);
        assert_eq!(outer > arcs[0].radius, count > 3, "{count}: a second ring only above three");
        assert!((halo_reach(count, TOOL_BUBBLE) - outer - RING_WIDTH * 0.5).abs() < 1e-4);
    }
}

#[test]
fn slider_ring_runs_from_lower_left_over_the_top_and_clamps_past_its_ends() {
    let ring = slider_ring(SLIDER_BUBBLE);
    let [start, top, end] = [ring.point(0.), ring.point(0.5), ring.point(1.)];
    assert!(start[0] < 0. && start[1] > 0.);
    assert!(top[0].abs() < 1e-3 && top[1] < 0.);
    assert!(end[0] > 0. && end[1] > 0.);
    assert_eq!(ring.fraction([-5., 40.]), 0.);
    assert_eq!(ring.fraction([5., 40.]), 1.);
    assert!(ring.radius + ring.width * 0.5 <= SLIDER_BUBBLE * 0.5);
    assert!(!ring.contains([0., 0.]), "the center moves the bubble");
}

#[test]
fn a_dragged_bubble_pushes_neighbours_which_slide_and_keep_their_new_place() {
    let mut desk = desk(&[[300., 300.], [400., 300.]]);
    desk.grab(1);
    desk.drag([360., 300.]);
    assert!(apart(&desk));
    assert!(desk.center(2).unwrap()[0] > 400.);
    let settled = desk.settled();
    desk.release();
    run(&mut desk, 0, 600, 16_666_667);
    assert!(!desk.moving());
    for (id, rest) in settled {
        let live = desk.center(id).unwrap();
        assert!((live[0] - rest[0]).abs() < 0.01 && (live[1] - rest[1]).abs() < 0.01, "{id}: {live:?} {rest:?}");
    }
    assert!(desk.center(2).unwrap()[0] > 360. + TOOL_BUBBLE + WORKSPACE_SPACING, "pushes keep their place");
}

#[test]
fn growth_pushes_are_temporary() {
    let mut desk = desk(&[[300., 300.], [370., 300.]]);
    desk.grow(Some((1, halo_reach(2, TOOL_BUBBLE))));
    run(&mut desk, 0, 120, 16_666_667);
    assert!(desk.center(2).unwrap()[0] > 300. + halo_reach(2, TOOL_BUBBLE));
    desk.grow(None);
    run(&mut desk, 2_000_000_000, 240, 16_666_667);
    assert_eq!(desk.center(2), Some([370., 300.]));
    assert!(!desk.moving());
}

#[test]
fn walls_keep_bubbles_inside_and_pushes_slide_along_them() {
    let mut desk = desk(&[[200., 200.], [200., AREA.y + 28.]]);
    desk.grab(1);
    for y in (100..=200).rev().step_by(4) {
        desk.drag([230., y as f32]);
    }
    let pushed = desk.center(2).unwrap();
    assert!(pushed[1] >= AREA.y + 28. - 1e-3);
    assert!(pushed[0] < 200. - 20., "slides along the top wall, away from the dragged bubble");
    desk.drag([-500., 2000.]);
    let corner = desk.center(1).unwrap();
    assert_eq!(corner, [AREA.x + 28., AREA.y + AREA.height - 28.]);
    let mut tiny = Desk::default();
    tiny.sync(&layout(&[[0., 0.]]), Bounds { x: 6., y: 48., width: 20., height: 20. });
    assert_eq!(tiny.center(1), Some([16., 58.]), "centred when the area is smaller than the bubble");
}

#[test]
fn motion_follows_elapsed_time_at_any_frame_rate() {
    let mut slow = desk(&[[300., 300.], [400., 300.]]);
    let mut fast = slow.clone();
    for desk in [&mut slow, &mut fast] {
        desk.grab(1);
        desk.drag([370., 300.]);
        desk.release();
    }
    run(&mut slow, 0, 30, 16_666_667);
    run(&mut fast, 0, 60, 8_333_334);
    let [a, b] = [slow.center(2).unwrap(), fast.center(2).unwrap()];
    assert!((a[0] - b[0]).abs() < 0.5, "{a:?} {b:?}");
}

#[test]
fn sync_follows_layout_changes_without_moving_kicked_or_held_bubbles() {
    let mut desk = desk(&[[300., 300.], [500., 500.]]);
    let mut changed = layout(&[[300., 300.], [600., 520.]]);
    assert!(!desk.matches(&changed, AREA));
    desk.sync(&changed, AREA);
    run(&mut desk, 0, 300, 16_666_667);
    assert_eq!(desk.center(2), Some([600., 520.]), "undo glides to the restored place");
    changed.bubbles.remove(0);
    desk.grab(1);
    desk.sync(&changed, AREA);
    assert!(desk.placements().iter().all(|p| !p.lifted), "a removed bubble cannot stay held");
    assert!(desk.matches(&changed, AREA));
}

#[test]
fn bubble_layout_validation_rejects_duplicates_and_bad_values() {
    let mut ids = std::collections::BTreeSet::new();
    layout(&[[1., 2.], [3., 4.]]).validate(&mut ids).unwrap();
    let mut twice = layout(&[[1., 2.], [3., 4.]]);
    twice.bubbles[1].source = twice.bubbles[0].source;
    assert!(twice.validate(&mut Default::default()).is_err());
    let mut size = layout(&[[1., 2.]]);
    size.bubbles[0].options.size = 400.;
    assert!(size.validate(&mut Default::default()).is_err());
    let mut point = layout(&[[f32::NAN, 2.]]);
    assert!(point.validate(&mut Default::default()).is_err());
    point.bubbles[0].center = [1., 2.];
    point.bubbles[0].source = BubbleSource::Control { control: ToolbarControl::Opacity };
    assert!(point.validate(&mut Default::default()).is_err(), "Opacity is stored as the opacity slider");
    let mut rings = BubbleLayout::default();
    rings.set_ring(DefaultBrushPreset::GPen as u32, vec!["hardness".into(), "hardness".into()]);
    assert!(rings.validate(&mut Default::default()).is_err());
    rings.set_ring(DefaultBrushPreset::GPen as u32, (0..7).map(|_| "size".to_string()).collect());
    assert!(rings.validate(&mut Default::default()).is_err());
    rings.set_ring(DefaultBrushPreset::GPen as u32, vec!["hardness".into(), "unknown".into()]);
    assert!(rings.validate(&mut Default::default()).is_err());
    let mut taken = std::collections::BTreeSet::from([1]);
    assert!(layout(&[[1., 2.]]).validate(&mut taken).is_err(), "bubble IDs share the workspace ID space");
    let json = serde_json::to_string(&layout(&[[1., 2.]])).unwrap();
    assert!(!json.contains("options"), "default options are omitted");
    assert_eq!(serde_json::from_str::<BubbleLayout>(&json).unwrap(), layout(&[[1., 2.]]));
}

#[test]
fn sources_normalize_toolbar_controls() {
    use ToolbarControl as C;
    assert_eq!(BubbleSource::for_control(C::Opacity), Some(BubbleSource::Control { control: C::BrushOpacitySlider }));
    assert_eq!(BubbleSource::for_control(C::Divider), None);
    assert_eq!(BubbleSource::for_control(C::Command { command: CommandId::Undo }), None);
    assert_eq!(BubbleSource::for_control(C::Panel { panel: Panel::Color }), None);
    assert_eq!(BubbleSource::for_control(C::ToolSlot { slot: ToolSlotId::Drawing }).map(BubbleSource::kind), Some(BubbleKind::Dial));
    assert_eq!(BubbleSource::for_control(C::Command { command: CommandId::Eraser }).map(BubbleSource::kind), Some(BubbleKind::Dial));
    assert_eq!(BubbleSource::for_control(C::Panel { panel: Panel::Layers }).map(BubbleSource::kind), Some(BubbleKind::Layers));
    assert_eq!(BubbleSource::for_variant(ToolVariant::BrushGroup { group: ToolGroup::Oil }).map(BubbleSource::kind), Some(BubbleKind::Medium));
    assert_eq!(BubbleSource::for_variant(ToolVariant::Command { command: CommandId::Lasso }), None);
}
