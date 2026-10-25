use super::*;

pub(super) struct WorkspaceRowUi {
    pub row: gtk::ListBoxRow,
    pin: gtk::ToggleButton,
    title: gtk::Label,
    subtitle: gtk::Label,
    current: gtk::Image,
    handle: gtk::MenuButton,
    entry: gtk::Entry,
    name: gtk::Stack,
    pub swipe: crate::swipe_row::SwipeRow,
    context: gtk::Popover,
    editing: Cell<bool>,
    submitted: Cell<bool>,
}
impl Drop for WorkspaceRowUi {
    fn drop(&mut self) {
        if self.context.parent().is_some() { self.context.unparent(); }
    }
}
impl WorkspaceRowUi {
    pub(super) fn update(&self, w: &Rc<Workspace>, item: &layer_workspace::WorkspaceRow) {
        self.title.set_text(&item.title);
        self.subtitle.set_text(&item.subtitle);
        self.subtitle.set_visible(!item.subtitle.is_empty());
        self.current.set_visible(item.current);
        let caption = layer_ui::NativeCaption::Reorder { title: item.title.clone() }.message(&w.localization());
        self.handle.set_tooltip_text(Some(&caption));
        self.handle.update_property(&[gtk::accessible::Property::Label(&caption)]);
        self.entry.set_placeholder_text(Some(&w.localization().text(layer_ui::MessageId::COMMON_NAME)));
        self.entry.update_property(&[gtk::accessible::Property::Label(&w.localization().text(layer_ui::MessageId::COMMON_NAME))]);
        if let Some(pin) = &item.pin {
            self.pin.set_active(pin.selected);
            self.pin.update_state(&[gtk::accessible::State::Disabled(!pin.enabled)]);
            self.pin.set_opacity(if pin.enabled { 1. } else { 0.5 });
            self.pin.set_tooltip_text(Some(&pin.label));
            self.pin.update_property(&[gtk::accessible::Property::Label(&pin.label)]);
            crate::icons::set_button(&self.pin, &format!("layer-{}-symbolic", pin.icon));
        }
        let delete = item.options.iter().find(|button| matches!(button.action, ManagerAction::Delete(_)));
        self.swipe.delete_button().set_visible(delete.is_some());
        self.swipe.delete_button().set_sensitive(delete.is_some_and(|button| button.enabled));
        self.swipe.delete_button().child().and_downcast::<gtk::Label>().unwrap().set_text(&w.localization().text(layer_ui::MessageId::COMMON_DELETE));
        if delete.is_none() && self.swipe.is_open() { self.swipe.reveal(false); }
    }
}

impl ManagerUi {
    pub(super) fn workspace_row(&self, w: &Rc<Workspace>, item: &layer_workspace::WorkspaceRow) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.add_css_class("workspace-manager-row");
        row.set_widget_name(&format!("workspace-row-{}", item.id));
        row.set_activatable(true);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        content.add_css_class("workspace-row-content");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some(&item.title));
        title.set_widget_name(&format!("workspace-name-{}", item.id));
        title.add_css_class("workspace-name");
        title.set_xalign(0.);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_width_chars(1);
        let entry = gtk::Entry::builder().has_frame(false).width_chars(1).build();
        entry.add_css_class("workspace-name-entry");
        entry.set_placeholder_text(Some(&w.localization().text(layer_ui::MessageId::COMMON_NAME)));
        entry.update_property(&[gtk::accessible::Property::Label(&w.localization().text(layer_ui::MessageId::COMMON_NAME))]);
        entry.set_widget_name(&format!("workspace-rename-{}", item.id));
        let composition = crate::input::guard_entry_activation(&entry);
        let name = gtk::Stack::new();
        name.set_hhomogeneous(false);
        name.set_vhomogeneous(true);
        name.add_named(&title, Some("name"));
        name.add_named(&entry, Some("edit"));
        text.append(&name);
        let subtitle = gtk::Label::new(Some(&item.subtitle));
        subtitle.set_xalign(0.);
        subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("subtitle");
        subtitle.set_visible(!item.subtitle.is_empty());
        text.append(&subtitle);
        content.append(&text);
        let current = crate::icons::image("layer-check-symbolic");
        current.set_visible(item.current);
        content.append(&current);
        let pin = gtk::ToggleButton::new();
        pin.set_widget_name(&format!("workspace-pin-{}", item.id));
        pin.add_css_class("flat");
        pin.add_css_class("workspace-pin");
        pin.set_size_request(34, 34);
        pin.set_valign(gtk::Align::Center);
        let id = item.id.clone();
        pin.connect_clicked(glib::clone!(#[weak] w, #[strong] id, move |button| {
            if let Some(pin) = w.workspaces.view().rows.iter().find(|row| row.id == id).and_then(|row| row.pin.clone()) {
                button.set_active(pin.selected);
                if pin.enabled { w.workspaces.send(&w, WorkspaceInput::EditSwitcher { edit: pin.edit }); }
            }
        }));
        content.append(&pin);
        let caption = layer_ui::NativeCaption::Reorder { title: item.title.clone() }.message(&w.localization());
        let handle = gtk::MenuButton::new();
        handle.add_css_class("workspace-reorder-handle");
        handle.add_css_class("drag-immediate");
        handle.set_widget_name(&format!("workspace-reorder-handle-{}", item.id));
        handle.set_child(Some(&crate::icons::image("layer-grip-symbolic")));
        handle.set_tooltip_text(Some(&caption));
        handle.update_property(&[gtk::accessible::Property::Label(&caption)]);
        handle.set_size_request(16, 34);
        handle.set_valign(gtk::Align::Center);
        handle.set_cursor_from_name(Some("grab"));
        let reorder = actions_popover(w, Vec::new());
        self.add_switcher_actions(w, &reorder, &id);
        handle.set_popover(Some(&reorder));
        content.append(&handle);
        let context = actions_popover(w, Vec::new());
        context.set_widget_name(&format!("workspace-context-{id}"));
        context.connect_show(glib::clone!(#[weak] w, #[strong] id, move |popup| {
            clear(&popup.child().and_downcast::<gtk::Box>().unwrap());
            let actions = w.workspaces.view().rows.into_iter().find(|row| row.id == id).map_or_else(Vec::new, |row| row.options);
            append_actions(&w, popup, actions);
            w.workspaces.ui.add_switcher_actions(&w, popup, &id);
        }));
        context.set_parent(&row);
        let swipe = crate::swipe_row::SwipeRow::new(&content,
            glib::clone!(#[weak] w, #[strong] id, move || {
                if w.workspaces.ui.finish_active_workspace_name(&w) {
                    w.workspaces.send(&w, WorkspaceInput::Action { action: ManagerAction::Delete(id.clone()) });
                }
            }),
            || {}, || {}, || {},
            glib::clone!(#[weak] w, move |opened| {
                let rows: Vec<_> = w.workspaces.ui.workspace_rows.borrow().values().cloned().collect();
                for row in rows { if row.swipe != *opened && row.swipe.is_open() { row.swipe.reveal(false); } }
            }),
            glib::clone!(#[weak] handle, move |row| handle.set_opacity(1. - row.swipe_progress())));
        swipe.lock_button().set_visible(false);
        swipe.duplicate_button().set_visible(false);
        row.set_child(Some(&swipe));
        self.bind_reorder_row(w, &row, &id, &handle, &context);
        entry.connect_activate(glib::clone!(#[weak] w, #[strong] id, move |entry| w.workspaces.ui.finish_workspace_name(&w, &id, entry, false)));
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(glib::clone!(#[weak] w, #[weak] entry, #[strong] id, move |_| {
            glib::idle_add_local_once(glib::clone!(#[weak] w, #[weak] entry, #[strong] id, move || {
                w.workspaces.ui.finish_workspace_name(&w, &id, &entry, false);
            }));
        }));
        entry.add_controller(focus);
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(#[weak] w, #[weak] entry, #[strong] id, #[upgrade_or] glib::Propagation::Proceed, move |_, key, _, _| {
            if key == gdk::Key::Escape {
                if !composition.active() { w.workspaces.ui.finish_workspace_name(&w, &id, &entry, true); }
                glib::Propagation::Stop
            } else { glib::Propagation::Proceed }
        }));
        entry.add_controller(keys);
        let ui = Rc::new(WorkspaceRowUi { row: row.clone(), pin, title, subtitle, current, handle, entry, name, swipe, context, editing: Cell::new(false), submitted: Cell::new(false) });
        ui.update(w, item);
        self.workspace_rows.borrow_mut().insert(id, ui);
        row
    }

    pub(super) fn workspace_name_action(&self, w: &Rc<Workspace>, id: &str) -> Option<ManagerAction> {
        w.workspaces.view().rows.into_iter().find(|row| row.id == id)
            .and_then(|row| row.options.into_iter().find(|button| button.enabled && matches!(button.action, ManagerAction::Rename(_))))
            .map(|button| button.action)
    }
    pub(super) fn close_workspace_swipes(&self) {
        let rows: Vec<_> = self.workspace_rows.borrow().values().cloned().collect();
        for row in rows { if row.swipe.is_open() { row.swipe.reveal(false); } }
    }
    fn finish_workspace_name(&self, w: &Rc<Workspace>, id: &str, entry: &gtk::Entry, cancel: bool) {
        let row = self.workspace_rows.borrow().get(id).cloned();
        let Some(row) = row.filter(|row| row.entry == *entry && row.editing.get() && !row.submitted.replace(true)) else { return; };
        w.workspaces.send(w, if cancel { WorkspaceInput::Cancel } else {
            WorkspaceInput::Submit { name: row.entry.text().trim().into(), description: None, choice: None }
        });
    }
    pub(super) fn finish_active_workspace_name(&self, w: &Rc<Workspace>) -> bool {
        let editing = self.workspace_rows.borrow().iter().find_map(|(id, row)| row.editing.get().then(|| (id.clone(), row.entry.clone())));
        let Some((id, entry)) = editing else { return true; };
        self.finish_workspace_name(w, &id, &entry, false);
        let view = w.workspaces.view();
        view.busy || !matches!(view.prompt_action, Some(ManagerAction::Rename(_)))
    }
    pub(super) fn render_inline_rename(&self, view: &WorkspaceView) -> bool {
        let editing = match &view.prompt_action { Some(ManagerAction::Rename(id)) if view.page == Some(ManagerPage::Workspaces) => Some(id), _ => None };
        let rows: Vec<_> = self.workspace_rows.borrow().iter().map(|(id, row)| (id.clone(), row.clone())).collect();
        for (id, row) in rows {
            if editing == Some(&id) {
                if !row.editing.replace(true) {
                    row.entry.set_text(view.prompt.as_ref().and_then(|prompt| prompt.name.as_deref()).unwrap_or(""));
                    row.name.set_visible_child_name("edit");
                    let entry = row.entry.clone();
                    glib::idle_add_local_once(glib::clone!(#[weak] entry, move || { entry.grab_focus(); entry.select_region(0, -1); }));
                }
                if !view.busy { row.submitted.set(false); }
                if view.error.is_some() { row.entry.add_css_class("error"); } else { row.entry.remove_css_class("error"); }
            } else if row.editing.replace(false) {
                row.name.set_visible_child_name("name");
                row.submitted.set(false);
            }
        }
        editing.is_some()
    }
}
