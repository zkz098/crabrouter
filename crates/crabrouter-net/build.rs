use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "cargo:rerun-if-changed=ebpf/crabrouter_tcx.bpf.c");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_else(|_| "target".to_string()));
    let out_obj = out_dir.join("crabrouter_tcx.bpf.o");
    let src = PathBuf::from("ebpf/crabrouter_tcx.bpf.c");

    let arch_inc = format!("-I/usr/include/{}-linux-gnu", env::consts::ARCH);
    let status = Command::new("clang")
        .args(["-O2", "-g", "-target", "bpf", &arch_inc, "-c"])
        .arg(&src)
        .arg("-o")
        .arg(&out_obj)
        .status();

    match status {
        Ok(s) if s.success() => {}
        _ => {
            // Fallback empty placeholder when clang is unavailable (e.g. non-Linux or minimal build).
            let _ = fs::write(&out_obj, []);
        }
    }
}
