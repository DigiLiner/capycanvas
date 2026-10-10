//! Native controls shared by compact panels and export previews.
use adw::prelude::*;

pub const SPACING: i32 = layer_ui::PANEL_GROUP_SPACING as i32;
pub const CONTROL_SPACING: i32 = layer_ui::PANEL_CONTROL_SPACING as i32;
pub const COMPOUND_SPACING: i32 = layer_ui::PANEL_COMPOUND_SPACING as i32;

pub fn column() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, SPACING)
}

/// Native action/choice row using the same compact height as panel numbers.
pub fn action_row(spacing: i32) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
    row.add_css_class("panel-control-row");
    row
}

/// Native menu choice, sized like the adjacent dropdowns. Long profile names
/// ellipsize instead of widening or clipping the panel; the tooltip keeps them.
pub fn menu_choice(button: &gtk::MenuButton, text: &str) {
    button.set_label(text);
    button.set_always_show_arrow(true);
    button.set_can_shrink(true);
    button.add_css_class("panel-choice");
}

pub fn check(label: &str) -> gtk::CheckButton {
    let check = gtk::CheckButton::new();
    check.connect_label_notify(|check| {
        if let Some(label) = std::iter::successors(check.first_child(), |child| child.next_sibling())
            .find_map(|child| child.downcast::<gtk::Label>().ok()) {
            label.set_wrap(true);
            label.set_xalign(0.);
            label.set_max_width_chars(1);
        }
    });
    check.set_label(Some(label));
    check
}

/// Selected text can shrink in compact rows; the popup retains full labels.
pub fn dropdown(choices: &[&str]) -> gtk::DropDown {
    let dropdown = gtk::DropDown::from_strings(choices);
    dropdown.set_factory(Some(&choice_factory(true)));
    dropdown.set_list_factory(Some(&choice_factory(false)));
    dropdown
}

pub fn command_dropdown(commands: &[&layer_ui::CommandState]) -> gtk::DropDown {
    let model = gtk::gio::ListStore::new::<gtk::glib::BoxedAnyObject>();
    let dropdown = gtk::DropDown::new(Some(model), None::<gtk::Expression>);
    dropdown.set_factory(Some(&choice_factory(true)));
    dropdown.set_list_factory(Some(&choice_factory(false)));
    update_commands(&dropdown, commands);
    dropdown
}

pub fn update_commands(dropdown: &gtk::DropDown, commands: &[&layer_ui::CommandState]) {
    let model = dropdown.model().unwrap().downcast::<gtk::gio::ListStore>().unwrap();
    if model.n_items() != commands.len() as u32 || commands.iter().enumerate().any(|(i, command)| {
        let item = model.item(i as u32).unwrap().downcast::<gtk::glib::BoxedAnyObject>().unwrap();
        let old = item.borrow::<(std::sync::Arc<str>, bool)>();
        old.0 != command.label || old.1 != command.enabled
    }) {
        model.splice(0, model.n_items(), &commands.iter().map(|command| gtk::glib::BoxedAnyObject::new((command.label.clone(), command.enabled))).collect::<Vec<_>>());
    }
}

fn choice_factory(shrink: bool) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::new(None);
        label.set_xalign(0.);
        if shrink {
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_width_chars(1);
        }
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, object| {
        let item = object.downcast_ref::<gtk::ListItem>().unwrap();
        let value = item.item().unwrap();
        let (text, enabled) = if let Some(value) = value.downcast_ref::<gtk::StringObject>() { (value.string().to_string(), true) }
        else { let value = value.downcast_ref::<gtk::glib::BoxedAnyObject>().unwrap().borrow::<(std::sync::Arc<str>, bool)>(); (value.0.to_string(), value.1) };
        let label = item.child().and_downcast::<gtk::Label>().unwrap();
        label.set_label(&text);
        label.set_tooltip_text(Some(&text));
        label.set_sensitive(enabled);
        item.set_selectable(enabled);
        item.set_activatable(enabled);
    });
    factory
}

/// A mutually exclusive choice with native keyboard, pointer and accessibility
/// behavior. Names are stable model identifiers; labels are presentation only.
pub fn segmented(name: &str, choices: &[(&str, &str)]) -> adw::ToggleGroup {
    let group = adw::ToggleGroup::builder()
        .homogeneous(true)
        .can_shrink(true)
        .hexpand(true)
        .build();
    group.set_widget_name(name);
    group.add_css_class("flat");
    for &(id, label) in choices {
        group.add(adw::Toggle::builder().name(id).label(label).build());
    }
    group
}

/// Standard single-line label/control arrangement, also used for compact
/// numeric controls. No panel-specific drawing, input handling or CSS.
pub fn row(title: &str, control: &impl IsA<gtk::Widget>, spacing: i32) -> gtk::Box {
    let row = action_row(spacing);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.);
    label.set_width_chars(9);
    label.set_mnemonic_widget(Some(control));
    row.append(&label);
    control.set_hexpand(true);
    row.append(control);
    row
}
