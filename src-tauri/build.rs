fn main() {
    // Android: o GStreamer vem numa biblioteca só (libgstreamer_android.so), gerada pelo
    // ndk-build em gen/android/app/build/gstreamer/libs/<abi> (tarefa gstreamerNdkBuild).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        let abi = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default().as_str() {
            "aarch64" => "arm64-v8a",
            "arm" => "armeabi-v7a",
            "x86" => "x86",
            _ => "x86_64",
        };
        let libs = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("gen/android/app/build/gstreamer/libs")
            .join(abi);
        println!("cargo:rustc-link-search=native={}", libs.display());
        println!("cargo:rustc-link-lib=dylib=gstreamer_android");
        println!("cargo:rerun-if-changed={}", libs.join("libgstreamer_android.so").display());
    }
    tauri_build::build()
}
