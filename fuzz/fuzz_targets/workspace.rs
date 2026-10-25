#![no_main]
use layer_workspace::PackageKind;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    for kind in [PackageKind::WorkspaceBackup, PackageKind::Toolbar] {
        let _ = layer_workspace::import_package(data, kind, 0);
    }
});
