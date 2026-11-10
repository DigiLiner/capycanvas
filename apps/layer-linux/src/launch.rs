use gtk::prelude::*;

pub(crate) struct LaunchSplash {
    pub root: gtk::Revealer,
    label: gtk::Label,
}
impl LaunchSplash {
    pub fn new(localization: &layer_ui::Localizer) -> Self {
        let root = gtk::Revealer::new();
        root.set_widget_name("launch-splash");
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        root.set_transition_duration(0);
        let background = gtk::Overlay::new();
        background.add_css_class("launch-splash");
        let icon = crate::icons::image("layer-zen-looking-up-symbolic");
        icon.set_pixel_size(96);
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Center);
        background.set_child(Some(&icon));
        let label = gtk::Label::new(Some(&layer_ui::bootstrap_view(localization).starting_canvas));
        label.set_opacity(0.45);
        label.set_wrap(true);
        label.set_justify(gtk::Justification::Center);
        label.set_max_width_chars(48);
        let caption = gtk::Revealer::new();
        caption.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        caption.set_transition_duration(150);
        caption.set_child(Some(&label));
        caption.set_halign(gtk::Align::Center);
        caption.set_valign(gtk::Align::Center);
        caption.set_margin_top(160);
        background.add_overlay(&caption);
        root.set_child(Some(&background));
        root.set_reveal_child(true);
        let weak = caption.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(layer_ui::LAUNCH_CAPTION_DELAY_MS), move || {
            if let Some(caption) = weak.upgrade() { caption.set_reveal_child(true); }
        });
        Self {root, label}
    }
    pub fn update(&self, view: layer_ui::LaunchView, localization: &layer_ui::Localizer) {
        if !view.visible && !self.root.reveals_child() { return; }
        if view.visible {
            self.label.set_text(&localization.text(match view.notice {
                layer_ui::CanvasNotice::Canvas => layer_ui::MessageId::COMMON_PREPARING_CANVAS,
                layer_ui::CanvasNotice::Brush => layer_ui::MessageId::COMMON_PREPARING_BRUSH,
            }));
        }
        self.root.set_transition_duration(if view.finished {180} else {0});
        self.root.set_can_target(view.visible);
        self.root.set_reveal_child(view.visible);
    }
    pub fn bypass(&self) {
        self.root.set_transition_duration(0);
        self.root.set_reveal_child(false);
        self.root.set_can_target(false);
    }
    #[cfg(test)]
    pub fn covering(&self) -> bool { self.root.reveals_child() || self.root.is_child_revealed() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::tests::{native_test_app, pump};

    #[test]
    #[ignore = "private Wayland display: splash caption, reveal and error bypass"]
    fn native_launch_splash_timing_and_bypass() {
        let app = native_test_app("art.capycanvas.LaunchSplash");
        crate::icons::register();
        let localization = layer_ui::Localizer::shared(layer_ui::UiLanguage::English);
        let started = std::time::Instant::now();
        let splash = LaunchSplash::new(&localization);
        let window = gtk::ApplicationWindow::new(&*app);
        window.set_default_size(400, 300);
        window.set_child(Some(&splash.root));
        window.present();
        pump(100);
        let caption = splash.label.parent().unwrap().downcast::<gtk::Revealer>().unwrap();
        let capture = |name: &str| {
            if let Some(directory) = std::env::var_os("LAYER_TEST_ARTIFACTS") {
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).unwrap();
                crate::snapshot_window(&window, 1.).save_to_png(directory.join(name)).unwrap();
            }
        };
        assert!(splash.covering());
        let icon = splash.root.child().unwrap().downcast::<gtk::Overlay>().unwrap().child().unwrap();
        let bounds = icon.compute_bounds(&window).unwrap();
        let surface = splash.root.compute_bounds(&window).unwrap();
        assert!((bounds.x() + bounds.width() * 0.5 - (surface.x() + surface.width() * 0.5)).abs() <= 1.);
        assert!((bounds.y() + bounds.height() * 0.5 - (surface.y() + surface.height() * 0.5)).abs() <= 1.);
        assert!(!caption.reveals_child());
        let delay = std::time::Duration::from_millis(layer_ui::LAUNCH_CAPTION_DELAY_MS);
        let before = delay.saturating_sub(std::time::Duration::from_millis(100));
        pump(before.saturating_sub(started.elapsed()).as_millis() as u64);
        assert!(started.elapsed() < delay);
        assert!(!caption.reveals_child(), "only the mark appears before the shared caption deadline");
        capture("launch-before-caption.png");
        pump(delay.saturating_sub(started.elapsed()).as_millis() as u64 + 200);
        assert!(caption.reveals_child());
        assert_eq!(icon.compute_bounds(&window).unwrap(), bounds, "caption reveal keeps the mark centered");
        capture("launch-after-caption.png");
        let view = layer_ui::LaunchView {visible: false, finished: true, notice: layer_ui::CanvasNotice::Canvas, frame: None};
        splash.update(view, &localization);
        assert_eq!(splash.root.transition_duration(), 180);
        assert!(splash.covering());
        pump(240);
        assert!(!splash.covering());
        splash.update(layer_ui::LaunchView {visible: true, finished: false, ..view}, &localization);
        assert!(splash.covering());
        splash.update(layer_ui::LaunchView {finished: false, ..view}, &localization);
        assert_eq!(splash.root.transition_duration(), 0);
        pump(10);
        assert!(!splash.covering());
        window.destroy();
        pump(20);
    }
}
