use super::*;
use layer_core::DefaultBrushPreset;

const AREA: Bounds = Bounds { x: 14., y: 56., width: 1172., height: 830. };
const VIEWPORT: [f32; 2] = [1200., 900.];

fn desk(centers: &[[f32; 2]]) -> Desk {
    let items: Vec<_> = centers.iter().enumerate().map(|(i, &c)| (i as u32 + 1, c, Shape::circle(TOOL_BUBBLE * 0.5))).collect();
    let mut desk = Desk::default();
    desk.sync(&items, AREA);
    desk
}

fn run(desk: &mut Desk, from: u64, frames: u64) -> u64 {
    let mut now = from;
    for _ in 0..frames {
        now += 16_666_667;
        desk.advance(now);
    }
    now
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn angle(p: [f32; 2]) -> f32 {
    degrees(p)
}

#[test]
fn halo_arcs_sit_on_glass_rings_with_their_icons_at_the_start() {
    for count in 1..=MAX_RING_SETTINGS {
        let arcs = halo_arcs(count);
        assert_eq!(arcs.len(), count);
        assert_eq!(halo_disc(count), if count > 3 { 188. } else { 122. });
        for (i, arc) in arcs.iter().enumerate() {
            assert!(arc.radius + arc.band * 0.5 <= halo_disc(count) * 0.5 + 1e-3, "{count}/{i} fits the glass");
            assert!(arc.radius - arc.band * 0.5 > TOOL_BUBBLE * 0.5, "{count}/{i} clears the face");
            for f in [0., 0.25, 0.5, 0.75, 1.] {
                assert!((arc.fraction(arc.point(f)) - f).abs() < 1e-3, "{count}/{i}: {f}");
            }
            assert_eq!(ring_at(&arcs, arc.point(0.5)), Some(i), "{count}/{i}");
            assert!((distance(arc.icon_point(), arc.point(0.)) - 13.).abs() < 0.3, "the icon sits just before its track");
            let [start, middle, end] = [arc.point(0.), arc.point(0.5), arc.point(1.)];
            if arc.sweep.abs() > 200. {
                assert!(start[0] < 0. && end[0] > 0. && middle[1] < 0., "one setting is a gauge over the top");
            } else if middle[0].abs() < arc.radius * 0.5 {
                assert!(end[0] > start[0], "{count}/{i}: top and bottom arcs grow rightward");
            } else {
                assert!(end[1] < start[1], "{count}/{i}: side arcs grow upward");
            }
            for (j, other) in arcs.iter().enumerate().filter(|(j, _)| *j != i) {
                assert!(distance(arc.icon_point(), other.icon_point()) >= 16., "{count}: icons {i} and {j} overlap");
            }
        }
    }
    assert_eq!(halo_arcs(4)[..2], halo_arcs(2)[..], "four settings are the pair twice");
    assert_eq!(halo_arcs(6)[..3], halo_arcs(3)[..]);
    assert_eq!(ring_at(&halo_arcs(4), [0., -63.5]), None, "between the rings is glass");
    assert_eq!(ring_at(&halo_arcs(2), [0., 0.]), None);
}

#[test]
fn slider_ring_runs_from_lower_left_over_the_top_around_a_moving_center() {
    let ring = slider_ring();
    let [start, top, end] = [ring.point(0.), ring.point(0.5), ring.point(1.)];
    assert!(start[0] < 0. && start[1] > 0.);
    assert!(top[0].abs() < 1e-3 && top[1] < 0.);
    assert!(end[0] > 0. && end[1] > 0.);
    assert_eq!(ring.fraction([-5., 40.]), 0.);
    assert_eq!(ring.fraction([5., 40.]), 1.);
    assert_eq!(ring_at(&[ring], [0., -ring.radius]), Some(0));
    assert_eq!(ring_at(&[ring], [0., -SLIDER_CENTER * 0.5 + 1.]), None, "the center moves the bubble");
    assert!(ring.radius + RING_TRACK <= SLIDER_BUBBLE * 0.5);
}

#[test]
fn color_beads_orbit_the_plate_with_recents_on_top_and_the_palette_below() {
    let options = BubbleOptions::default();
    let size = COLOR_RING_SIZES[1];
    let radius = size * 0.5;
    let beads = color_beads(size, options, 20, 30, [0.; 2]);
    let of = |beads: &[BeadPlacement], bead: Bead| beads.iter().find(|b| b.bead == bead).map(|b| b.center).unwrap();
    for bead in &beads {
        assert!(distance(bead.center, [0.; 2]) - bead.size * 0.5 >= radius + 4., "{:?} stays off the plate", bead.bead);
    }
    let recent: Vec<_> = beads.iter().filter(|b| matches!(b.bead, Bead::Recent { .. }) && b.alpha == 1.).collect();
    assert_eq!(recent.len(), strip_columns(BeadStrip::Recent, size));
    assert_eq!(strip_columns(BeadStrip::Recent, COLOR_RING_SIZES[2]), RECENT_BEADS);
    assert!(recent.iter().all(|b| (angle(b.center) - 270.).abs() <= 60.), "recents run along the top");
    assert!(recent.windows(2).all(|w| w[0].center[0] < w[1].center[0]), "the newest is first, on the left");
    assert!(beads.iter().any(|b| matches!(b.bead, Bead::Recent { .. }) && b.alpha < 1.), "a faint end shows there are more");
    let swatches: Vec<_> = beads.iter().filter(|b| matches!(b.bead, Bead::Swatch { .. })).collect();
    assert!(swatches.iter().all(|b| b.center[1] > 0.), "the palette wraps the lower half");
    let [a, b] = [of(&beads, Bead::Swatch { index: 0 }), of(&beads, Bead::Swatch { index: 1 })];
    assert!((angle(a) - angle(b)).abs() < 1e-3, "a column shares an angle");
    assert!((distance(b, [0.; 2]) - distance(a, [0.; 2]) - BEAD_STEP).abs() < 1e-3, "two orbits 28 px apart");
    assert!(a[0] < 0., "the first column is on the left");
    let palette = of(&beads, Bead::Palettes);
    let foreground = of(&beads, Bead::Foreground);
    assert!(palette[0] > 0. && palette[1] > 0., "the palette bead ends the rows on the right");
    assert!((angle(foreground) - 174.).abs() < 1e-3 && (angle(of(&beads, Bead::Transparent)) - 10.).abs() < 0.1);
    let plain = color_beads(size, BubbleOptions { palette: false, ..options }, 20, 30, [0.; 2]);
    assert!((angle(of(&plain, Bead::Foreground)) - 140.).abs() < 1e-3 && (angle(of(&plain, Bead::Transparent)) - 45.).abs() < 1e-3);
    assert!(!plain.iter().any(|b| matches!(b.bead, Bead::Palettes | Bead::Swatch { .. })));
    let turned = color_beads(size, options, 20, 30, [0., 2.]);
    assert!(distance(of(&turned, Bead::Swatch { index: 4 }), a) < 1e-3, "turning two columns brings column two to the start");
    assert_eq!(of(&turned, Bead::Palettes), palette, "the palette bead stays put");
    assert!(beads.iter().any(|b| matches!(b.bead, Bead::Swatch { .. }) && b.alpha < 0.5), "the next column peeks out");
    assert_eq!(bead_at(&beads, palette), Some(Bead::Palettes));
    assert_eq!(bead_at(&beads, foreground), Some(Bead::Foreground), "the foreground lies over the background");
    assert_eq!(bead_at(&beads, recent[2].center), Some(Bead::Recent { index: 2 }));
    assert_eq!(bead_at(&beads, [0., 0.]), None);
    assert_eq!(Bead::Swatch { index: 3 }.strip(), Some(BeadStrip::Palette));
    assert!(on_color_rim(size, options, [radius - 3., 0.]), "the plate's rim moves the ring");
    assert!(on_color_rim(size, options, [radius + 10., 0.]), "so does the glass between the plate and its beads");
    assert!(!on_color_rim(size, options, [radius - RING_RIM - 2., 0.]) && !on_color_rim(size, options, [color_reach(size, options) + 2., 0.]));
    let pill = palette_pill(size);
    assert_eq!([pill.width, pill.height], PALETTE_PILL);
    assert!(pill.y > distance(b, [0.; 2]) + BUBBLE_BEAD * 0.5, "the name sits below the rows");
}

#[test]
fn color_beads_never_overlap_at_any_size() {
    let combos = [(true, true, true), (true, false, true), (false, true, true), (true, true, false), (false, false, true)];
    for size in [128., 136., 176., 216., 256.] {
        for (recent, palette, pair) in combos {
            let options = BubbleOptions { size, recent, palette, pair, ..BubbleOptions::default() };
            let beads: Vec<_> = color_beads(size, options, 64, 48, [0.; 2]).into_iter().filter(|b| b.alpha == 1.).collect();
            for (i, a) in beads.iter().enumerate() {
                for b in &beads[i + 1..] {
                    if matches!((a.bead, b.bead), (Bead::Background, Bead::Foreground)) {
                        continue;
                    }
                    assert!(
                        distance(a.center, b.center) >= (a.size + b.size) * 0.5 - 0.5,
                        "{size} {options:?}: {:?} and {:?} overlap",
                        a.bead,
                        b.bead
                    );
                }
            }
            assert!(beads.iter().all(|b| distance(b.center, [0.; 2]) + b.size * 0.5 <= color_reach(size, options) + 1e-3));
        }
    }
}

#[test]
fn bead_slides_follow_the_finger_around_the_ring() {
    let size = COLOR_RING_SIZES[1];
    assert!(bead_travel(BeadStrip::Recent, size, [-30., -120.], [30., -120.]) < 0., "dragging right brings newer colors back");
    assert!(bead_travel(BeadStrip::Palette, size, [-60., 100.], [60., 100.]) < 0., "dragging right brings earlier columns back");
    assert!(bead_travel(BeadStrip::Palette, size, [60., 100.], [-60., 100.]) > 0.);
    let one = bead_travel(BeadStrip::Recent, size, polar(orbit(size, 0), 270.), polar(orbit(size, 0), 270. + bead_degrees(size)));
    assert!((one + 1.).abs() < 1e-3, "one bead step moves one bead");
    assert_eq!(bead_scroll_limit(BeadStrip::Palette, size, 7), 0.);
    assert_eq!(bead_scroll_limit(BeadStrip::Recent, size, 20), 20. - strip_columns(BeadStrip::Recent, size) as f32);
}

#[test]
fn the_chooser_rings_the_dial_and_folds_toward_open_space_near_edges() {
    let middle = [600., 450.];
    for count in [3, 8, 12] {
        let ring = chooser_ring(count, middle, AREA);
        assert_eq!(ring.len(), count);
        assert!(ring.iter().all(|p| distance(*p, [0.; 2]) >= CHOOSER_RING - 1e-3));
        assert!(ring[0][1] < 0. && ring[0][0].abs() < 1e-3, "the first item is at the top");
        assert!(ring.windows(2).all(|w| distance(w[0], w[1]) >= CHOOSER_ITEM), "{count} items don't overlap");
    }
    for at in [[AREA.x + 40., AREA.y + 40.], [AREA.x + AREA.width - 40., 450.], [600., AREA.y + AREA.height - 30.]] {
        for count in [3, 6, 9, 12] {
            let folded = chooser_ring(count, at, AREA);
            assert_eq!(folded.len(), count);
            for (i, p) in folded.iter().enumerate() {
                let point = [at[0] + p[0], at[1] + p[1]];
                assert!(fits(AREA, point, CHOOSER_ITEM * 0.5), "{at:?}/{count}: {point:?} stays in the window");
                assert!(distance(*p, [0.; 2]) >= CHOOSER_RING - 1e-3, "{at:?}/{count}: clear of the dial");
                for q in &folded[i + 1..] {
                    assert!(distance(*p, *q) >= CHOOSER_ITEM, "{at:?}/{count} items don't overlap");
                }
            }
        }
    }
}

#[test]
fn brushes_curve_beside_their_medium_and_stay_on_screen() {
    let middle = [600., 450.];
    let clear = |column: &[[f32; 2]], reach: f32| {
        column.iter().all(|p| {
            let gap = [(p[0].abs() - BRUSH_PILL[0] * 0.5).max(0.), (p[1].abs() - BRUSH_PILL[1] * 0.5).max(0.)];
            gap[0].hypot(gap[1]) >= reach + CHOOSER_ITEM * 0.5
        })
    };
    let column = chooser_column(9, middle, Some(([CHOOSER_RING, 0.], CHOOSER_RING)), AREA);
    assert!(clear(&column, CHOOSER_RING), "brushes clear the media ring");
    assert!(clear(&chooser_column(4, middle, Some(([CHOOSER_RING, 0.], 134.)), AREA), 134.), "and a second arc of media");
    assert!(column.windows(2).all(|w| w[1][1] - w[0][1] >= BRUSH_PILL[1]), "pills stack without overlapping");
    assert!(column[0][0] < column[4][0] && column[8][0] < column[4][0], "the column curves around the dial");
    assert!((column[4][1]).abs() < 1e-3, "the column centers on its medium");
    let left = chooser_column(4, middle, Some(([-60., -60.], CHOOSER_RING)), AREA);
    assert!(left.iter().all(|p| p[0] <= -72.), "a medium on the left opens its brushes on the left");
    let low = [600., AREA.y + AREA.height - 40.];
    let clamped = chooser_column(9, low, Some(([CHOOSER_RING, 0.], CHOOSER_RING)), AREA);
    assert!(clamped.iter().all(|p| low[1] + p[1] + BRUSH_PILL[1] * 0.5 <= AREA.y + AREA.height), "the column stays above the bottom edge");
    let fan = chooser_column(5, [AREA.x + AREA.width - 60., 450.], None, AREA);
    assert!(fan.iter().all(|p| p[0] < 0.), "a medium bubble near the right edge fans to the left");
    let fan = chooser_column(5, [200., 450.], None, AREA);
    assert!(fan.iter().all(|p| p[0] > 0.));
}

#[test]
fn the_layers_card_grows_to_its_rows_and_mode() {
    assert_eq!(layers_card(false, 3)[0], 244.);
    assert_eq!(layers_card(true, 3)[0], 300.);
    assert_eq!(layers_card(false, 1), layers_card(false, 3), "room for the properties sheet");
    assert!(layers_card(false, 4)[1] < layers_card(false, 5)[1]);
    assert_eq!(layers_card(false, 5), layers_card(false, 20), "more than five rows scroll");
    assert_eq!(layers_card(true, 5)[1] - layers_card(false, 5)[1], CARD_HEADER + 5. * (CARD_ROWS[1] - CARD_ROWS[0]));
}

#[test]
fn a_dragged_bubble_pushes_neighbours_which_keep_their_new_place() {
    let mut desk = desk(&[[300., 300.], [400., 300.]]);
    desk.grab(1);
    desk.drag([360., 300.]);
    assert!(distance(desk.center(1).unwrap(), desk.center(2).unwrap()) >= TOOL_BUBBLE + WORKSPACE_SPACING - 0.05);
    let settled = desk.settled();
    desk.release();
    run(&mut desk, 0, 600);
    assert!(!desk.moving());
    for (id, rest) in settled {
        assert!(distance(desk.center(id).unwrap(), rest) < 0.05, "{id}");
    }
    assert!(desk.center(2).unwrap()[0] > 360. + TOOL_BUBBLE + WORKSPACE_SPACING - 0.1);
}

#[test]
fn letting_go_glides_a_short_way_unless_motion_is_reduced() {
    let glide = |still: bool| {
        let mut desk = desk(&[[300., 400.]]);
        desk.still(still);
        desk.grab(1);
        let mut now = 0;
        for step in 0..12 {
            desk.drag([300. + step as f32 * 30., 400.]);
            now += 16_666_667;
            desk.advance(now);
        }
        let released = desk.center(1).unwrap();
        let settled = desk.settled();
        desk.release();
        run(&mut desk, now, 600);
        assert!(distance(desk.center(1).unwrap(), settled[0].1) < 0.05);
        desk.center(1).unwrap()[0] - released[0]
    };
    let far = glide(false);
    assert!(far > 20. && far <= 135., "{far}");
    assert_eq!(glide(true), 0., "reduced motion has no glide");
}

#[test]
fn motion_follows_elapsed_time_at_any_frame_rate() {
    let at = |frame_ns: u64| {
        let mut desk = desk(&[[300., 300.], [360., 300.]]);
        desk.grab(1);
        desk.drag([330., 300.]);
        desk.release();
        let mut now = 0;
        while now < 500_000_000 {
            now += frame_ns;
            desk.advance(now);
        }
        desk.center(2).unwrap()
    };
    assert!(distance(at(16_666_667), at(8_333_333)) < 0.5);
}

#[test]
fn growth_pushes_are_temporary_and_cards_push_as_cards() {
    let mut desk = desk(&[[300., 300.], [370., 300.]]);
    desk.grow(vec![(1, vec![Shape::circle(halo_disc(2) * 0.5)])]);
    run(&mut desk, 0, 120);
    assert!(desk.center(2).unwrap()[0] >= 300. + halo_disc(2) * 0.5 + WORKSPACE_SPACING + TOOL_BUBBLE * 0.5 - 0.1);
    desk.grow(Vec::new());
    run(&mut desk, 2_000_000_000, 240);
    assert_eq!(desk.center(2), Some([370., 300.]));
    let card = layers_card(false, 5);
    let mut wide = self::desk(&[[300., 300.], [300. + card[0] * 0.5, 300. + card[1] * 0.5]]);
    wide.grow(vec![(1, vec![Shape::rect(card)])]);
    run(&mut wide, 0, 120);
    let pushed = wide.center(2).unwrap();
    let clear = |axis: usize| (pushed[axis] - 300.).abs() - TOOL_BUBBLE * 0.5 >= card[axis] * 0.5 + WORKSPACE_SPACING - 0.5;
    assert!(clear(0) || clear(1), "the card's corner pushes like a card: {pushed:?}");
}

#[test]
fn grown_parts_push_only_where_they_are() {
    let mut desk = desk(&[[300., 300.], [300., 380.], [420., 300.]]);
    desk.grow(vec![(1, vec![Shape::circle(28.), Shape::rect(BRUSH_PILL).at([120., 0.])])]);
    run(&mut desk, 0, 120);
    assert_eq!(desk.center(2), Some([300., 380.]), "nothing grew toward the bubble below");
    let pushed = desk.center(3).unwrap();
    assert!(distance(pushed, [420., 300.]) > 20., "the pill pushed its neighbour: {pushed:?}");
    let mut edge = self::desk(&[[AREA.x + 40., 300.]]);
    edge.grow(vec![(1, vec![Shape::circle(28.), Shape::circle(22.).at([-60., 0.])])]);
    run(&mut edge, 0, 60);
    assert!(edge.center(1).unwrap()[0] >= AREA.x + 82. - 1e-3, "walls hold every part inside");
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
    assert_eq!(desk.center(1).unwrap(), [AREA.x + 28., AREA.y + AREA.height - 28.]);
    desk.release();
    let mut tiny = Desk::default();
    tiny.sync(&[(1, [0., 0.], Shape::circle(28.))], Bounds { x: 6., y: 48., width: 20., height: 20. });
    assert_eq!(tiny.center(1), Some([16., 58.]), "centred when the area is smaller than the bubble");
}

#[test]
fn strokes_fade_nearby_bubbles_until_the_next_stroke() {
    let mut desk = desk(&[[300., 300.], [700., 300.]]);
    assert!(!desk.fade(Some([300. + 28. + STROKE_FADE + 5., 300.])));
    assert!(desk.fade(Some([300. + 28. + STROKE_FADE - 5., 300.])));
    assert_eq!(desk.faded(), [1]);
    desk.fade(Some([700., 300.]));
    assert_eq!(desk.faded(), [1, 2]);
    assert!(desk.fade(None));
    assert!(desk.faded().is_empty());
}

#[test]
fn bubbles_keep_their_distance_to_the_nearest_corner() {
    let mut bubble = ToolBubble::new(1, BubbleSource::Medium { group: ToolGroup::Pen }, [1100., 820.], VIEWPORT);
    assert_eq!(bubble.corner, Corner::BottomRight);
    assert_eq!(bubble.center(VIEWPORT), [1100., 820.]);
    assert_eq!(bubble.center([800., 600.]), [700., 520.]);
    bubble.place([100., 120.], VIEWPORT);
    assert_eq!(bubble.corner, Corner::TopLeft);
    assert_eq!(bubble.center([800., 600.]), [100., 120.]);
}

#[test]
fn bubble_layout_validation_rejects_duplicates_and_bad_values() {
    let layout = |n: usize| BubbleLayout {
        bubbles: (0..n).map(|i| ToolBubble::new(i as u32 + 1, BubbleSource::Medium { group: ToolGroup::ALL[i] }, [100., 100.], VIEWPORT)).collect(),
        rings: Vec::new(),
    };
    layout(2).validate(&mut Default::default()).unwrap();
    let mut twice = layout(2);
    twice.bubbles[1].source = twice.bubbles[0].source;
    assert!(twice.validate(&mut Default::default()).is_err());
    let mut size = layout(1);
    size.bubbles[0].options.size = 400.;
    assert!(size.validate(&mut Default::default()).is_err());
    let mut point = layout(1);
    point.bubbles[0].offset = [f32::NAN, 2.];
    assert!(point.validate(&mut Default::default()).is_err());
    point.bubbles[0].offset = [1., 2.];
    point.bubbles[0].source = BubbleSource::Control { control: ToolbarControl::Opacity };
    assert!(point.validate(&mut Default::default()).is_err(), "Opacity is stored as the opacity slider");
    let mut rings = BubbleLayout::default();
    rings.set_ring(DefaultBrushPreset::GPen as u32, vec!["hardness".into(), "hardness".into()]);
    assert!(rings.validate(&mut Default::default()).is_err());
    rings.set_ring(DefaultBrushPreset::GPen as u32, vec!["hardness".into(), "unknown".into()]);
    assert!(rings.validate(&mut Default::default()).is_err());
    let mut taken = std::collections::BTreeSet::from([1]);
    assert!(layout(1).validate(&mut taken).is_err(), "bubble IDs share the workspace ID space");
    let json = serde_json::to_string(&layout(1)).unwrap();
    assert!(!json.contains("options") && json.contains("top_left"));
    assert_eq!(serde_json::from_str::<BubbleLayout>(&json).unwrap(), layout(1));
}

#[test]
fn sources_normalize_toolbar_controls() {
    use ToolbarControl as C;
    assert_eq!(BubbleSource::for_control(C::Opacity), Some(BubbleSource::Control { control: C::BrushOpacitySlider }));
    assert_eq!(BubbleSource::for_control(C::Divider), None);
    assert_eq!(BubbleSource::for_control(C::Command { command: CommandId::Undo }), None);
    assert_eq!(BubbleSource::for_control(C::Panel { panel: Panel::Color }), None);
    assert_eq!(BubbleSource::for_control(C::ToolSlot { slot: ToolSlotId::Drawing }).map(BubbleSource::kind), Some(BubbleKind::Dial));
    assert_eq!(BubbleSource::for_control(C::Panel { panel: Panel::Layers }).map(BubbleSource::kind), Some(BubbleKind::Layers));
    assert_eq!(BubbleSource::for_variant(ToolVariant::BrushGroup { group: ToolGroup::Oil }).map(BubbleSource::kind), Some(BubbleKind::Medium));
    assert_eq!(BubbleSource::for_variant(ToolVariant::Command { command: CommandId::Lasso }), None);
    assert!(!BubbleSource::tears_without_hold(C::BrushSizeSlider), "plain drags keep editing sliders");
    assert!(BubbleSource::tears_without_hold(C::Opacity));
}
