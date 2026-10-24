fn main() -> gtk::glib::ExitCode {
    unsafe { layer_linux::enable_gtk_color_management() };
    layer_linux::run()
}
