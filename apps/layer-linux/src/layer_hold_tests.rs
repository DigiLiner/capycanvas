//! Actual Mutter touch/mouse delivery for held row reordering and scrolling.
use super::*;

#[test]
#[ignore = "isolated native-input.js --layer-hold"]
fn native_layer_hold_input() {
    let app = native_test_app("art.capycanvas.LayerHold");
    let w = fixture_workspace(&app);
    w.window.maximize();
    w.window.present();
    pump(1600);
    w.dispatch(UiAction::RestoreWorkspace {
        workspace: Box::new(layer_ui::WorkspaceState::default()),
    });
    for _ in 0..2 {
        w.dispatch(UiAction::Layer {
            action: layer_ui::LayerAction::New {
                group: false,
                clipped: false,
            },
        });
    }
    pump(300);
    let order = || state(&w).layers.iter().map(|l| l.id).collect::<Vec<_>>();
    let before = order();
    w.dispatch(UiAction::Layer {
        action: layer_ui::LayerAction::AddMask {
            id: before[0],
            replace: false,
        },
    });
    pump(300);
    let row = |id| widgets(w.layer_panel.root.upcast_ref()).find(|node| node.is_mapped() && node.widget_name() == format!("art-layer-{id}")).unwrap();
    let bounds = |node: &gtk::Widget| node.compute_bounds(&w.surface).unwrap();
    let dismiss = || {
        let popovers: Vec<_> = w
            .popovers
            .borrow()
            .iter()
            .filter_map(|p| p.upgrade())
            .collect();
        for p in popovers {
            p.popdown();
            p.set_autohide(true);
        }
        for swipe in widgets(w.layer_panel.root.upcast_ref()).filter_map(|node| node.downcast::<crate::swipe_row::SwipeRow>().ok()) {
            swipe.reset();
        }
        pump(120);
    };
    let mut input = RemoteInput::new().settle_ms(120);
    input.ready();
    pump(400);
    for theme in [Theme::Light, Theme::Dark] {
    w.dispatch(UiAction::SetTheme { theme: Some(theme) });
    pump(180);
    for touch in [false, true] {
        let device = if touch { "touch" } else { "mouse" };
        for handle in [false, true] {
            let source = row(before[0]);
            let node = if handle {
                source.last_child().unwrap()
            } else {
                find_css(&source, "layer-name").unwrap()
            };
            let b = bounds(&node);
            let start = [b.x() + b.width() / 2., b.y() + b.height() / 2.];
            let b = bounds(&row(before[1]));
            let target = [b.x() + b.width() / 2., b.y() + b.height() - 3.];
            if handle {
                let picked = w.surface.pick(start[0] as f64, start[1] as f64, gtk::PickFlags::DEFAULT).unwrap();
                assert!(picked == node || picked.is_ancestor(&node), "{theme:?}/{device}: grip owns native hit target");
            }
            let slop = gtk::Settings::default().unwrap().gtk_dnd_drag_threshold() as f32;
            let pickup = [start[0] - slop * 3., start[1]];
            let jitter = [start[0] - 2., start[1]];
            let controllers = if handle {
                node.observe_controllers()
            } else {
                source.observe_controllers()
            };
            let drag = (0..controllers.n_items())
                .find_map(|i| controllers.item(i).and_downcast::<gtk::DragSource>())
                .unwrap();
            if touch {
                input.perform(serde_json::json!([{"touch":"down", "point":start}]));
                if handle {
                    assert_eq!(drag.sequence_state(&drag.last_updated_sequence().unwrap()), gtk::EventSequenceState::Claimed,
                        "{theme:?}/{device}: grip contact is claimed before slop");
                }
            }
            input.perform(if touch {
                serde_json::json!([{"touch":"move","point":jitter},{"touch":"move","point":pickup}])
            } else {
                serde_json::json!([{"point":start},{"down":true},{"point":jitter},{"point":pickup}])
            });
            assert_eq!(
                drag.drag().is_some(),
                handle || !touch,
                "pickup crosses slop without waiting for a hold"
            );
            let drop = [target[0] + 1., target[1]];
            input.perform(serde_json::json!([
                contact(device, "move", drop),
                contact(device, "up", drop)
            ]));
            if handle || !touch {
                assert_ne!(
                    order(),
                    before,
                    "touch={touch} handle={handle}: immediate row pickup"
                );
                w.dispatch(UiAction::Invoke {
                    command: CommandId::Undo,
                });
                pump(150);
            }
            assert_eq!(
                order(),
                before,
                "unheld touch body must not reorder; direct drag is one undo"
            );
            dismiss();
        }
    }
    for region in [
        "padding", "name", "content", "mask", "eye", "check", "link", "grip",
    ] {
        for release_only in [true, false] {
            dismiss();
            let source = row(before[0]);
            let eye = source.first_child().unwrap();
            let check = eye.next_sibling().unwrap();
            let thumbnails = check.next_sibling().unwrap();
            let node = match region {
                "name" => find_css(&source, "layer-name").unwrap(),
                "content" => thumbnails.first_child().unwrap().next_sibling().unwrap(),
                "mask" => thumbnails.last_child().unwrap(),
                "eye" => eye,
                "check" => check,
                "link" => thumbnails
                    .first_child()
                    .unwrap()
                    .next_sibling()
                    .unwrap()
                    .next_sibling()
                    .unwrap(),
                "grip" => source.last_child().unwrap(),
                _ => source.clone(),
            };
            let b = bounds(&node);
            let start = [
                b.x()
                    + if region == "padding" {
                        1.
                    } else {
                        b.width() / 2.
                    },
                b.y() + b.height() / 2.,
            ];
            let visible = state(&w)
                .layers
                .iter()
                .find(|l| l.id == before[0])
                .unwrap()
                .visible;
            let mask_before = state(&w).layer_tools.editing_layer.as_ref().unwrap().mask_selected;
            input.perform(serde_json::json!([{"touch":"down", "point":start}]));
            pump(800);
            let menu = w
                .popovers
                .borrow()
                .iter()
                .filter_map(|p| p.upgrade())
                .find(|p| p.is_visible())
                .expect("hold opens menu");
            assert_eq!(
                state(&w)
                    .layer_tools
                    .editing_layer
                    .as_ref()
                    .unwrap()
                    .mask_selected,
                region == "mask" || mask_before,
                "hold preserves editing target unless the mask menu is requested"
            );
            if release_only {
                input.perform(serde_json::json!([{"touch":"up"}]));
                assert!(menu.is_visible(), "{region}: release keeps menu");
                assert_eq!(order(), before);
                assert_eq!(
                    state(&w)
                        .layers
                        .iter()
                        .find(|l| l.id == before[0])
                        .unwrap()
                        .visible,
                    visible,
                    "hold must not click row buttons"
                );
            } else {
                let b = bounds(&row(before[1]));
                let point = [b.x() + b.width() / 2., b.y() + b.height() - 3.];
                input.perform(serde_json::json!([{"touch":"move", "point":point}]));
                if region == "grip" {
                    assert!(!menu.is_visible(), "{theme:?}: held grip pickup closes menu");
                    input.perform(serde_json::json!([{"touch":"move", "point":[point[0]+1.,point[1]]}, {"touch":"up"}]));
                    assert_ne!(order(), before, "{theme:?}: held grip commits native reorder");
                    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
                    pump(150);
                    assert_eq!(order(), before, "one undo restores held grip reorder");
                    w.dispatch(UiAction::Invoke { command: CommandId::Redo });
                    pump(150);
                    assert_ne!(order(), before);
                    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
                    pump(150);
                } else {
                    let controllers = source.observe_controllers();
                    let drag = (0..controllers.n_items()).find_map(|i| controllers.item(i).and_downcast::<gtk::DragSource>()).unwrap();
                    assert!(drag.drag().is_none(), "{theme:?}/{region}: held body cannot start native reorder");
                    input.perform(serde_json::json!([{"touch":"up"}]));
                    assert_eq!(order(), before, "{theme:?}/{region}: hold movement changes no layer order");
                }
            }
        }
    }
    dismiss();
    let b = bounds(&find_css(&row(before[0]), "layer-name").unwrap());
    let start = [b.x() + b.width() / 2., b.y() + b.height() / 2.];
    input.perform(serde_json::json!([{"touch":"down","point":start}]));
    pump(800);
    let b = bounds(&row(before[1]));
    let point = [b.x() + b.width() / 2., b.y() + b.height() - 3.];
    input.perform(serde_json::json!([{"touch":"move","point":point}]));
    input.perform(
        serde_json::json!([{"key":65307,"down":true},{"key":65307,"down":false},{"touch":"up"}]),
    );
    assert_eq!(
        order(),
        before,
        "Escape after body hold changes no layer order"
    );
    dismiss();
    let b = bounds(&find_css(&row(before[0]), "layer-name").unwrap());
    let start = [b.x() + b.width() / 2., b.y() + b.height() / 2.];
    input.perform(serde_json::json!([{"point":start},{"down":true}]));
    pump(800);
    assert!(
        !w.popovers
            .borrow()
            .iter()
            .filter_map(|p| p.upgrade())
            .any(|p| p.is_visible()),
        "mouse hold must not open a menu"
    );
    let slop = gtk::Settings::default().unwrap().gtk_dnd_drag_threshold() as f32;
    input.perform(serde_json::json!([{"point":[start[0]-2.,start[1]]},{"point":[start[0]-slop*3.,start[1]]},{"point":point},{"point":[point[0]+1.,point[1]]},{"down":false}]));
    assert_ne!(order(), before, "mouse continues held drag");
    w.dispatch(UiAction::Invoke {
        command: CommandId::Undo,
    });
    pump(150);
    assert_eq!(order(), before);
    let source = row(before[0]);
    let swipe = source.parent().unwrap().downcast::<crate::swipe_row::SwipeRow>().unwrap();
    let grip = source.last_child().unwrap();
    let controllers = grip.observe_controllers();
    let drag = (0..controllers.n_items()).find_map(|i| controllers.item(i).and_downcast::<gtk::DragSource>()).unwrap();
    let start = screen_point(&source, &w.surface, [0.5, 0.5]);
    input.perform(serde_json::json!([{"touch":"down", "point":start}]));
    let mut opacity = 1.;
    for (distance, fading_out) in [(36., true), (72., true), (36., false)] {
        input.perform(serde_json::json!([{"touch":"move", "point":[start[0] - distance, start[1]]}]));
        let next = grip.opacity();
        assert!(grip.is_child_visible() && next > 0. && next < 1., "{theme:?}: partial swipe keeps a translucent grip");
        assert_eq!(next < opacity, fading_out, "{theme:?}: grip fades with swipe distance in either direction");
        opacity = next;
        if distance == 72. && std::env::var_os("LAYER_NATIVE_CAPTURE_DIR").is_some() {
            input.perform(serde_json::json!([{"capture":format!("swipe-grip-{}",format!("{theme:?}").to_lowercase())}]));
        }
    }
    input.perform(serde_json::json!([{"touch":"move", "point":start}, {"touch":"up"}]));
    pump(180);
    assert_eq!(grip.opacity(), 1., "{theme:?}: reverse swipe restores full grip opacity");
    swipe.reveal(true);
    pump(180);
    assert!(!swipe.is_at_rest());
    assert_eq!(grip.opacity(), 0., "{theme:?}: open swipe fades grip completely");
    assert!(drag.emit_by_name::<Option<gdk::ContentProvider>>("prepare", &[&8f64, &8f64]).is_none(),
        "{theme:?}: displaced row refuses native drag preparation");
    swipe.reveal(false);
    assert!(!swipe.is_at_rest(), "closing animation retains displaced state");
    pump(50);
    assert!(grip.opacity() > 0. && grip.opacity() < 1., "{theme:?}: closing animation fades grip back in");
    pump(180);
    assert!(swipe.is_at_rest());
    assert_eq!(grip.opacity(), 1., "{theme:?}: restored row exposes grip");
    let start = screen_point(&grip, &w.surface, [0.5, 0.5]);
    input.perform(serde_json::json!([{"touch":"down", "point":start}]));
    assert!(drag.emit_by_name::<Option<gdk::ContentProvider>>("prepare", &[&8f64, &8f64]).is_some(),
        "{theme:?}: restored row permits drag preparation from native grip contact");
    input.perform(serde_json::json!([{"touch":"up"}]));
    swipe.reveal(true);
    pump(180);
    swipe.reset();
    pump(30);
    assert!(swipe.is_at_rest() && grip.opacity() == 1., "{theme:?}: cancelled swipe restores grip");
    }
    let layer = |action| { w.dispatch(UiAction::Layer { action }); pump(150); };
    let checked = || state(&w).layers.iter().filter(|r| r.selected).map(|r| r.id).collect::<Vec<_>>();
    let check = |id| row(id).first_child().unwrap().next_sibling().unwrap();
    let click = |input: &mut RemoteInput, node: &gtk::Widget| input.click(screen_point(node, &w.surface, [0.5,0.5]));
    let capture = |input: &mut RemoteInput, theme: Theme, name: &str| {
        if std::env::var_os("LAYER_NATIVE_CAPTURE_DIR").is_some() {
            input.perform(serde_json::json!([{"capture":format!("{name}-{}",format!("{theme:?}").to_lowercase())}]));
        }
    };
    for theme in [Theme::Light,Theme::Dark] {
        w.dispatch(UiAction::SetTheme { theme: Some(theme) }); pump(180);
        layer(layer_ui::LayerAction::Select { id: before[0], mask: true });
        click(&mut input,&check(before[1]));
        click(&mut input,&find_css(&row(before[0]),"layer-name").unwrap());
        assert!(state(&w).layer_tools.editing_layer.unwrap().mask_selected,"active row preserves mask editing");
        assert_eq!(checked(),before[..2],"active row preserves checked companions");
        let link = find_css(&row(before[0]),"layer-link").unwrap().downcast::<gtk::Button>().unwrap();
        click(&mut input,link.upcast_ref());
        assert_eq!(crate::icons::name(&link.child().unwrap().downcast::<gtk::Image>().unwrap()).as_deref(),Some("layer-unlink-symbolic"));
        capture(&mut input,theme,"mask-unlinked");
        click(&mut input,link.upcast_ref());
        assert_eq!(crate::icons::name(&link.child().unwrap().downcast::<gtk::Image>().unwrap()).as_deref(),Some("layer-link-symbolic"));
        capture(&mut input,theme,"mask-linked");
        click(&mut input,&find_css(&row(before[0]),"layer-thumbnail").unwrap());
        input.perform(serde_json::json!([{"key":65505,"down":true}]));
        click(&mut input,&find_css(&row(before[2]),"layer-name").unwrap());
        input.perform(serde_json::json!([{"key":65505,"down":false}]));
        assert_eq!(checked(),before[..3],"Shift-click selects a visible range");
        layer(layer_ui::LayerAction::Select { id: before[0], mask: false });
        input.perform(serde_json::json!([{"key":65505,"down":true}]));
        click(&mut input,&check(before[2]));
        input.perform(serde_json::json!([{"key":65505,"down":false}]));
        assert_eq!(checked(),before[..3],"Shift checkbox selects the same range");
        let group = find_named(w.layer_panel.footer.upcast_ref(),"layer-folder-symbolic").unwrap();
        click(&mut input,&group);
        let group_id = state(&w).layer_tools.editing_layer.unwrap().id;
        assert!(state(&w).layers.iter().filter(|r| before[..3].contains(&r.id)).all(|r|r.depth==1));
        assert!(!state(&w).layers.iter().find(|r|r.id==group_id).unwrap().content_selected);
        click(&mut input,&find_named(w.layer_panel.footer.upcast_ref(),"delete-selected-layers").unwrap());
        assert_eq!(order(),before,"deleting an expanded folder preserves unchecked children");
        for _ in 0..2 { w.dispatch(UiAction::Invoke { command: CommandId::Undo }); pump(150); }
        assert_eq!(order(),before);
        layer(layer_ui::LayerAction::Select { id: before[0], mask: false });
        w.dispatch(UiAction::Effect { action: EffectAction::Insert { effect: "curves".into() } }); pump(180);
        w.dispatch(UiAction::SelectPanelTab { group: state(&w).workspace.layout.panel_group(Panel::Layers).unwrap(), panel: Panel::Layers }); pump(180);
        let effect = state(&w).layer_tools.editing_layer.unwrap().id;
        let thumbnail = find_css(&row(effect),"layer-thumbnail").unwrap();
        assert!(!thumbnail.has_css_class("layer-editing"),"filter glyph has no editable-pixel border");
        capture(&mut input,theme,"filter-selected");
        w.dispatch(UiAction::Invoke { command: CommandId::Undo }); pump(180);
        layer(layer_ui::LayerAction::Select { id: before[0], mask: false });
        click(&mut input,&check(before[1]));
        let source = row(before[0]);
        let name = find_css(&source,"layer-name").unwrap();
        assert!(name.is_mapped(),"checked row name is visible");
        let start = screen_point(&name,&w.surface,[0.5,0.5]);
        let target = screen_point(&row(before[2]),&w.surface,[0.7,0.9]);
        let slop = gtk::Settings::default().unwrap().gtk_dnd_drag_threshold() as f32;
        input.perform(serde_json::json!([{"point":start},{"down":true},{"point":[start[0]-2.,start[1]]},{"point":[start[0]-slop*3.,start[1]]}]));
        let controllers = source.observe_controllers();
        let drag = (0..controllers.n_items()).find_map(|i| controllers.item(i).and_downcast::<gtk::DragSource>()).unwrap();
        assert!(drag.drag().is_some(),"checked row starts native pickup");
        input.perform(serde_json::json!([{"point":target},{"point":[target[0]+1.,target[1]]},{"down":false}]));
        assert_eq!(order(),[vec![before[2],before[0],before[1]],before[3..].to_vec()].concat(),"drag moves checked rows as one ordered block");
        w.dispatch(UiAction::Invoke { command: CommandId::Undo }); pump(180);
        assert_eq!(order(),before);
        layer(layer_ui::LayerAction::Select { id: before[0], mask: false });
    }
    for _ in 0..30 {
        w.dispatch(UiAction::Layer {
            action: layer_ui::LayerAction::New {
                group: false,
                clipped: false,
            },
        });
    }
    pump(300);
    for floating in [false, true] {
    for theme in [Theme::Light, Theme::Dark] {
    w.dispatch(UiAction::RestoreWorkspace { workspace: Box::new(layer_ui::WorkspaceState::default()) });
    if floating {
        w.dispatch(UiAction::MovePanel { panel: Panel::Layers, target: DockTarget::Float { position: [700., 200.] },
            viewport: [w.surface.width() as f32, w.surface.height() as f32] });
    }
    pump(250);
    w.dispatch(UiAction::SetTheme { theme: Some(theme) });
    pump(180);
    w.layer_panel.list.vadjustment().set_value(0.);
    pump(200);
    let scrolling_order = order();
    let bar = w.layer_panel.list.vscrollbar();
    let bar_point = screen_point(&bar, &w.surface, [0.5, 0.1]);
    input.perform(serde_json::json!([{"point":bar_point}]));
    pump(180);
    assert!(bar.can_target() && bar.opacity() == 1., "{theme:?}: mouse exposes scrollbar");
    let thumb = widgets(&bar).find(|node| node.css_name() == "slider").expect("native scrollbar thumb");
    let thumb_start = screen_point(&thumb, &w.surface, [0.5, 0.5]);
    input.perform(serde_json::json!([{"point":thumb_start},{"down":true},{"point":[thumb_start[0],thumb_start[1]+60.]},{"down":false}]));
    assert!(w.layer_panel.list.vadjustment().value() > 20., "{theme:?}: native mouse thumb scrolls overflowing list");
    assert_eq!(order(), scrolling_order);
    w.layer_panel.list.vadjustment().set_value(0.);
    pump(180);
    let source = row(scrolling_order[1]);
    let grip = source.last_child().unwrap();
    let gb = bounds(&grip);
    let bb = bounds(&bar);
    let left = gb.x().max(bb.x());
    let right = (gb.x()+gb.width()).min(bb.x()+bb.width());
    assert!(right > left, "{theme:?}: scrollbar overlaps grip edge");
    let start = [(left+right)/2., gb.y()+gb.height()/2.];
    assert!(bounds(w.layer_panel.list.upcast_ref()).contains_point(&gtk::graphene::Point::new(start[0],start[1])),
        "{theme:?}/{floating}: grip contact lies within list viewport");
    input.perform(serde_json::json!([{"point":start}]));
    input.perform(serde_json::json!([{"touch":"down", "point":start}]));
    assert!(!bar.can_target() && bar.opacity() == 0., "{theme:?}: first touch after mouse hides scrollbar");
    let picked = w.surface.pick(start[0] as f64, start[1] as f64, gtk::PickFlags::DEFAULT).unwrap();
    assert!(picked == grip || picked.is_ancestor(&grip), "{theme:?}: first touch picks grip through hidden scrollbar");
    let controllers = grip.observe_controllers();
    let drag = (0..controllers.n_items()).find_map(|i| controllers.item(i).and_downcast::<gtk::DragSource>()).unwrap();
    assert_eq!(drag.sequence_state(&drag.last_updated_sequence().unwrap()), gtk::EventSequenceState::Claimed,
        "{theme:?}/{floating}: first edge touch is owned by grip source");
    let slop = gtk::Settings::default().unwrap().gtk_dnd_drag_threshold() as f32;
    input.perform(serde_json::json!([{"touch":"move", "point":[start[0]-slop*3.,start[1]]}]));
    assert!(drag.drag().is_some(), "{theme:?}: overflowing list touch grip starts native DND");
    let target = screen_point(&row(scrolling_order[2]), &w.surface, [0.5, 0.9]);
    let viewport = bounds(w.layer_panel.list.upcast_ref());
    let hint = ui_session(&w).layer_drop_preview(scrolling_order[1], scrolling_order[2], 0.9, layer_ui::LayerDropSurface::Row)
        .expect("shared overflowing row drop is valid");
    assert!(viewport.contains_point(&gtk::graphene::Point::new(target[0],target[1])),
        "{theme:?}/{floating}: native drop point lies within list viewport");
    input.perform(serde_json::json!([{"touch":"move","point":target},{"touch":"move","point":[target[0]+1.,target[1]]}]));
    let target_row = row(hint.target);
    let class = match hint.position {
        layer_ui::LayerDropPosition::Above => "layer-drop-before",
        layer_ui::LayerDropPosition::Below => "layer-drop-after",
        layer_ui::LayerDropPosition::Into => "layer-drop-into",
        layer_ui::LayerDropPosition::Attach => "layer-drop-attach",
    };
    assert!(target_row.has_css_class(class), "{theme:?}/{floating}: native hint matches shared normalized drop {hint:?}; source={:?}",
        drag.drag().map(|drag|drag.selected_action()));
    assert_eq!(drag.drag().unwrap().selected_action(), gdk::DragAction::MOVE,
        "native compositor accepts overflowing grip drop");
    input.perform(serde_json::json!([{"touch":"up"}]));
    assert_ne!(order(), scrolling_order, "{theme:?}/{floating}: overflowing list touch grip commits reorder");
    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    pump(180);
    assert_eq!(order(), scrolling_order);
    w.dispatch(UiAction::Invoke { command: CommandId::Redo });
    pump(180);
    assert_ne!(order(), scrolling_order, "overflowing grip reorder has one-step redo");
    w.dispatch(UiAction::Invoke { command: CommandId::Undo });
    pump(180);
    let grip = row(scrolling_order[1]).last_child().unwrap();
    let start = screen_point(&grip, &w.surface, [0.5, 0.5]);
    let picked = w.surface.pick(start[0] as f64, start[1] as f64, gtk::PickFlags::DEFAULT).unwrap();
    assert!(picked == grip || picked.is_ancestor(&grip), "{theme:?}/{floating}: cancelled drag starts on grip");
    input.perform(serde_json::json!([{"touch":"down","point":start}]));
    let controllers = grip.observe_controllers();
    let drag = (0..controllers.n_items()).find_map(|i| controllers.item(i).and_downcast::<gtk::DragSource>()).unwrap();
    assert_eq!(drag.sequence_state(&drag.last_updated_sequence().unwrap()), gtk::EventSequenceState::Claimed,
        "{theme:?}/{floating}: cancel grip owns initial contact");
    input.perform(serde_json::json!([{"touch":"move","point":[start[0]-2.,start[1]]},{"touch":"move","point":[start[0]-slop*3.,start[1]]}]));
    assert!(drag.drag().is_some(), "{theme:?}/{floating}: cancel journey starts native touch grip DND");
    input.perform(serde_json::json!([{"touch":"move","point":target},{"key":65307,"down":true},{"key":65307,"down":false},{"touch":"up"}]));
    assert_eq!(order(), scrolling_order, "Escape cancels grip native DND");
    assert!(ui_session(&w).engine().can_redo(), "cancelled grip DND preserves history");
    input.perform(serde_json::json!([{"touch":"down","point":start},{"touch":"move","point":[start[0]-2.,start[1]]},{"touch":"move","point":[start[0]-slop*3.,start[1]]}]));
    assert!(drag.drag().is_some(), "{theme:?}/{floating}: lock journey starts native touch grip DND");
    let lock = |value| w.dispatch(UiAction::Layer { action: layer_ui::LayerAction::Lock { id: scrolling_order[1], value } });
    lock(true);
    pump(120);
    assert!(drag.drag().is_none(), "{theme:?}/{floating}: locking mid-drag retires native grip DND");
    input.perform(serde_json::json!([{"touch":"move","point":target},{"touch":"up"}]));
    pump(180);
    assert_eq!(order(), scrolling_order, "a retired grip DND preserves order");
    lock(false);
    pump(120);
    let b = bounds(&find_css(&row(scrolling_order[1]), "layer-name").unwrap());
    let start = [b.x() + b.width() / 2., b.y() + b.height() / 2.];
    input.perform(
        serde_json::json!([{"touch":"down","point":start},{"touch":"move","point":[start[0],start[1]-40.]},{"touch":"move","point":[start[0],start[1]-85.]},{"touch":"up"}]),
    );
    pump(800);
    assert!(
        w.layer_panel.list.vadjustment().value() > 20.,
        "unheld touch scrolls normally"
    );
    assert_eq!(order(), scrolling_order);
    assert!(
        !w.popovers
            .borrow()
            .iter()
            .filter_map(|p| p.upgrade())
            .any(|p| p.is_visible()),
        "scroll cancels the hold"
    );
    }
    }
    input.finish();
    w.window.destroy();
    pump(100);
}
