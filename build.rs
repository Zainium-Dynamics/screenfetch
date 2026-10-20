// Build-time branding: any distro can bake its own logo / name / share dir.
//   SCREENFETCH_DISTRO     id of the distro to brand as (default: zainium)
//   SCREENFETCH_LOGO       path to an SVG embedded as that distro's logo
//   SCREENFETCH_SHARE_DIR  base dir searched for <id>.pixmap/<id>_logo.svg
//                          (default: /overlayer/syshub/share)
use std::{env, fmt::Write, fs, path::Path};

fn main() {
    for v in ["SCREENFETCH_DISTRO", "SCREENFETCH_LOGO", "SCREENFETCH_SHARE_DIR"] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    println!("cargo:rerun-if-changed=assets/logos");

    let distro = env::var("SCREENFETCH_DISTRO").unwrap_or_else(|_| "zainium".into());
    let share = env::var("SCREENFETCH_SHARE_DIR").unwrap_or_else(|_| "/overlayer/syshub/share".into());
    println!("cargo:rustc-env=SCREENFETCH_DEFAULT_DISTRO={distro}");
    println!("cargo:rustc-env=SCREENFETCH_BUILD_SHARE_DIR={share}");

    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut table = String::from("pub static EMBEDDED: &[(&str, &[u8])] = &[\n");

    if let Ok(custom) = env::var("SCREENFETCH_LOGO") {
        println!("cargo:rerun-if-changed={custom}");
        let abs = fs::canonicalize(&custom).unwrap_or_else(|e| panic!("SCREENFETCH_LOGO {custom}: {e}"));
        writeln!(table, "    ({distro:?}, include_bytes!({:?})),", abs.display().to_string()).unwrap();
    }
    let mut files: Vec<_> = fs::read_dir(Path::new(&root).join("assets/logos")).unwrap().flatten().collect();
    files.sort_by_key(|e| e.file_name());
    for f in files {
        let name = f.file_name().to_string_lossy().into_owned();
        if let Some(id) = name.strip_suffix("_logo.svg") {
            writeln!(table, "    ({id:?}, include_bytes!({:?})),", f.path().display().to_string()).unwrap();
        }
    }
    table.push_str("];\n");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("logos.rs"), table).unwrap();
}
