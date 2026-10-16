use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory6, DXGI_ADAPTER_FLAG_SOFTWARE,
    DXGI_GPU_PREFERENCE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
};

/// The two GPU roles Windows' per-app graphics preference understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    PowerSaving,
    HighPerformance,
}

impl Role {
    pub const ALL: [Role; 2] = [Role::PowerSaving, Role::HighPerformance];

    pub fn arg(self) -> &'static str {
        match self {
            Role::PowerSaving => "power-saving",
            Role::HighPerformance => "high-performance",
        }
    }

    pub fn from_arg(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.arg().eq_ignore_ascii_case(s))
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::PowerSaving => "Power saving",
            Role::HighPerformance => "High performance",
        }
    }

    /// Value of `GpuPreference=` in `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`.
    pub fn pref_value(self) -> u32 {
        match self {
            Role::PowerSaving => 1,
            Role::HighPerformance => 2,
        }
    }

    fn dxgi(self) -> DXGI_GPU_PREFERENCE {
        match self {
            Role::PowerSaving => DXGI_GPU_PREFERENCE_MINIMUM_POWER,
            Role::HighPerformance => DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        }
    }
}

pub struct Gpu {
    pub role: Role,
    pub name: Option<String>,
}

impl Gpu {
    pub fn menu_label(&self) -> String {
        match &self.name {
            Some(name) => format!("{} ({})", self.role.label(), name),
            None => self.role.label().to_string(),
        }
    }
}

/// Resolves which physical GPU Windows uses for each role. If both roles map to the
/// same adapter (single-GPU machine) only one entry is returned. If DXGI fails,
/// both roles are returned without model names.
pub fn detect() -> Vec<Gpu> {
    let fallback = || Role::ALL.into_iter().map(|role| Gpu { role, name: None }).collect();

    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory6>() }) else {
        return fallback();
    };

    let mut gpus = Vec::new();
    let mut seen_luids = Vec::new();
    for role in Role::ALL {
        let Ok(adapter) = (unsafe { factory.EnumAdapterByGpuPreference::<IDXGIAdapter1>(0, role.dxgi()) })
        else {
            continue;
        };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        // Skip the Microsoft Basic Render Driver / WARP.
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 || desc.VendorId == 0x1414 {
            continue;
        }
        let luid = (desc.AdapterLuid.LowPart, desc.AdapterLuid.HighPart);
        if seen_luids.contains(&luid) {
            continue;
        }
        seen_luids.push(luid);

        let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..len]).trim().to_string();
        gpus.push(Gpu { role, name: (!name.is_empty()).then_some(name) });
    }

    if gpus.is_empty() {
        fallback()
    } else {
        gpus
    }
}
