//! Hardware acceleration (Options > General, #445): the window is drawn
//! with wgpu on the graphics card chosen there (the fastest one when none
//! is), or with the system's software adapter when it is off. A start
//! whose renderer cannot open the window starts TraceDraw again with the
//! next one, hardware, then software, then OpenGL, so a virtual machine
//! without a usable graphics card still opens (#439). See
//! docs/behavior/about-and-startup.md.

use std::sync::{Arc, OnceLock};

/// How a start draws the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// wgpu on a graphics card (a software adapter when there is none).
    Hardware,
    /// wgpu on the software adapter (WARP on Windows, a CPU Vulkan
    /// driver elsewhere).
    Software,
    /// OpenGL, the renderer of earlier versions.
    Gl,
}

/// The command-line switch that names the renderer of a start.
const ARG: &str = "--renderer=";

impl Mode {
    pub fn arg(self) -> String {
        format!("{ARG}{}", self.name())
    }

    /// The renderer `--renderer=` names among `args`.
    pub fn from_args(args: &[String]) -> Option<Mode> {
        args.iter()
            .find_map(|a| a.strip_prefix(ARG).and_then(Mode::from_name))
    }

    /// The renderer to try when this one cannot open the window.
    /// How the settings remember it.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Hardware => "hardware",
            Mode::Software => "software",
            Mode::Gl => "gl",
        }
    }

    fn from_name(name: &str) -> Option<Mode> {
        [Mode::Hardware, Mode::Software, Mode::Gl]
            .into_iter()
            .find(|m| m.name() == name)
    }

    /// The renderer a start uses: the one `--renderer=` names (a start
    /// before could not open the window), else the one that opened it
    /// last time when the first choice did not, else the graphics card or,
    /// with hardware acceleration off, the software adapter.
    pub fn for_start(args: &[String], s: &crate::settings::Settings) -> Mode {
        let first = if s.hardware_acceleration {
            Mode::Hardware
        } else {
            Mode::Software
        };
        Mode::from_args(args)
            .or_else(|| Mode::from_name(&s.working_renderer).filter(|m| *m != first))
            .unwrap_or(first)
    }

    pub fn next(self) -> Option<Mode> {
        match self {
            Mode::Hardware => Some(Mode::Software),
            Mode::Software => Some(Mode::Gl),
            Mode::Gl => None,
        }
    }
}

static IN_USE: OnceLock<String> = OnceLock::new();

/// What draws the window in this run, as Options shows it.
pub fn in_use() -> Option<&'static str> {
    IN_USE.get().map(String::as_str)
}

/// Note what the window was opened with; a renderer other than the
/// first choice is remembered, so the next start goes straight to it.
pub fn note_renderer(cc: &eframe::CreationContext<'_>, mode: Mode) {
    let mut s = crate::settings::Settings::load();
    let first = if s.hardware_acceleration {
        Mode::Hardware
    } else {
        Mode::Software
    };
    let remember = if mode == first { "" } else { mode.name() };
    if s.working_renderer != remember {
        s.working_renderer = remember.to_string();
        s.save();
    }
    let what = match cc.wgpu_render_state.as_ref() {
        Some(rs) => {
            let i = rs.adapter.get_info();
            format!("{} ({:?})", i.name, i.backend)
        }
        None => "OpenGL".to_string(),
    };
    let _ = IN_USE.set(what);
}

/// The graphics adapters of this computer by name, software ones last;
/// looked for once.
pub fn adapters() -> &'static [String] {
    static LIST: OnceLock<Vec<String>> = OnceLock::new();
    LIST.get_or_init(|| {
        if cfg!(test) {
            return Vec::new();
        }
        let instance = eframe::wgpu::Instance::new(
            eframe::wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
        );
        let found =
            pollster::block_on(instance.enumerate_adapters(eframe::wgpu::Backends::PRIMARY));
        let mut list: Vec<(u8, String)> = found
            .iter()
            .map(|a| {
                let i = a.get_info();
                (rank(i.device_type), i.name)
            })
            .collect();
        list.sort();
        let mut names: Vec<String> = Vec::new();
        for (_, n) in list {
            if !names.contains(&n) {
                names.push(n);
            }
        }
        names
    })
}

/// The order adapters are preferred in: separate cards, built-in ones,
/// virtual ones, others, then software.
fn rank(t: eframe::wgpu::DeviceType) -> u8 {
    use eframe::wgpu::DeviceType;
    match t {
        DeviceType::DiscreteGpu => 0,
        DeviceType::IntegratedGpu => 1,
        DeviceType::VirtualGpu => 2,
        DeviceType::Other => 3,
        DeviceType::Cpu => 4,
    }
}

/// The renderer and the wgpu set-up of a start in `mode`; `wanted` names
/// the graphics card chosen (empty: the fastest).
pub fn options(
    mode: Mode,
    wanted: &str,
) -> (eframe::Renderer, eframe::egui_wgpu::WgpuConfiguration) {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
    if mode == Mode::Gl {
        return (eframe::Renderer::Glow, WgpuConfiguration::default());
    }
    let wanted = wanted.to_string();
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.native_adapter_selector = Some(Arc::new(move |list, surface| {
        let usable: Vec<&eframe::wgpu::Adapter> = list
            .iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .collect();
        let pick = |want: &dyn Fn(&eframe::wgpu::AdapterInfo) -> bool| {
            usable
                .iter()
                .filter(|a| want(&a.get_info()))
                .min_by_key(|a| rank(a.get_info().device_type))
                .map(|a| (*a).clone())
        };
        let software =
            |i: &eframe::wgpu::AdapterInfo| i.device_type == eframe::wgpu::DeviceType::Cpu;
        let found = match mode {
            Mode::Software => pick(&software),
            _ => pick(&|i| !wanted.is_empty() && i.name == wanted)
                .or_else(|| pick(&|i| !software(i)))
                .or_else(|| pick(&software)),
        };
        found.ok_or_else(|| "no graphics adapter can draw the window".to_string())
    }));
    let config = WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    };
    (eframe::Renderer::Wgpu, config)
}

/// Run TraceDraw again with the renderer after `mode`, the other
/// arguments kept, until it ends; false when there is none left or it did
/// not start.
pub fn restart_with_next(mode: Mode, args: &[String]) -> bool {
    let Some(next) = mode.next() else {
        return false;
    };
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut rest: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with(ARG))
        .cloned()
        .collect();
    rest.push(next.arg());
    std::process::Command::new(exe).args(rest).status().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_renderer_that_fails_hands_over_to_the_next() {
        let args = vec!["drawing.cdr".to_string()];
        assert_eq!(Mode::from_args(&args), None);
        let mut m = Mode::Hardware;
        let mut seen = vec![m];
        while let Some(n) = m.next() {
            let mut with = args.clone();
            with.push(n.arg());
            assert_eq!(Mode::from_args(&with), Some(n));
            seen.push(n);
            m = n;
        }
        assert_eq!(seen, vec![Mode::Hardware, Mode::Software, Mode::Gl]);
        // The renderer that opened the window last time, unless it is the
        // first choice anyway; hardware acceleration off starts in software.
        let mut s = crate::settings::Settings::default();
        assert_eq!(Mode::for_start(&args, &s), Mode::Hardware);
        s.working_renderer = "gl".into();
        assert_eq!(Mode::for_start(&args, &s), Mode::Gl);
        s.working_renderer.clear();
        s.hardware_acceleration = false;
        assert_eq!(Mode::for_start(&args, &s), Mode::Software);
        assert_eq!(Mode::for_start(&[Mode::Gl.arg()], &s), Mode::Gl);
        assert_eq!(options(Mode::Gl, "").0, eframe::Renderer::Glow);
        assert_eq!(options(Mode::Hardware, "").0, eframe::Renderer::Wgpu);
    }
}
