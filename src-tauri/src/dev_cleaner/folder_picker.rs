use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

/// Opens the native Windows file picker (`IFileOpenDialog`) on a dedicated COM STA thread.
pub fn pick_file_native() -> Result<Option<String>, String> {
    std::thread::spawn(|| {
        let init_hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let com_initialized = init_hr.is_ok();

        let result = (|| -> Result<Option<String>, String> {
            let dialog: IFileOpenDialog =
                unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL) }
                    .map_err(|e| format!("No se pudo inicializar el selector de archivos: {e}"))?;

            unsafe {
                let options = dialog.GetOptions().unwrap_or_default();
                dialog
                    .SetOptions(options | FOS_FORCEFILESYSTEM)
                    .map_err(|e| format!("No se pudieron configurar opciones del diálogo: {e}"))?;

                match dialog.Show(None) {
                    Ok(()) => {}
                    Err(_) => return Ok(None),
                }

                let item = dialog
                    .GetResult()
                    .map_err(|e| format!("No se pudo leer el archivo seleccionado: {e}"))?;
                let pwstr = item
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .map_err(|e| format!("No se pudo obtener la ruta: {e}"))?;
                let path = pwstr.to_string().ok();
                CoTaskMemFree(Some(pwstr.as_ptr().cast()));
                Ok(path)
            }
        })();

        if com_initialized {
            unsafe {
                CoUninitialize();
            }
        }
        result
    })
    .join()
    .map_err(|_| "El hilo del selector de archivos falló inesperadamente.".to_string())?
}

/// Opens the native Windows folder picker (`IFileOpenDialog` with `FOS_PICKFOLDERS`)
/// on a dedicated COM STA thread so it never conflicts with Tauri/Tokio worker threads.
pub fn pick_folder_native() -> Result<Option<String>, String> {
    std::thread::spawn(|| {
        let init_hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let com_initialized = init_hr.is_ok();

        let result = (|| -> Result<Option<String>, String> {
            let dialog: IFileOpenDialog =
                unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL) }
                    .map_err(|e| format!("No se pudo inicializar el selector de carpetas: {e}"))?;

            unsafe {
                let options = dialog.GetOptions().unwrap_or_default();
                dialog
                    .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
                    .map_err(|e| format!("No se pudieron configurar opciones del diálogo: {e}"))?;

                match dialog.Show(None) {
                    Ok(()) => {}
                    Err(_) => return Ok(None), // User cancelled
                }

                let item = dialog
                    .GetResult()
                    .map_err(|e| format!("No se pudo leer la carpeta seleccionada: {e}"))?;
                let pwstr = item
                    .GetDisplayName(SIGDN_FILESYSPATH)
                    .map_err(|e| format!("No se pudo obtener la ruta: {e}"))?;
                let path = pwstr.to_string().ok();
                CoTaskMemFree(Some(pwstr.as_ptr().cast()));
                Ok(path)
            }
        })();

        if com_initialized {
            unsafe {
                CoUninitialize();
            }
        }
        result
    })
    .join()
    .map_err(|_| "El hilo del selector de carpetas falló inesperadamente.".to_string())?
}
