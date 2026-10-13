//! Native output discovery only. Shared Rust owns display conversion and tone mapping.
use layer_render_wgpu::SdrSurfaceColor;
use serde::Serialize;
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{
            Dxgi::{
                Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, CreateDXGIFactory1,
                IDXGIFactory1, IDXGIOutput6,
            },
            Gdi::{HMONITOR, MONITOR_DEFAULTTONEAREST, MonitorFromWindow},
        },
    },
    core::Interface,
};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Display {
    pub hdr: bool,
    pub peak_nits: f32,
    pub headroom: f32,
    pub name: String,
    pub error: Option<String>,
}
impl Default for Display {
    fn default() -> Self {
        Self {
            hdr: false,
            peak_nits: 0.,
            headroom: 1.,
            name: String::new(),
            error: None,
        }
    }
}
impl Display {
    pub(super) fn reported(hdr: bool, peak_nits: f32, name: String) -> Self {
        let valid_peak = peak_nits.is_finite() && peak_nits > 0.;
        Self {
            hdr: hdr && valid_peak,
            peak_nits: if valid_peak { peak_nits } else { 0. },
            headroom: if hdr && valid_peak {
                (peak_nits / layer_core::color::hdr::REFERENCE_WHITE_NITS).clamp(1., 100.)
            } else {
                1.
            },
            name,
            error: None,
        }
    }
    pub fn encoding(&self, format: wgpu::TextureFormat) -> SdrSurfaceColor {
        if format == wgpu::TextureFormat::Rgba16Float {
            if self.hdr {
                SdrSurfaceColor::WindowsScrgb
            } else {
                SdrSurfaceColor::ExtendedLinearSrgb
            }
        } else {
            SdrSurfaceColor::Srgb
        }
    }
    pub fn available_headroom(&self, format: wgpu::TextureFormat) -> f32 {
        if format == wgpu::TextureFormat::Rgba16Float {
            self.headroom
        } else {
            1.
        }
    }
}
/// A SwapChainPanel composition chain has no HWND association. Enumerate DXGI
/// outputs and match the native window's monitor, including another adapter.
/// Outputs are enumerated again only when the window changes monitor or the
/// factory stops being current, as it does for HDR toggles, docking and display
/// topology changes.
#[derive(Default)]
pub(crate) struct DisplayProbe {
    factory: Option<IDXGIFactory1>,
    monitor: Option<HMONITOR>,
    display: Display,
}
impl DisplayProbe {
    pub(crate) fn poll(&mut self, window: usize) -> &Display {
        self.poll_monitor((window != 0).then(|| unsafe {
            MonitorFromWindow(HWND(window as *mut _), MONITOR_DEFAULTTONEAREST)
        }))
    }
    fn poll_monitor(&mut self, monitor: Option<HMONITOR>) -> &Display {
        let current = self
            .factory
            .as_ref()
            .is_some_and(|factory| unsafe { factory.IsCurrent() }.as_bool());
        if current && monitor.is_some() && monitor == self.monitor && self.display.error.is_none() {
            return &self.display;
        }
        if !current {
            self.factory = None;
        }
        self.monitor = monitor;
        self.display = self.read().unwrap_or_else(|error| Display {
            error: Some(error),
            ..Default::default()
        });
        &self.display
    }
    fn read(&mut self) -> Result<Display, String> {
        let monitor = self.monitor.ok_or("Display window is unavailable")?;
        let factory = match self.factory.clone() {
            Some(factory) => factory,
            None => self
                .factory
                .insert(unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }.map_err(|e| e.to_string())?)
                .clone(),
        };
        let mut a = 0;
        while let Ok(adapter) = unsafe { factory.EnumAdapters1(a) } {
            a += 1;
            let mut o = 0;
            while let Ok(output) = unsafe { adapter.EnumOutputs(o) } {
                o += 1;
                let desc = unsafe { output.GetDesc() }.map_err(|e| e.to_string())?;
                if desc.Monitor != monitor {
                    continue;
                }
                let output: IDXGIOutput6 = output.cast().map_err(|e| e.to_string())?;
                let desc = unsafe { output.GetDesc1() }.map_err(|e| e.to_string())?;
                let end = desc
                    .DeviceName
                    .iter()
                    .position(|&v| v == 0)
                    .unwrap_or(desc.DeviceName.len());
                return Ok(Display::reported(
                    desc.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
                    desc.MaxLuminance,
                    String::from_utf16_lossy(&desc.DeviceName[..end]),
                ));
            }
        }
        Err("No current DXGI output; showing the saved SDR appearance".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{Foundation::POINT, Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint}};
    #[test]
    fn live_output_changes_and_invalid_reports_fall_back_without_changing_artwork() {
        for (hdr, peak, expected) in [
            (false, 1000., 1.),
            (true, 1015., 5.),
            (true, 0., 1.),
            (true, f32::NAN, 1.),
            (true, f32::INFINITY, 1.),
        ] {
            let display = Display::reported(hdr, peak, String::new());
            assert_eq!(
                display.available_headroom(wgpu::TextureFormat::Rgba16Float),
                expected
            );
            assert_eq!(
                display.available_headroom(wgpu::TextureFormat::Bgra8Unorm),
                1.
            );
            assert_eq!(
                display.encoding(wgpu::TextureFormat::Bgra8Unorm),
                SdrSurfaceColor::Srgb
            );
        }
        assert_eq!(
            Display::reported(true, 1015., String::new())
                .encoding(wgpu::TextureFormat::Rgba16Float),
            SdrSurfaceColor::WindowsScrgb
        );
        assert_eq!(
            Display::default().encoding(wgpu::TextureFormat::Rgba16Float),
            SdrSurfaceColor::ExtendedLinearSrgb
        );
        assert!(DisplayProbe::default().poll(0).error.is_some());
    }
    #[test]
    fn unchanged_outputs_reuse_the_factory_and_display() {
        let monitor = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
        let mut probe = DisplayProbe::default();
        let first = probe.poll_monitor(Some(monitor)).clone();
        let factory = probe.factory.as_ref().map(Interface::as_raw);
        assert!(factory.is_some());
        for _ in 0..3 {
            assert_eq!(probe.poll_monitor(Some(monitor)), &first);
            assert_eq!(probe.factory.as_ref().map(Interface::as_raw), factory, "a current factory is reused");
        }
        assert!(probe.poll_monitor(None).error.is_some());
    }
}
