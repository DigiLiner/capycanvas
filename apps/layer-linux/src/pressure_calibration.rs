use gtk::{glib, prelude::*};
use layer_ui::{Bounds, ContactPhase, CurveEditorAction, CurveEditorTarget, PressureCalibrationAction, PressureCalibrationView, TiltCalibrationAction, TiltCalibrationView, UiAction};
use std::{cell::{Cell, RefCell}, rc::Rc};
use crate::{curve_editor::CurveEditor, workspace::{PanelColumns, Workspace}};

pub(crate) struct PressurePanel {
    pub root: PanelColumns,
    column: gtk::Box,
    title: gtk::Label,
    close: gtk::Button,
    body: gtk::Box,
    device: gtk::Label,
    direction: gtk::Box,
    direction_title: gtk::Label,
    arrows: gtk::DrawingArea,
    legend: gtk::Label,
    help: gtk::Label,
    axes: [gtk::CheckButton;3],
    tilt: Cell<bool>,
    projecting: Cell<bool>,
    tilt_view: RefCell<Option<TiltCalibrationView>>,
    firmer: gtk::Button,
    lighter: gtk::Button,
    reset: gtk::Button,
    cancel: gtk::Button,
    apply: gtk::Button,
    editor: RefCell<Option<CurveEditor>>,
    bounds: Cell<Option<Bounds>>,
    pending_bounds: Cell<Option<Bounds>>,
    presented: RefCell<Option<PressureCalibrationView>>,
    measured: Cell<Option<([f32; 2], [f32; 2])>>,
    bound: Cell<bool>,
}

impl PressurePanel {
    pub fn new() -> Rc<Self> {
        let column = gtk::Box::builder().orientation(gtk::Orientation::Vertical).accessible_role(gtk::AccessibleRole::Dialog).build();
        let root = PanelColumns::new(&column);
        root.add_css_class("floating-panel"); root.add_css_class("utility-panel"); root.set_widget_name("pen-pressure-panel");
        root.set_visible(false);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header.add_css_class("dock-tabs"); header.add_css_class("utility-header"); header.set_height_request(layer_ui::TAB_BAR_HEIGHT as i32);
        let title = gtk::Label::builder().hexpand(true).xalign(0.).margin_start(12).ellipsize(gtk::pango::EllipsizeMode::End).build();
        title.set_widget_name("pen-pressure-title"); title.set_cursor_from_name(Some("grab"));
        title.add_css_class("utility-title");
        let close = crate::icons::button("layer-window-close-symbolic"); close.add_css_class("flat"); close.set_widget_name("pen-pressure-close");
        close.add_css_class("utility-close"); close.set_valign(gtk::Align::Center);
        close.set_margin_start(4); close.set_margin_end(6); close.set_margin_top(4); close.set_margin_bottom(4);
        header.append(&title); header.append(&close); column.append(&header);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.set_margin_start(12); body.set_margin_end(12); body.set_margin_top(12); body.set_margin_bottom(12);
        let scroller = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true).vexpand(true).child(&body).build();
        column.append(&scroller);
        let sensitivity = gtk::Box::new(gtk::Orientation::Horizontal, 6); sensitivity.set_homogeneous(true);
        let firmer = gtk::Button::new(); firmer.set_widget_name("pen-pressure-firmer");
        let lighter = gtk::Button::new(); lighter.set_widget_name("pen-pressure-lighter");
        sensitivity.append(&firmer); sensitivity.append(&lighter); body.append(&sensitivity);
        let device = gtk::Label::builder().xalign(0.).wrap(true).build(); device.add_css_class("dim-label"); body.prepend(&device);
        let direction = gtk::Box::new(gtk::Orientation::Vertical,6);
        let direction_title = gtk::Label::builder().xalign(0.).build(); direction_title.add_css_class("heading"); direction.append(&direction_title);
        let arrows = gtk::DrawingArea::builder().content_height(72).hexpand(true).build(); arrows.set_widget_name("pen-tilt-direction"); direction.append(&arrows);
        let legend = gtk::Label::new(None); direction.append(&legend);
        let axes = std::array::from_fn(|_|gtk::CheckButton::new());
        for button in &axes {direction.append(button);}
        let help = gtk::Label::builder().wrap(true).xalign(0.).build(); help.add_css_class("dim-label"); direction.append(&help); body.append(&direction);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        footer.set_margin_start(12); footer.set_margin_end(12); footer.set_margin_bottom(12);
        let reset = gtk::Button::new(); reset.set_widget_name("pen-pressure-reset"); reset.add_css_class("flat");
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0); spacer.set_hexpand(true);
        let cancel = gtk::Button::new(); cancel.set_widget_name("pen-pressure-cancel");
        let apply = gtk::Button::new(); apply.set_widget_name("pen-pressure-apply"); apply.add_css_class("suggested-action");
        footer.append(&reset); footer.append(&spacer); footer.append(&cancel); footer.append(&apply); column.append(&footer);
        Rc::new(Self { root, column, title, close, body, device, direction, direction_title, arrows, legend, help, axes,
            tilt:Cell::new(false), projecting:Cell::new(false), tilt_view:RefCell::new(None), firmer, lighter, reset, cancel, apply,
            editor: RefCell::new(None), bounds: Cell::new(None), pending_bounds: Cell::new(None), presented: RefCell::new(None), measured: Cell::new(None), bound: Cell::new(false) })
    }
    pub fn bounds(&self) -> Option<Bounds> { self.bounds.get() }
    fn panel_action(&self,action:PressureCalibrationAction)->UiAction {
        if self.tilt.get() {UiAction::TiltCalibration{action:TiltCalibrationAction::Panel{action}}} else {UiAction::PressureCalibration{action}}
    }
    fn target(&self)->CurveEditorTarget {if self.tilt.get(){CurveEditorTarget::Tilt}else{CurveEditorTarget::Pressure}}
    fn bind(self:&Rc<Self>, w: &Rc<Workspace>) {
        if self.bound.replace(true) { return; }
        for (button, action) in [(&self.close, PressureCalibrationAction::Cancel), (&self.cancel, PressureCalibrationAction::Cancel),
            (&self.apply, PressureCalibrationAction::Apply), (&self.firmer, PressureCalibrationAction::Sensitivity { lighter: false }),
            (&self.lighter, PressureCalibrationAction::Sensitivity { lighter: true })] {
            button.connect_clicked(glib::clone!(#[weak] w, move |_| { w.dispatch(w.pressure_calibration.panel_action(action.clone())); }));
        }
        self.reset.connect_clicked(glib::clone!(#[weak] w, move |_| {
            w.dispatch(UiAction::CurveEditor { target: w.pressure_calibration.target(), action: CurveEditorAction::Reset });
        }));
        for (button,name) in self.axes.iter().zip(["reverse-horizontal","reverse-vertical","swap-axes"]) {
            button.set_widget_name(&format!("pen-tilt-{name}"));
            button.connect_toggled(glib::clone!(#[weak(rename_to=panel)] self, #[weak] w, move |_| {
                if panel.projecting.get(){return;}
                let axes=layer_core::TiltAxes{reverse_horizontal:panel.axes[0].is_active(),reverse_vertical:panel.axes[1].is_active(),swap:panel.axes[2].is_active()};
                w.dispatch(UiAction::TiltCalibration{action:TiltCalibrationAction::Axes{axes}});
            }));
        }
        self.arrows.set_draw_func(glib::clone!(#[weak(rename_to=panel)] self, move |area,context,width,height| {
            let view=panel.tilt_view.borrow();let Some(view)=view.as_ref()else{return};
            let (cx,cy)=(width as f64/2.,height as f64/2.);let radius=height.min(width) as f64/2.-5.;
            let color=area.color();context.set_source_rgba(color.red() as f64,color.green() as f64,color.blue() as f64,0.18);
            context.arc(cx,cy,radius,0.,std::f64::consts::TAU);let _=context.stroke();
            for (vector,adjusted) in [(view.input_direction,false),(view.adjusted_direction,true)] {
                let Some([x,y])=vector else{continue};
                if adjusted {context.set_source_rgb(0.16,0.52,0.95);}else{context.set_source_rgba(color.red() as f64,color.green() as f64,color.blue() as f64,0.55);}
                let (x,y)=(x as f64*radius,y as f64*radius);context.set_line_width(if adjusted{2.5}else{1.5});
                context.move_to(cx,cy);context.line_to(cx+x,cy+y);let _=context.stroke();
                let length=x.hypot(y);if length>3. {let (ux,uy)=(x/length,y/length);context.move_to(cx+x-ux*6.-uy*3.,cy+y-uy*6.+ux*3.);context.line_to(cx+x,cy+y);context.line_to(cx+x-ux*6.+uy*3.,cy+y-uy*6.-ux*3.);let _=context.stroke();}
            }
        }));
        let drag = gtk::GestureDrag::new(); drag.set_button(1);
        let capture = Rc::new(Cell::new(false));
        drag.connect_drag_begin(glib::clone!(#[weak] w, #[strong] capture, move |gesture, _, _| {
            let Some((x,y)) = gesture.current_event().and_then(|event| event.position()) else { return; };
            capture.set(true); gesture.set_state(gtk::EventSequenceState::Claimed);
            w.dispatch(w.pressure_calibration.panel_action(PressureCalibrationAction::Drag { phase: ContactPhase::Down,
                position: [x as f32,y as f32], viewport: [w.surface.width() as f32,w.surface.height() as f32] }));
        }));
        for (phase, ending) in [(ContactPhase::Move,false),(ContactPhase::Up,true)] {
            let callback = glib::clone!(#[weak] w, #[strong] capture, move |gesture: &gtk::GestureDrag, _: f64, _: f64| {
                if !capture.get() { return; }
                if ending { capture.set(false); }
                let position = gesture.current_event().and_then(|event|event.position());
                let phase = if position.is_some() { phase } else { ContactPhase::Cancel };
                let (x,y) = position.unwrap_or_default();
                w.dispatch(w.pressure_calibration.panel_action(PressureCalibrationAction::Drag { phase, position: [x as f32,y as f32],
                    viewport: [w.surface.width() as f32,w.surface.height() as f32] }));
                if ending { w.pressure_calibration.present_motion(&w); }
            });
            if ending { drag.connect_drag_end(callback); } else { drag.connect_drag_update(callback); }
        }
        drag.connect_cancel(glib::clone!(#[weak] w, move |_, _| {
            if capture.replace(false) { w.dispatch(w.pressure_calibration.panel_action(PressureCalibrationAction::Drag {
                phase: ContactPhase::Cancel, position: [0.;2], viewport: [w.surface.width() as f32,w.surface.height() as f32] })); }
            w.pressure_calibration.present_motion(&w);
        }));
        self.title.add_controller(drag);
    }
    pub fn refresh(self: &Rc<Self>, w: &Rc<Workspace>, pressure: Option<&PressureCalibrationView>, tilt:Option<&TiltCalibrationView>) {
        self.bind(w);
        let changed_mode=self.tilt.replace(tilt.is_some())!=tilt.is_some();
        let tilt_changed=self.tilt_view.borrow().as_ref()!=tilt;
        if changed_mode {
            let previous=self.editor.borrow_mut().take();
            if let Some(editor)=previous {self.body.remove(&editor.root);}
            self.presented.borrow_mut().take();self.measured.set(None);
        }
        *self.tilt_view.borrow_mut()=tilt.cloned();
        let view=tilt.map(|v|&v.panel).or(pressure);
        let Some(view) = view else {
            self.bounds.set(None); self.pending_bounds.set(None); self.presented.borrow_mut().take(); self.root.set_visible(false); self.measured.set(None); return;
        };
        let moved = self.bounds.replace(Some(view.bounds)) != Some(view.bounds);
        if moved { self.pending_bounds.set(Some(view.bounds)); w.queue_workspace_frame(); w.surface.queue_draw(); }
        let viewport = [w.surface.width() as f32, w.surface.height() as f32];
        {
            let mut presented = self.presented.borrow_mut();
            if let Some(previous) = &mut *presented { previous.bounds = view.bounds; }
            if !tilt_changed && presented.as_ref() == Some(view) && self.measured.get().is_some_and(|(_, previous)| previous == viewport) { return; }
            *presented = Some(view.clone());
        }
        self.root.set_visible(true);
        let prefix=if self.tilt.get(){"pen-tilt"}else{"pen-pressure"};
        for (widget,suffix) in [(self.root.upcast_ref::<gtk::Widget>(),"panel"),(self.title.upcast_ref(),"title"),(self.close.upcast_ref(),"close"),
            (self.firmer.upcast_ref(),"firmer"),(self.lighter.upcast_ref(),"lighter"),(self.reset.upcast_ref(),"reset"),
            (self.cancel.upcast_ref(),"cancel"),(self.apply.upcast_ref(),"apply")] {widget.set_widget_name(&format!("{prefix}-{suffix}"));}
        self.device.set_visible(tilt.is_some()); self.direction.set_visible(tilt.is_some());
        let ready=tilt.is_none_or(|v|v.ready);self.reset.set_sensitive(ready);self.apply.set_sensitive(ready);
        self.projecting.set(true);
        if let Some(tilt)=tilt {
            self.device.set_label(&tilt.device);self.direction_title.set_label(&tilt.direction);
            self.legend.set_label(&format!("{} · {}",tilt.input,tilt.adjusted));self.help.set_label(&tilt.help);
            for (i,(label,active)) in [(&tilt.reverse_horizontal,tilt.axes.reverse_horizontal),(&tilt.reverse_vertical,tilt.axes.reverse_vertical),(&tilt.swap_axes,tilt.axes.swap)].into_iter().enumerate(){
                self.axes[i].set_label(Some(label));self.axes[i].set_active(active);self.axes[i].set_sensitive(ready);
            }
            self.arrows.queue_draw();
        }
        self.projecting.set(false);
        self.title.set_label(&view.title); self.column.update_property(&[gtk::accessible::Property::Label(&view.title),gtk::accessible::Property::Modal(false)]);
        self.close.set_tooltip_text(Some(&view.close)); self.close.update_property(&[gtk::accessible::Property::Label(&view.close)]);
        set_button_content(&self.reset, "layer-reset-symbolic", &view.reset);
        set_button_content(&self.firmer, "layer-minus-symbolic", &view.firmer);
        set_button_content(&self.lighter, "layer-plus-symbolic", &view.lighter);
        self.firmer.set_sensitive(view.firmer_enabled); self.lighter.set_sensitive(view.lighter_enabled);
        self.cancel.set_label(&view.cancel); self.apply.set_label(&view.apply);
        if self.editor.borrow().is_none() {
            let editor = CurveEditor::new(w, self.target(), prefix, &view.title, &view.editor);
            self.body.insert_child_after(&editor.root,Some(&self.device)); *self.editor.borrow_mut() = Some(editor);
        }
        self.editor.borrow().as_ref().unwrap().root.set_sensitive(ready);
        self.editor.borrow().as_ref().unwrap().update(&view.editor, &w.localization());
        let width = 336f32.max(self.column.measure(gtk::Orientation::Horizontal,-1).0 as f32).min(viewport[0]);
        let extent = [width,360f32.max(self.column.measure(gtk::Orientation::Vertical,width as i32).1 as f32).min(viewport[1])];
        if self.measured.get() != Some((extent, viewport)) {
            self.measured.set(Some((extent, viewport)));
            let action=self.panel_action(PressureCalibrationAction::Measure{extent,viewport});
            glib::idle_add_local_once(glib::clone!(#[weak] w, move || {w.dispatch(action);}));
        }
    }
    pub(super) fn present_motion(&self, w: &Workspace) {
        if let Some(bounds) = self.pending_bounds.take() {
            crate::workspace::allocate_at(self.root.upcast_ref(), bounds); w.surface.queue_draw();
        }
    }
}

fn set_button_content(button: &gtk::Button, icon: &str, label: &str) {
    if button.child().and_then(|w|w.downcast::<gtk::Box>().ok()).and_then(|b|b.last_child())
        .and_then(|w|w.downcast::<gtk::Label>().ok()).is_some_and(|l|l.label()==label) { return; }
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6); content.set_halign(gtk::Align::Center);
    content.append(&gtk::Image::from_icon_name(icon)); content.append(&gtk::Label::new(Some(label))); button.set_child(Some(&content));
}
