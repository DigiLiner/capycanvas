//! Every blender chosen from Sculpt › Blend or the brush list, tuned in Tool
//! Options and painted with compositor input over seeded color bands, in both
//! themes.
use super::super::canvas_bar_tests::{canvas_point, document};
use super::*;
use layer_core::DefaultBrushPreset as P;

const BLENDERS: [P; 20] = [
    P::Smudge, P::NaturalBlender, P::SoftBlender, P::Blur, P::Sharpen, P::BristleBlender, P::GrainyBlender, P::TextureBlender, P::HueBlender,
    P::ScatterBlender, P::PainterlyBlender, P::CrystalBlender, P::Marbling, P::CombBlender,
    P::EdgeKeepingBlender, P::DitherBlender, P::BlendingKnife, P::WetBlender, P::ColorMixer, P::MixingBrush,
];

fn committed(d: &Driver) -> u64 {
    ui_session(&d.w).engine().metrics().committed_strokes
}

fn path(d: &mut Driver, points: &[[f32; 2]]) {
    let before = committed(d);
    let surface: Vec<_> = points.iter().map(|&p| canvas_point(&d.w, p)).collect();
    let mut events = vec![contact("mouse", "down", surface[0])];
    events.extend(surface[1..].iter().map(|&p| contact("mouse", "move", p)));
    events.push(contact("mouse", "up", *surface.last().unwrap()));
    d.input.perform(serde_json::Value::Array(events));
    let deadline = Instant::now() + Duration::from_secs(10);
    while committed(d) == before {
        assert!(Instant::now() < deadline, "stroke from {:?} did not commit", points[0]);
        pump(20);
    }
}

fn seed(d: &mut Driver, size: [f32; 2]) {
    d.w.dispatch(UiAction::SelectBrush { id: P::GPen as u32 });
    d.w.dispatch(UiAction::SetBrushSize { value: 60. });
    let colors = [[0.85, 0.55, 0.02, 1.], [0.6, 0.02, 0.02, 1.], [0.02, 0.12, 0.6, 1.], [0.02, 0.35, 0.18, 1.]];
    for (band, rgba) in colors.into_iter().enumerate() {
        d.w.dispatch(UiAction::SetColor { rgba });
        let top = size[1] * (0.14 + 0.17 * band as f32);
        let rows = (size[1] * 0.17 / 24.).ceil() as usize;
        for row in 0..rows {
            let y = top + row as f32 * 24.;
            path(d, &[[size[0] * 0.04, y], [size[0] * 0.96, y]]);
        }
    }
}

fn show_tool_options(d: &Driver) {
    let mut workspace = state(&d.w).workspace;
    workspace.layout.set_panel_visible(Panel::ToolSettings, true).unwrap();
    workspace
        .layout
        .move_panel([1600., 1000.], Panel::ToolSettings, DockTarget::Edge { edge: Edge::Right, outer: false })
        .unwrap();
    d.w.dispatch(UiAction::RestoreWorkspace { workspace: Box::new(workspace) });
    pump(200);
}

fn choose_in_drawer(d: &mut Driver, preset: P) {
    let opener = d.header_tool(ToolbarControl::Command { command: CommandId::Sculpt });
    d.click_name(&opener);
    pump(300);
    if let Some(set) = find_named(d.w.window.upcast_ref(), "sculpt-set-blend").filter(|w| w.is_mapped()) {
        d.click(&set);
        pump(250);
    }
    let choice = d.named(&format!("brush-{}", preset as u32));
    let mut row = choice.clone();
    while !row.is_focusable() { row = row.parent().unwrap(); }
    row.grab_focus();
    pump(250);
    crate::capture(&d.w, d.input.dir.join(format!("blend-tools-{preset:?}-{:?}.png", state(&d.w).theme)).to_str().unwrap());
    d.click(&choice);
    assert_eq!(state(&d.w).brush.preset, preset as u32, "{preset:?} chosen from the Blend tool");
    if state(&d.w).customization.drawer.is_some() {
        d.input.key(0xff1b);
        pump(250);
    }
}

fn setting(d: &Driver, id: &str) -> gtk::Widget {
    find_named(&d.w.panel_widget(Panel::ToolSettings), &format!("tool-setting-{id}"))
        .filter(|w| w.is_mapped())
        .unwrap_or_else(|| panic!("Tool Options shows {id}"))
}

fn tune(d: &mut Driver, preset: P) {
    let edits: &[(&str, &str)] = match preset {
        P::SoftBlender => &[("spread", "30")],
        P::CrystalBlender => &[("cell_size", "30")],
        P::Marbling => &[("swirl", "200")],
        P::EdgeKeepingBlender => &[("edge_keeping", "80")],
        P::ColorMixer => &[("persistence", "120")],
        _ => &[],
    };
    for &(id, text) in edits {
        let control = setting(d, id);
        super::super::histogram::scroll_to(&control);
        pump(100);
        d.number(&control, text);
        pump(100);
        let brush = ui_session(&d.w).engine().configured_brush().clone();
        let value = match id {
            "spread" => brush.blender.reach,
            "cell_size" => brush.blender.scale,
            "persistence" => brush.wet_mix.stretch,
            _ => brush.blender.character,
        };
        assert!((value * 100. - text.parse::<f32>().unwrap()).abs() < 0.5, "{preset:?} {id} typed {text}: {value}");
    }
}

#[test]
#[ignore = "private Mutter: --native-test=native_blenders_paint"]
fn native_blenders_paint() {
    let mut d = Driver::new("art.capycanvas.Blenders");
    let size = document(&d.w).composition().size.map(|v| v as f32);
    show_tool_options(&d);
    for theme in [Theme::Light, Theme::Dark] {
        d.w.dispatch(UiAction::SetTheme { theme: Some(theme) });
        pump(200);
        seed(&mut d, size);
        for (index, preset) in BLENDERS.into_iter().enumerate() {
            if matches!(preset, P::SoftBlender | P::Marbling | P::WetBlender) {
                choose_in_drawer(&mut d, preset);
            } else {
                d.w.dispatch(UiAction::SelectBrush { id: preset as u32 });
            }
            let width = size[0] * 0.88 / BLENDERS.len() as f32;
            d.w.dispatch(UiAction::SetBrushSize { value: if preset == P::BlendingKnife { width * 0.25 } else { width * 0.8 } });
            pump(80);
            assert_eq!(state(&d.w).brush.preset, preset as u32);
            tune(&mut d, preset);
            let x = size[0] * 0.06 + width * (index as f32 + 0.5);
            let points: Vec<_> = (0..=40).map(|i| {
                let t = i as f32 / 40.;
                [x + if i % 2 == 0 { -0.3 } else { 0.3 } * width, size[1] * (0.1 + 0.75 * t)]
            }).collect();
            path(&mut d, &points);
            if preset == P::CrystalBlender {
                crate::capture(&d.w, d.input.dir.join(format!("blender-options-{theme:?}.png")).to_str().unwrap());
            }
        }
        d.capture_canvas(&format!("blenders-{theme:?}.png"));
        crate::capture(&d.w, d.input.dir.join(format!("blenders-window-{theme:?}.png")).to_str().unwrap());
        d.w.dispatch(UiAction::Invoke { command: CommandId::ClearLayer });
        pump(150);
    }
    assert!(state(&d.w).host_error.is_none());
    d.finish();
}
