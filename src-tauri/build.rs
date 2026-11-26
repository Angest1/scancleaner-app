use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_os == "windows" && target_env == "gnu" {
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let specs = PathBuf::from(&manifest_dir)
                .join(".cargo")
                .join("no-default-manifest.specs");
            println!("cargo:rerun-if-changed={}", specs.display());
            println!("cargo:rustc-link-arg=-specs={}", specs.display());
        }

        // Cargo tests (`--lib` / `--test`) do not receive tauri-build's `rustc-link-arg-bins`
        // resource object, yet `tao` imports `TaskDialogIndirect` from ComCtl32 v6.
        // Embed a Common-Controls v6 manifest specifically for test executables on GNU.
        if let Ok(out_dir) = std::env::var("OUT_DIR") {
            let out = PathBuf::from(out_dir);
            let rc_path = out.join("test_manifest.rc");
            let obj_path = out.join("test_manifest.o");
            let rc_content = r#"1 24
{
" <assembly xmlns=""urn:schemas-microsoft-com:asm.v1"" manifestVersion=""1.0""> "
" <dependency> "
" <dependentAssembly> "
" <assemblyIdentity type=""win32"" name=""Microsoft.Windows.Common-Controls"" version=""6.0.0.0"" processorArchitecture=""*"" publicKeyToken=""6595b64144ccf1df"" language=""*"" /> "
" </dependentAssembly> "
" </dependency> "
" </assembly> "
}
"#;
            if std::fs::write(&rc_path, rc_content).is_ok() {
                let windres_ok = Command::new("windres")
                    .args([
                        rc_path.to_string_lossy().as_ref(),
                        "-O",
                        "coff",
                        "-o",
                        obj_path.to_string_lossy().as_ref(),
                    ])
                    .status()
                    .is_ok_and(|s| s.success());
                if windres_ok {
                    let lib_path = out.join("libtestmanifest.a");
                    let ar_ok = Command::new("ar")
                        .args([
                            "rcs",
                            lib_path.to_string_lossy().as_ref(),
                            obj_path.to_string_lossy().as_ref(),
                        ])
                        .status()
                        .is_ok_and(|s| s.success());
                    if ar_ok {
                        println!("cargo:rustc-link-search=native={}", out.display());
                    }
                    println!("cargo:rustc-link-arg-tests={}", obj_path.display());
                }
            }
        }
    }

    tauri_build::build()
}
