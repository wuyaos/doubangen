use std::process::Command;

fn main() {
    // build.rs 编译为 host 二进制，cfg!(target_os) 检查的是 host 而非 target；
    // 用 cargo 注入的 CARGO_CFG_TARGET_OS 判断实际编译目标。
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let rc_path = format!("{}/icon.rc", out_dir);
    let obj_path = format!("{}/icon.o", out_dir);
    let lib_path = format!("{}/libiconres.a", out_dir);

    // .rc 里用绝对路径引用 .ico，避免 windres 工作目录问题
    let ico_abs = std::fs::canonicalize("assets/icon.ico")
        .expect("assets/icon.ico not found");
    let rc_content = format!("1 ICON \"{}\"\n", ico_abs.display());
    std::fs::write(&rc_path, rc_content).expect("write icon.rc");

    // windres: .rc → COFF .o（GNU 交叉工具链带前缀）
    let windres = std::env::var("WINDRES")
        .unwrap_or_else(|_| "x86_64-w64-mingw32-windres".to_string());
    let ok = Command::new(&windres)
        .args([&rc_path, "-O", "coff", "-o", &obj_path, "-J", "rc"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !ok {
        // 回退：无前缀 windres
        let ok2 = Command::new("windres")
            .args([&rc_path, "-O", "coff", "-o", &obj_path, "-J", "rc"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok2 {
            println!("cargo:warning=windres 不可用，.exe 图标未嵌入");
            return;
        }
    }

    // ar: .o → 静态库，whole-archive 链接进 .exe
    let ar = std::env::var("AR")
        .unwrap_or_else(|_| "x86_64-w64-mingw32-ar".to_string());
    let ar_ok = Command::new(&ar)
        .args(["rcs", &lib_path, &obj_path])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ar_ok {
        println!("cargo:rustc-link-search=native={}", out_dir);
        println!("cargo:rustc-link-lib=static:+whole-archive=iconres");
    } else {
        println!("cargo:warning=ar 不可用，.exe 图标未嵌入");
    }
}
