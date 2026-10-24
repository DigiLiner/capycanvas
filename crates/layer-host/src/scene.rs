//! Per-frame scene inputs shared by the native presenters. Hosts own surface
//! acquisition, pacing, presenter construction and presentation.
use crate::NativeHost;
use layer_render_wgpu::{
    BackdropBlurStyle, BackdropRegion, OverviewPlacement, ViewportPresenter,
    local_tone::GpuToneGuide,
};
use layer_ui::{CanvasCursor, DrawerConnection};
use serde::Deserialize;
use std::sync::Arc;

const MAX_SLOTS: usize = 32;
const MAX_GLASS: usize = 256;

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigatorSlot {
    pub bounds: [f32; 4],
    pub clip: [f32; 4],
    pub order: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SlotUnits {
    #[default]
    Logical,
    Physical,
}

/// Navigator image slots in native layout coordinates. Empty slots are dropped.
#[derive(Default)]
pub struct Navigators {
    slots: Vec<NavigatorSlot>,
    units: SlotUnits,
}
impl Navigators {
    pub fn new(units: SlotUnits) -> Self {
        Self {
            slots: Vec::new(),
            units,
        }
    }

    pub fn set(&mut self, mut slots: Vec<NavigatorSlot>) -> Result<bool, String> {
        if slots.len() > MAX_SLOTS
            || slots
                .iter()
                .any(|s| !s.bounds.iter().chain(&s.clip).all(|v| v.is_finite()))
        {
            return Err("Invalid Navigator geometry".into());
        }
        slots.retain(|s| s.bounds[2] > 0. && s.bounds[3] > 0. && s.clip[2] > 0. && s.clip[3] > 0.);
        slots.sort_by_key(|s| s.order);
        let changed = self.slots != slots;
        self.slots = slots;
        Ok(changed)
    }

    pub fn count(&self) -> usize {
        self.slots.len()
    }

    /// Surface-pixel placements that sample the live composition and camera.
    pub fn placements(&self, host: &NativeHost, scale: f32) -> Vec<OverviewPlacement> {
        let state = host.session.state();
        let document = host.session.engine().document();
        let fg = state.palette.text.linear();
        let units = match self.units {
            SlotUnits::Logical => 1.,
            SlotUnits::Physical => scale,
        };
        self.slots
            .iter()
            .filter_map(|slot| {
                let [x, y, w, h] = slot.bounds.map(|v| v / units);
                let g = layer_ui::NavigatorGeometry::new(
                    &state.camera,
                    [document.composition().size[0], document.composition().size[1]],
                    [w, h],
                )?;
                Some(OverviewPlacement {
                    bounds: [
                        (x + g.image.x) * scale,
                        (y + g.image.y) * scale,
                        g.image.width * scale,
                        g.image.height * scale,
                    ],
                    clip: Some(slot.clip.map(|v| v / units * scale)),
                    work_area: g.work_area.map(|[a, b]| [(x + a) * scale, (y + b) * scale]),
                    outline_linear: [fg[0], fg[1], fg[2]],
                    scale,
                    opacity: 1.,
                })
            })
            .collect()
    }
}

/// Glass regions are `[x, y, w, h, tl, tr, br, bl]` in logical points, with an
/// optional ninth value of 1 for a squircle outline and tenth value for strength.
#[derive(Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlassLayout {
    regions: Vec<Vec<f32>>,
    connections: Vec<DrawerConnection>,
    #[serde(default)]
    arcs: Vec<[f32; 7]>,
    #[serde(default)]
    color_bubbles: Vec<[f32; 7]>,
}

#[derive(Default)]
pub struct Glass {
    layout: GlassLayout,
    scaled: Vec<BackdropRegion>,
    scale: f32,
}
impl Glass {
    pub fn set(&mut self, layout: GlassLayout) -> Result<bool, String> {
        let regions = layout.regions.iter().all(|r| {
            matches!(r.len(), 8..=10) && r.iter().all(|v| v.is_finite()) && r[2] >= 0. && r[3] >= 0.
        });
        let arcs = layout.arcs.iter().all(|a| a.iter().all(|v| v.is_finite()) && a[2] >= 0. && a[3] >= 0.);
        let color_bubbles = layout.color_bubbles.iter().all(|b| b.iter().all(|v| v.is_finite()) && b[2..5].iter().all(|v| *v >= 0.));
        let connections = layout.connections.iter().all(|c| {
            [
                c.bounds.x,
                c.bounds.y,
                c.bounds.width,
                c.bounds.height,
                c.length,
                c.depth,
            ]
            .iter()
            .chain(&c.transform)
            .chain(&c.radii)
            .all(|v| v.is_finite())
        });
        if layout.regions.len() + layout.connections.len() + layout.arcs.len() + layout.color_bubbles.len() > MAX_GLASS || !regions || !connections || !arcs || !color_bubbles {
            return Err("Invalid glass geometry".into());
        }
        let changed = self.layout != layout;
        if changed {
            self.layout = layout;
            self.scale = 0.;
        }
        Ok(changed)
    }

    pub fn regions(&mut self, scale: f32) -> &[BackdropRegion] {
        if self.scale != scale {
            self.scale = scale;
            let boxes = self.layout.regions.iter().map(|b| {
                BackdropRegion::rounded(
                    [b[0], b[1], b[2], b[3]].map(|v| v * scale),
                    [b[4], b[5], b[6], b[7]].map(|v| v * scale),
                    if b.get(8) == Some(&1.) {
                        BackdropRegion::SQUIRCLE
                    } else {
                        BackdropRegion::CIRCULAR
                    },
                ).with_strength(b.get(9).copied().unwrap_or(1.))
            });
            let connections =
                self.layout
                    .connections
                    .iter()
                    .flat_map(|c| c.glass())
                    .map(|(bounds, radii)| BackdropRegion {
                        bounds: bounds.map(|v| v * scale),
                        radii: radii.map(|v| v * scale),
                        shape: BackdropRegion::SQUIRCLE,
                    });
            let arcs = self.layout.arcs.iter().map(|a| BackdropRegion::arc(
                [a[0] * scale, a[1] * scale], a[2] * scale, a[3] * scale, a[4], a[5],
            ).with_strength(a[6]));
            let color_bubbles = self.layout.color_bubbles.iter().map(|b| BackdropRegion::color_bubble(
                [b[0] * scale, b[1] * scale], b[2] * scale, b[3] * scale, b[4] * scale, b[5],
            ).with_strength(b[6]));
            self.scaled = boxes.chain(connections).chain(arcs).chain(color_bubbles).collect();
        }
        &self.scaled
    }

    pub fn count(&self) -> usize {
        self.layout.regions.len() + self.layout.connections.len() + self.layout.arcs.len() + self.layout.color_bubbles.len()
    }
}

pub struct SceneDisplay {
    /// Surface pixels per logical point.
    pub scale: f32,
    /// Display headroom for the SDR-relative HDR view.
    pub headroom: f32,
    pub blank_presented: bool,
    /// The platform compositor tone maps the HDR surface itself.
    pub compositor_hdr: bool,
}

impl NativeHost {
    pub fn set_glass(&mut self, glass: &mut Glass, layout: GlassLayout) -> Result<(), String> {
        let count = glass.count();
        if glass.set(layout)? {
            self.dirty = true;
            if count != glass.count() { self.invalidate_snapshot(); }
        }
        Ok(())
    }

    /// Applies this frame's cursor, color picker, proof, HDR view, tone guide,
    /// navigators and glass. Optional overview and backdrop pipelines wait for
    /// the first paper frame and a ready canvas.
    pub fn compose_scene(
        &mut self,
        presenter: &mut ViewportPresenter,
        cursor: &mut CanvasCursor,
        navigators: &Navigators,
        glass: &[BackdropRegion],
        tone: Option<Arc<GpuToneGuide>>,
        display: SceneDisplay,
    ) -> Result<(), String> {
        let error = |e: layer_render_wgpu::GpuRasterError| e.to_string();
        self.session.update_canvas_cursor(cursor);
        self.session.append_layer_overlay(&mut cursor.segments);
        let proof = self.proof.lut(&self.session);
        let staged = display.blank_presented && self.startup.canvas_ready;
        let overviews = if staged {
            navigators.placements(self, display.scale)
        } else {
            Vec::new()
        };
        let session = &self.session;
        let state = session.state();
        let style = state.palette.glass;
        let backdrop = if staged && style.transparency.enabled() {
            glass
        } else {
            &[]
        };
        let gpu = session
            .engine()
            .backend()
            .0
            .as_deref()
            .ok_or("Missing native renderer")?;
        let rendition = session
            .engine()
            .document()
            .composition().color
            .depth
            .is_float()
            .then(|| session.effective_sdr_rendition());
        if display.compositor_hdr {
            let rendition = rendition.ok_or("HDR output requires HDR artwork")?;
            presenter
                .set_compositor_hdr_view(gpu, rendition)
                .map_err(error)?;
        } else {
            let headroom = if session.hdr_presentation_allowed() {
                display.headroom.max(1.)
            } else {
                1.
            };
            presenter
                .set_hdr_view(gpu, rendition, headroom)
                .map_err(error)?;
        }
        presenter
            .set_gpu_local_tone_guide(gpu, tone)
            .map_err(error)?;
        presenter
            .set_proof(gpu, proof, state.soft_proof, state.gamut_warning)
            .map_err(error)?;
        presenter.set_cursor(gpu.device(), &cursor.segments, display.scale);
        presenter.set_color_picker(gpu, session.color_picker_overlay());
        presenter.set_overviews(gpu, &overviews);
        presenter.set_backdrop(
            gpu,
            backdrop,
            BackdropBlurStyle {
                levels: style.blur.levels,
                offset: style.blur.offset,
            },
            session.hold_canvas_backdrop(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glass(value: serde_json::Value) -> Result<GlassLayout, serde_json::Error> {
        serde_json::from_value(value)
    }

    #[test]
    fn color_bubble_glass_transports_one_scaled_contour_and_independent_strength() {
        let mut layer = Glass::default();
        let layout = |strength| glass(serde_json::json!({"regions":[],"connections":[],"color_bubbles":[[30,40,64,72,9,0.75,strength]]})).unwrap();
        assert!(layer.set(layout(0.375)).unwrap());
        assert_eq!(layer.count(), 1);
        for scale in [2., 1.5, 2.] {
            assert_eq!(layer.regions(scale), [BackdropRegion::color_bubble([30.*scale,40.*scale],64.*scale,72.*scale,9.*scale,0.75).with_strength(0.375)]);
        }
        let before = layer.regions(2.).to_vec();
        assert!(layer.set(layout(0.125)).unwrap());
        assert!(!layer.set(layout(0.125)).unwrap());
        let after = layer.regions(2.);
        assert_eq!(before[0].bounds, after[0].bounds);
        assert_eq!(before[0].radii, after[0].radii);
        assert_eq!(after[0].shape[1], 0.125);
        assert_eq!(before[0].shape[2..], after[0].shape[2..]);
    }

    #[test]
    fn invalid_color_bubbles_preserve_glass_and_share_the_region_limit() {
        let valid = || glass(serde_json::json!({"regions":[[1,2,30,40,3,3,3,3]],"connections":[],"color_bubbles":[[30,40,64,72,9,0.75,0.5]]})).unwrap();
        let mut layer = Glass::default();
        layer.set(valid()).unwrap();
        let before = layer.regions(1.5).to_vec();
        for index in 0..7 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut invalid = valid(); invalid.color_bubbles[0][index] = value;
                assert!(layer.set(invalid).is_err());
                assert_eq!(layer.regions(1.5), before);
            }
        }
        for index in 2..5 {
            let mut invalid = valid(); invalid.color_bubbles[0][index] = -1.;
            assert!(layer.set(invalid).is_err());
            assert_eq!(layer.regions(1.5), before);
        }
        let mut invalid = valid(); invalid.color_bubbles = vec![[0.;7]; MAX_GLASS];
        assert!(layer.set(invalid).is_err());
        assert_eq!(layer.count(), 2);
        assert_eq!(layer.regions(1.5), before);
        let mut maximum = valid(); maximum.color_bubbles = vec![[0.;7]; MAX_GLASS - 1];
        assert!(layer.set(maximum).unwrap());
        assert_eq!(layer.count(), MAX_GLASS);
        for entry in [serde_json::json!([0,0,1,1,1,0]), serde_json::json!([0,0,1,1,1,0,1,1])] {
            assert!(glass(serde_json::json!({"regions":[],"connections":[],"color_bubbles":[entry]})).is_err());
        }
    }

    #[test]
    fn glass_arcs_scale_geometry_once_without_scaling_angles_or_fade() {
        let mut layer = Glass::default();
        let layout = |strength| serde_json::json!({"regions":[[10,20,100,50,4,5,6,7,1,strength]],"connections":[],"arcs":[[30,40,20,6,0.25,-1.5,strength]]});
        assert!(layer.set(glass(layout(0.375)).unwrap()).unwrap());
        for scale in [2., 1.5, 2.] {
            let regions = layer.regions(scale);
            assert_eq!(regions[0].bounds, [10.,20.,100.,50.].map(|v| v*scale));
            assert_eq!(regions[0].radii, [4.,5.,6.,7.].map(|v| v*scale));
            assert_eq!(regions[0].shape, [4.,0.375,0.,0.]);
            let arc = BackdropRegion::arc([30.*scale,40.*scale],20.*scale,6.*scale,0.25,-1.5).with_strength(0.375);
            assert_eq!(regions[1], arc);
            assert_eq!(regions[1].radii, [30.*scale,40.*scale,20.*scale,3.*scale]);
            assert_eq!(regions[1].shape, [0.,0.375,-1.25,1.5]);
        }
        let before = layer.regions(2.).to_vec();
        assert!(layer.set(glass(layout(0.125)).unwrap()).unwrap());
        assert!(!layer.set(glass(layout(0.125)).unwrap()).unwrap());
        for (old, new) in before.iter().zip(layer.regions(2.)) {
            assert_eq!(old.bounds, new.bounds);
            assert_eq!(old.radii, new.radii);
            assert_eq!(new.shape[1], 0.125);
            assert_eq!(old.shape[2..], new.shape[2..]);
        }
    }

    #[test]
    fn invalid_arc_or_fade_preserves_the_last_valid_glass_layout() {
        let valid = || glass(serde_json::json!({"regions":[[1,2,30,40,3,3,3,3,0,0.5]],"connections":[],"arcs":[[20,30,12,4,0,2,0.75]]})).unwrap();
        let mut layer = Glass::default();
        layer.set(valid()).unwrap();
        let before = layer.regions(1.5).to_vec();
        for index in 0..7 {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut invalid = valid();
                invalid.arcs[0][index] = value;
                assert!(layer.set(invalid).is_err());
                assert_eq!(layer.regions(1.5), before);
            }
        }
        for index in [2,3] {
            let mut invalid = valid(); invalid.arcs[0][index] = -1.;
            assert!(layer.set(invalid).is_err());
            assert_eq!(layer.regions(1.5), before);
        }
        let mut invalid = valid(); invalid.regions[0][9] = f32::NAN;
        assert!(layer.set(invalid).is_err());
        let mut invalid = valid(); invalid.arcs = vec![[0.;7]; MAX_GLASS];
        assert!(layer.set(invalid).is_err());
        assert_eq!(layer.count(), 2);
        assert_eq!(layer.regions(1.5), before);
        let mut maximum = valid(); maximum.arcs = vec![[0.;7]; MAX_GLASS-1];
        assert!(layer.set(maximum).unwrap());
        assert_eq!(layer.count(), MAX_GLASS);
        assert!(glass(serde_json::json!({"regions":[],"connections":[],"arcs":[[0,0,1,1,0,1]]})).is_err());
    }

    #[test]
    fn glass_motion_and_fade_dirty_render_without_republishing_the_model() {
        let mut host = NativeHost::new(layer_ui::Platform::Windows).unwrap();
        let mut layer = Glass::default();
        host.take_update_bytes().unwrap().unwrap();
        let revision = host.session.state().revision;
        let layout = |x, strength| glass(serde_json::json!({
            "regions":[[x,20,100,50,8,8,8,8,1,strength]],"connections":[],
            "arcs":[[x,40,20,6,0.25,1.5,strength]]
        })).unwrap();
        host.set_glass(&mut layer, layout(10., 1.)).unwrap();
        assert!(host.take_update_bytes().unwrap().is_some());
        let mut previous = layer.regions(1.5).to_vec();
        for (x, strength) in [(30.,1.),(30.,0.5),(60.,0.125)] {
            host.dirty = false;
            host.set_glass(&mut layer, layout(x, strength)).unwrap();
            assert!(host.dirty, "Motion and fade must redraw glass");
            assert_eq!(layer.count(), 2);
            assert_ne!(layer.regions(1.5), previous);
            previous = layer.regions(1.5).to_vec();
            assert!(host.take_update_bytes().unwrap().is_none(), "Unchanged metadata must not republish the UI model");
            assert_eq!(host.session.state().revision, revision);
            host.dirty = false;
            host.set_glass(&mut layer, layout(x, strength)).unwrap();
            assert!(!host.dirty, "Identical geometry must remain idle");
        }
        host.set_glass(&mut layer, glass(serde_json::json!({"regions":[],"connections":[]})).unwrap()).unwrap();
        assert!(host.dirty);
        assert_eq!(layer.count(), 0);
        assert!(host.take_update_bytes().unwrap().is_some(), "A changed count must republish host metadata");
        assert_eq!(host.session.state().revision, revision);
    }

    #[test]
    fn glass_updates_publish_host_metadata_without_document_edits() {
        let mut host = NativeHost::new(layer_ui::Platform::Windows).unwrap();
        let mut layer = Glass::default();
        assert!(host.take_update_bytes().unwrap().is_some());
        assert!(host.take_update_bytes().unwrap().is_none());
        let revision = host.session.state().revision;
        for regions in [
            vec![[10, 20, 100, 50, 8, 8, 8, 8]],
            vec![[10, 20, 100, 50, 8, 8, 8, 8], [50, 80, 200, 40, 8, 8, 8, 8]],
            vec![],
        ] {
            let layout = serde_json::json!({"regions": regions, "connections": []});
            host.dirty = false;
            host.set_glass(&mut layer, glass(layout.clone()).unwrap()).unwrap();
            assert!(host.dirty);
            let bytes = host.take_update_bytes().unwrap().expect("Changed glass publishes host metadata");
            let metadata = serde_json::to_vec(&serde_json::json!({"windows_glass": {"regions": layer.count()}})).unwrap();
            let update: serde_json::Value = serde_json::from_slice(&crate::extend_update(bytes, &metadata)).unwrap();
            assert!(update["state"].is_object());
            assert_eq!(update["windows_glass"]["regions"], regions.len());
            assert_eq!(host.session.state().revision, revision);
            host.dirty = false;
            host.set_glass(&mut layer, glass(layout).unwrap()).unwrap();
            assert!(!host.dirty);
            assert!(host.take_update_bytes().unwrap().is_none());
        }
        let invalid = serde_json::json!({"regions": [[0, 0, -1, 10, 0, 0, 0, 0]], "connections": []});
        assert!(host.set_glass(&mut layer, glass(invalid).unwrap()).is_err());
        assert!(!host.dirty);
        assert_eq!(layer.count(), 0);
        assert!(host.take_update_bytes().unwrap().is_none());
        assert_eq!(host.session.state().revision, revision);
    }

    #[test]
    fn glass_regions_scale_once_and_reject_invalid_geometry() {
        let mut layer = Glass::default();
        let layout =
            serde_json::json!({"regions": [[10, 20, 100, 50, 8, 8, 8, 8]], "connections": []});
        assert!(layer.set(glass(layout.clone()).unwrap()).unwrap());
        assert!(!layer.set(glass(layout).unwrap()).unwrap());
        let region = layer.regions(1.5)[0];
        assert_eq!(region.bounds, [15., 30., 150., 75.]);
        assert_eq!(region.radii, [12.; 4]);
        assert_eq!(region.shape, BackdropRegion::CIRCULAR);
        assert_eq!(layer.count(), 1);
        for invalid in [
            serde_json::json!({"regions": [[0, 0, -1, 10, 0, 0, 0, 0]], "connections": []}),
            serde_json::json!({"regions": [[0, 0, 10, 10, 0, 0, 0]], "connections": []}),
            serde_json::json!({"regions": vec![[0, 0, 1, 1, 0, 0, 0, 0]; 257], "connections": []}),
        ] {
            assert!(layer.set(glass(invalid).unwrap()).is_err());
            assert_eq!(layer.count(), 1, "Failures keep the previous layout");
        }
        assert!(glass(serde_json::json!({"regions": [], "connections": [], "extra": 1})).is_err());
        let squircle =
            serde_json::json!({"regions": [[0, 0, 36, 36, 18, 18, 18, 18, 1]], "connections": []});
        assert!(layer.set(glass(squircle).unwrap()).unwrap());
        assert_eq!(layer.regions(1.)[0].shape, BackdropRegion::SQUIRCLE);
        let connection = DrawerConnection {
            bounds: layer_ui::Bounds {
                x: 100.,
                y: 200.,
                width: 60.,
                height: 12.,
            },
            transform: [1., 0., 0., 1., 6., 0.],
            length: 48.,
            depth: 12.,
            radii: [6., 6.],
            square_corners: [false; 4],
        };
        let connected = serde_json::json!({"regions": [], "connections": [connection]});
        assert!(layer.set(glass(connected).unwrap()).unwrap());
        assert_eq!(
            layer.regions(2.).len(),
            3,
            "A connector adds its body and both feet"
        );
        assert!(
            layer
                .set(glass(serde_json::json!({"regions": [], "connections": []})).unwrap())
                .unwrap()
        );
        assert!(layer.regions(1.5).is_empty());
    }

    #[test]
    fn navigator_slots_are_bounded_ordered_and_scaled() {
        let host = NativeHost::new(layer_ui::Platform::Mac).unwrap();
        let slot = |x: f32, order: i32| NavigatorSlot {
            bounds: [x, 20., 264., 200.],
            clip: [x, 30., 200., 170.],
            order,
        };
        let mut navigators = Navigators::default();
        assert!(navigators.set(vec![slot(30., 9), slot(10., 3)]).unwrap());
        assert!(!navigators.set(vec![slot(10., 3), slot(30., 9)]).unwrap());
        let logical = navigators.placements(&host, 2.);
        assert_eq!(logical.len(), 2);
        assert!(logical[0].bounds[0] < logical[1].bounds[0]);
        assert_eq!(logical[0].clip, Some([20., 60., 400., 340.]));
        assert_eq!(logical[0].scale, 2.);
        let mut empty = slot(0., 0);
        empty.bounds[2] = 0.;
        assert!(navigators.set(vec![empty]).unwrap());
        assert_eq!(navigators.count(), 0);
        let mut invalid = slot(0., 0);
        invalid.clip[0] = f32::NAN;
        assert!(navigators.set(vec![invalid]).is_err());
        assert!(navigators.set(vec![slot(0., 0); 33]).is_err());
        navigators.set(vec![slot(10., 3)]).unwrap();
        let mut physical = Navigators::new(SlotUnits::Physical);
        physical
            .set(vec![NavigatorSlot {
                bounds: [20., 40., 528., 400.],
                clip: [20., 60., 400., 340.],
                order: 3,
            }])
            .unwrap();
        let from_physical = physical.placements(&host, 2.);
        let from_logical = navigators.placements(&host, 2.);
        assert_eq!(from_physical[0].bounds, from_logical[0].bounds);
        assert_eq!(from_physical[0].clip, from_logical[0].clip);
        let mut host = host;
        host.dispatch(layer_ui::UiAction::Invoke {
            command: layer_ui::CommandId::RotateRight,
        })
        .unwrap();
        let rotated = navigators.placements(&host, 2.)[0];
        assert_eq!(rotated.bounds, from_logical[0].bounds);
        assert_ne!(rotated.work_area, from_logical[0].work_area);
        let scaled = navigators.placements(&host, 1.)[0];
        assert_eq!(scaled.bounds, rotated.bounds.map(|v| v / 2.));
    }
}
