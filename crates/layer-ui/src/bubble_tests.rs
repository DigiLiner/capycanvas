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
        assert_eq!(halo_disc(count), if count > 3 { 206. } else { 136. });
        for (i, arc) in arcs.iter().enumerate() {
            assert!(arc.radius + arc.band * 0.5 <= halo_disc(count) * 0.5 + 1e-3, "{count}/{i} fits the glass");
            assert!(arc.radius - arc.band * 0.5 >= TOOL_BUBBLE * 0.5 + 8., "{count}/{i} clears the face with room to spare");
            for f in [0., 0.25, 0.5, 0.75, 1.] {
                assert!((arc.fraction(arc.point(f)) - f).abs() < 1e-3, "{count}/{i}: {f}");
            }
            assert_eq!(ring_at(&arcs, arc.point(0.5)), Some(i), "{count}/{i}");
            assert!((distance(arc.icon_point(), arc.point(0.)) - RING_ICON).abs() < 0.5, "the icon sits before its track");
            assert!(RING_ICON - 8. - 7.5 >= 5., "a 16 px icon and the thumb at zero keep a gap");
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
    assert_eq!(ring_at(&halo_arcs(4), [0., -70.5]), None, "between the rings is glass");
    assert_eq!(ring_at(&halo_arcs(2), [0., 0.]), None);
}

#[test]
fn controls_ring_size_left_and_strength_right_around_their_center_and_history_splits_its_capsule() {
    assert_eq!(bubble_face(BubbleKind::Controls, halo_disc(CONTROL_RINGS.len())), [TOOL_BUBBLE; 2]);
    let arcs = halo_arcs(CONTROL_RINGS.len());
    assert!(arcs[0].point(0.5)[0] < 0. && arcs[1].point(0.5)[0] > 0., "size on the left, strength on the right");
    assert!(arcs.iter().all(|arc| arc.point(1.)[1] < arc.point(0.)[1]), "both grow upward");
    assert!(arcs.iter().all(|arc| arc.radius - HALO_BANDS[0] * 0.5 >= TOOL_BUBBLE * 0.5 + 4.), "the arcs clear the center");
    assert_eq!(bubble_face(BubbleKind::History, HISTORY_BUBBLE[0]), HISTORY_BUBBLE);
    assert_eq!(history_button([-20., 0.]), Some(0));
    assert_eq!(history_button([20., 4.]), Some(1));
    assert_eq!(history_button([0., HISTORY_BUBBLE[1]]), None);
}

fn pair() -> PairBeads {
    PairBeads::for_window(1600.)
}

#[test]
fn the_pair_and_transparent_beads_match_the_color_drawer_and_keep_its_gap_from_the_rim() {
    for width in [1600., 1000.] {
        let pair = PairBeads::for_window(width);
        let stage = Panel::Color.default_width(width) - 2. * PANEL_CONTENT_INSET;
        let drawer = ColorPanelLayout::new(stage).unwrap();
        assert_eq!((pair.foreground, pair.background), (drawer.foreground[2], drawer.background[2]), "{width}");
        assert_eq!(drawer.transparent[2], pair.background);
        assert!((8. ..14.).contains(&pair.gap), "{width}: {}", pair.gap);
        for size in COLOR_RING_SIZES {
            let beads = color_beads(size, BubbleOptions { pair: true, ..BubbleOptions::default() }, pair, 0, 0, [0.; 2], Bead::Foreground);
            let gap = |bead: Bead| beads.iter().find(|b| b.bead == bead).map(|b| distance(b.center, [0.; 2]) - b.size * 0.5 - size * 0.5).unwrap();
            assert!((gap(Bead::Transparent) - pair.gap).abs() < 0.01, "{width}/{size}: transparent sits the drawer's gap off the grey rim");
            assert!(gap(Bead::Foreground) >= pair.gap - 0.01 && gap(Bead::Background) >= pair.gap - 0.01);
            assert!(gap(Bead::Foreground).min(gap(Bead::Background)) < pair.gap + 0.5, "{width}/{size}: the pair hugs the same gap");
        }
    }
}

fn everything(size: f32) -> BubbleOptions {
    BubbleOptions { size, recent: true, palette: true, pair: true, ..BubbleOptions::default() }
}

#[test]
fn color_beads_orbit_the_plate_with_recents_on_top_and_the_palette_below() {
    let size = COLOR_RING_SIZES[1];
    let options = everything(size);
    let radius = size * 0.5;
    let beads = color_beads(size, options, pair(), 20, 30, [0.; 2], Bead::Foreground);
    let of = |beads: &[BeadPlacement], bead: Bead| beads.iter().find(|b| b.bead == bead).map(|b| b.center).unwrap();
    for bead in &beads {
        assert!(distance(bead.center, [0.; 2]) - bead.size * 0.5 >= radius + 3., "{:?} stays off the plate", bead.bead);
    }
    let recent: Vec<_> = beads.iter().filter(|b| matches!(b.bead, Bead::Recent { .. }) && b.alpha == 1.).collect();
    assert_eq!(recent.len(), RECENT_BEADS);
    assert!(strip_columns(BeadStrip::Recent, COLOR_RING_SIZES[0]) < RECENT_BEADS, "a small ring shows fewer");
    assert!(recent.iter().all(|b| (angle(b.center) - 270.).abs() <= 60.), "recents run along the top");
    assert!(recent.windows(2).all(|w| w[0].center[0] < w[1].center[0]), "the newest is first, on the left");
    let swatches: Vec<_> = beads.iter().filter(|b| matches!(b.bead, Bead::Swatch { .. })).collect();
    assert!(swatches.iter().all(|b| b.center[1] > 0.), "the palette wraps the lower half");
    let [a, b] = [of(&beads, Bead::Swatch { index: 0 }), of(&beads, Bead::Swatch { index: 1 })];
    assert!((angle(a) - angle(b)).abs() < 1e-3, "a column shares an angle");
    assert!((distance(b, [0.; 2]) - distance(a, [0.; 2]) - BEAD_STEP).abs() < 1e-3, "two orbits 28 px apart");
    assert!(a[0] < 0., "the first column is on the left");
    let palette = of(&beads, Bead::Palettes);
    assert!(palette[0] > 0. && palette[1] > 0., "the palette bead ends the rows on the right");
    let last = swatches.iter().filter(|b| b.alpha == 1.).map(|b| match b.bead {
        Bead::Swatch { index } => index,
        _ => 0,
    }).max().unwrap();
    let [inner, outer] = [of(&beads, Bead::Swatch { index: last - 1 }), of(&beads, Bead::Swatch { index: last })];
    assert!((distance(palette, inner) - distance(palette, outer)).abs() < 0.5, "as far from the inner orbit as the outer");
    assert!(distance(palette, inner) >= (BUBBLE_BEAD + PALETTE_BEAD) * 0.5 + 12., "with room around it");
    let [foreground, background] = [of(&beads, Bead::Foreground), of(&beads, Bead::Background)];
    let offset = [background[0] - foreground[0], background[1] - foreground[1]];
    assert!(offset[0] > 0. && offset[1] > 0., "the background sits lower right, as in the Color panel");
    assert!(distance(foreground, background) < (pair().foreground + pair().background) * 0.5, "the pair overlaps");
    let overlap = [foreground[0] + offset[0] * 0.5, foreground[1] + offset[1] * 0.5];
    assert_eq!(bead_at(&beads, overlap), Some(Bead::Foreground), "the chosen slot lies on top");
    let behind = color_beads(size, options, pair(), 20, 30, [0.; 2], Bead::Background);
    assert_eq!(bead_at(&behind, overlap), Some(Bead::Background));
    let plain = color_beads(size, BubbleOptions { palette: false, ..options }, pair(), 20, 30, [0.; 2], Bead::Foreground);
    assert!(of(&plain, Bead::Foreground)[0] < 0. && of(&plain, Bead::Transparent)[0] > 0.);
    assert!(!plain.iter().any(|b| matches!(b.bead, Bead::Palettes | Bead::Swatch { .. })));
    let turned = color_beads(size, options, pair(), 20, 30, [0., 2.], Bead::Foreground);
    assert!(distance(of(&turned, Bead::Swatch { index: 4 }), a) < 1e-3, "turning two columns brings column two to the start");
    assert_eq!(of(&turned, Bead::Palettes), palette, "the palette bead stays put");
    let half = color_beads(size, options, pair(), 20, 30, [0.5, 0.5], Bead::Foreground);
    assert!(half.iter().any(|b| matches!(b.bead, Bead::Recent { .. }) && b.alpha < 1.), "partly turned ends fade");
    assert_eq!(bead_at(&beads, palette), Some(Bead::Palettes));
    assert_eq!(Bead::Swatch { index: 3 }.strip(), Some(BeadStrip::Palette));
    assert!(on_color_rim(size, options, pair(), [radius - 3., 0.]), "the plate's rim moves the ring");
    assert!(on_color_rim(size, options, pair(), [radius + 10., 0.]), "so does the space out to the beads");
    assert!(!on_color_rim(size, options, pair(), [radius - RING_RIM - 2., 0.]) && !on_color_rim(size, options, pair(), [color_reach(size, options, pair()) + 2., 0.]));
    assert!(!color_drag_out(size, [size * 0.9, 0.]) && color_drag_out(size, [0., size * 1.05]), "two radii out is a drag");
    let pill = palette_pill(size);
    assert_eq!([pill.width, pill.height], PALETTE_PILL);
    assert!(pill.y > distance(b, [0.; 2]) + BUBBLE_BEAD * 0.5, "the name sits below the rows");
}

#[test]
fn color_beads_never_overlap_at_any_size_or_turn() {
    let combos = [(true, true, true), (true, false, true), (false, true, true), (true, true, false), (false, false, true)];
    for (size, fit) in [128., 136., 216., 280., 344., 352.].into_iter().flat_map(|s| [1600., 1000.].map(|w| (s, PairBeads::for_window(w)))) {
        for (recent, palette, pair) in combos {
            let options = BubbleOptions { size, recent, palette, pair, ..BubbleOptions::default() };
            for scroll in [[0., 0.], [0.3, 0.6], [2.5, 1.5], [3., 4.2]] {
                for front in [Bead::Foreground, Bead::Background] {
                    let beads = color_beads(size, options, fit, 64, 48, scroll, front);
                    for (i, a) in beads.iter().enumerate() {
                        for b in &beads[i + 1..] {
                            if matches!((a.bead, b.bead), (Bead::Background, Bead::Foreground) | (Bead::Foreground, Bead::Background)) {
                                continue;
                            }
                            assert!(
                                distance(a.center, b.center) >= (a.size + b.size) * 0.5 - 0.5,
                                "{size} {options:?} {scroll:?}: {:?} and {:?} overlap",
                                a.bead,
                                b.bead
                            );
                        }
                    }
                    assert!(beads.iter().all(|b| distance(b.center, [0.; 2]) + b.size * 0.5 <= color_reach(size, options, fit) + 1e-3));
                }
            }
        }
    }
}

#[test]
fn turning_beads_moves_and_fades_them_continuously() {
    let size = COLOR_RING_SIZES[1];
    let options = everything(size);
    let at = |scroll: f32| color_beads(size, options, pair(), 40, 48, [scroll, scroll], Bead::Foreground);
    let mut previous = at(0.);
    for step in 1..=400 {
        let next = at(step as f32 * 0.01);
        for bead in &next {
            match previous.iter().find(|b| b.bead == bead.bead) {
                Some(before) => {
                    assert!(distance(before.center, bead.center) < 1., "{:?} jumps at {step}", bead.bead);
                    assert!((before.alpha - bead.alpha).abs() < 0.02, "{:?} flickers at {step}", bead.bead);
                }
                None => assert!(bead.alpha < 0.02, "{:?} pops in at {step}", bead.bead),
            }
        }
        for gone in previous.iter().filter(|b| !next.iter().any(|n| n.bead == b.bead)) {
            assert!(gone.alpha < 0.02, "{:?} vanishes at {step}", gone.bead);
        }
        previous = next;
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
        ..BubbleLayout::default()
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
    point.bubbles[0].source = BubbleSource::Control { control: ToolbarControl::Command { command: CommandId::Redo } };
    assert!(point.validate(&mut Default::default()).is_err(), "Redo is stored as the undo and redo bubble");
    let mut rings = BubbleLayout::default();
    rings.set_ring(DefaultBrushPreset::GPen as u32, vec!["hardness".into(), "hardness".into()]);
    assert!(rings.validate(&mut Default::default()).is_err());
    rings.set_ring(DefaultBrushPreset::GPen as u32, vec!["hardness".into(), "unknown".into()]);
    assert!(rings.validate(&mut Default::default()).is_err());
    let mut kept = BubbleLayout::default();
    let options = BubbleOptions { size: 400., ..BubbleOptions::default() };
    kept.kept.push(KeptOptions { source: BubbleSource::Control { control: ToolbarControl::Color }, options });
    assert!(kept.validate(&mut Default::default()).is_err(), "kept sizes stay in range");
    let mut taken = std::collections::BTreeSet::from([1]);
    assert!(layout(1).validate(&mut taken).is_err(), "bubble IDs share the workspace ID space");
    let json = serde_json::to_string(&layout(1)).unwrap();
    assert!(!json.contains("options") && json.contains("top_left"));
    assert_eq!(serde_json::from_str::<BubbleLayout>(&json).unwrap(), layout(1));
}

#[test]
fn sources_normalize_toolbar_controls() {
    use ToolbarControl as C;
    assert_eq!(BubbleSource::for_control(C::Opacity), None);
    assert_eq!(BubbleSource::for_control(C::BrushSizeSlider), None, "sliders stay in their toolbars");
    assert_eq!(BubbleSource::for_control(C::Divider), None);
    let undo = BubbleSource::for_control(C::Command { command: CommandId::Undo });
    assert_eq!(BubbleSource::for_control(C::Command { command: CommandId::Redo }), undo, "one bubble for undo and redo");
    assert_eq!(undo.map(BubbleSource::kind), Some(BubbleKind::History));
    assert_eq!(BubbleSource::for_control(C::ColorPicker).map(BubbleSource::kind), Some(BubbleKind::Controls));
    assert_eq!(BubbleSource::for_control(C::Panel { panel: Panel::Color }), None);
    assert_eq!(BubbleSource::for_control(C::ToolSlot { slot: ToolSlotId::Drawing }).map(BubbleSource::kind), Some(BubbleKind::Dial));
    assert_eq!(BubbleSource::for_control(C::Panel { panel: Panel::Layers }).map(BubbleSource::kind), Some(BubbleKind::Layers));
    assert_eq!(BubbleSource::for_variant(ToolVariant::BrushGroup { group: ToolGroup::Oil }).map(BubbleSource::kind), Some(BubbleKind::Medium));
    assert_eq!(BubbleSource::for_variant(ToolVariant::Command { command: CommandId::Lasso }), None);
    assert!(!BubbleSource::tears_without_hold(C::BrushSizeSlider), "plain drags keep editing sliders");
    assert!(BubbleSource::tears_without_hold(C::ColorPicker) && BubbleSource::tears_without_hold(C::Command { command: CommandId::Redo }));
}
