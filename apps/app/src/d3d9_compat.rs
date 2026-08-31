use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::FreeLibrary,
        Graphics::Direct3D9::{
            D3D_SDK_VERSION, D3DADAPTER_DEFAULT,
            D3DCREATE_HARDWARE_VERTEXPROCESSING, D3DCREATE_MULTITHREADED,
            D3DDEVTYPE_HAL, D3DFMT_UNKNOWN, D3DPRESENT_INTERVAL_DEFAULT,
            D3DPRESENT_PARAMETERS, D3DSWAPEFFECT_COPY, IDirect3D9Ex,
        },
        System::LibraryLoader::{
            GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
            LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
        },
    },
    core::{HRESULT, Interface, PCSTR, PCWSTR},
};

type Direct3DCreate9ExFn =
    unsafe extern "system" fn(u32, *mut *mut c_void) -> HRESULT;

pub fn initialize(window: &tauri::Window) {
    let executable_path = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(
                "Could not locate the Axolotl executable for D3D9 \
                 compatibility: {error}"
            );
            return;
        }
    };
    let Some(executable_dir) = executable_path.parent() else {
        tracing::warn!(
            "Could not locate the Axolotl executable directory for D3D9 \
             compatibility"
        );
        return;
    };
    let dll_path = executable_dir.join("d3d9.dll");
    if !dll_path.is_file() {
        tracing::debug!(
            "No local d3d9.dll found; D3D9 compatibility is disabled"
        );
        return;
    }

    match initialize_from_path(window, &dll_path) {
        Ok(()) => tracing::info!(
            "Initialized D3D9 compatibility using {}",
            dll_path.display()
        ),
        Err(error) => tracing::warn!(
            "Failed to initialize D3D9 compatibility from {}: {error}",
            dll_path.display()
        ),
    }
}

fn initialize_from_path(
    window: &tauri::Window,
    dll_path: &Path,
) -> Result<(), String> {
    let wide_path = dll_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let module = unsafe {
        LoadLibraryExW(
            PCWSTR(wide_path.as_ptr()),
            None,
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .map_err(|error| format!("could not load the DLL: {error}"))?;

    let result = initialize_loaded_module(window, module);
    if result.is_err() {
        let _ = unsafe { FreeLibrary(module) };
    }
    result
}

fn initialize_loaded_module(
    window: &tauri::Window,
    module: windows::Win32::Foundation::HMODULE,
) -> Result<(), String> {
    let symbol = unsafe {
        GetProcAddress(module, PCSTR(b"Direct3DCreate9Ex\0".as_ptr()))
    }
    .ok_or_else(|| "the DLL does not export Direct3DCreate9Ex".to_string())?;
    let create_direct3d: Direct3DCreate9ExFn =
        unsafe { std::mem::transmute(symbol) };
    let mut direct3d_ptr = std::ptr::null_mut();
    unsafe { create_direct3d(D3D_SDK_VERSION, &mut direct3d_ptr) }
        .ok()
        .map_err(|error| format!("Direct3DCreate9Ex failed: {error}"))?;
    if direct3d_ptr.is_null() {
        return Err("Direct3DCreate9Ex returned a null interface".to_string());
    }
    let direct3d = unsafe { IDirect3D9Ex::from_raw(direct3d_ptr) };

    let hwnd = window.hwnd().map_err(|error| {
        format!("could not obtain the main window: {error}")
    })?;
    let mut parameters = D3DPRESENT_PARAMETERS {
        BackBufferWidth: 1,
        BackBufferHeight: 1,
        BackBufferFormat: D3DFMT_UNKNOWN,
        BackBufferCount: 1,
        SwapEffect: D3DSWAPEFFECT_COPY,
        hDeviceWindow: hwnd,
        Windowed: true.into(),
        PresentationInterval: D3DPRESENT_INTERVAL_DEFAULT as u32,
        ..Default::default()
    };
    let mut device = None;
    unsafe {
        direct3d.CreateDeviceEx(
            D3DADAPTER_DEFAULT,
            D3DDEVTYPE_HAL,
            hwnd,
            (D3DCREATE_HARDWARE_VERTEXPROCESSING | D3DCREATE_MULTITHREADED)
                as u32,
            &mut parameters,
            std::ptr::null_mut(),
            &mut device,
        )
    }
    .map_err(|error| format!("could not create the D3D9 device: {error}"))?;
    let device = device.ok_or_else(|| {
        "D3D9 device creation succeeded without returning a device".to_string()
    })?;

    std::mem::forget(device);
    std::mem::forget(direct3d);
    Ok(())
}
