use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::app_data_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Omitir la secuencia animada de la pantalla de carga al iniciar.
    #[serde(default)]
    pub skip_startup_animation: bool,
    /// Incluir Alternate Data Streams (ADS) en el cálculo de espacio NTFS.
    #[serde(default)]
    pub include_ads: bool,
    /// Iniciar con Windows minimizado en la bandeja del sistema (off por defecto).
    #[serde(default)]
    pub start_with_windows_tray: bool,
    /// Timeout en minutos para vigilar desinstaladores nativos de Windows (default 10).
    #[serde(default = "default_uninstall_timeout_mins")]
    pub uninstall_watch_timeout_mins: u64,
    /// Porcentaje de deriva de espacio libre que dispara reconciliación (default 1.0%).
    #[serde(default = "default_drift_percent")]
    pub drift_threshold_percent: f64,
    /// Umbral mínimo absoluto en GB para deriva de espacio libre (default 1 GB).
    #[serde(default = "default_drift_gb")]
    pub drift_threshold_gb: u64,
}

fn default_uninstall_timeout_mins() -> u64 {
    10
}
fn default_drift_percent() -> f64 {
    1.0
}
fn default_drift_gb() -> u64 {
    1
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            skip_startup_animation: false,
            include_ads: false,
            start_with_windows_tray: false,
            uninstall_watch_timeout_mins: default_uninstall_timeout_mins(),
            drift_threshold_percent: default_drift_percent(),
            drift_threshold_gb: default_drift_gb(),
        }
    }
}

pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

impl AppSettings {
    pub fn load_from(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Self>(&s).ok())
            .unwrap_or_default()
    }

    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&settings_path())
    }
}
