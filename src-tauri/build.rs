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
    credenciais_google();
    tauri_build::build()
}

/// ID e segredo do cliente OAuth do Google (sincronização pelo Drive). Nunca ficam no
/// repositório: vêm de `SKALD_GOOGLE_CLIENT_ID`/`SKALD_GOOGLE_CLIENT_SECRET` (CI) ou do JSON
/// baixado do Google Cloud (`SKALD_GOOGLE_OAUTH_JSON`, padrão
/// `~/Android/chaves/google-oauth-desktop.json`). Sem eles, a sincronização fica indisponível.
fn credenciais_google() {
    for v in ["SKALD_GOOGLE_CLIENT_ID", "SKALD_GOOGLE_CLIENT_SECRET", "SKALD_GOOGLE_OAUTH_JSON"] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    let env = |v: &str| std::env::var(v).ok().filter(|s| !s.is_empty());
    let (id, segredo) = match (env("SKALD_GOOGLE_CLIENT_ID"), env("SKALD_GOOGLE_CLIENT_SECRET")) {
        (Some(i), Some(s)) => (i, s),
        _ => {
            let caminho = env("SKALD_GOOGLE_OAUTH_JSON").map(std::path::PathBuf::from).or_else(|| {
                env("HOME").or_else(|| env("USERPROFILE")).map(|h| {
                    std::path::Path::new(&h).join("Android/chaves/google-oauth-desktop.json")
                })
            });
            let Some(caminho) = caminho.filter(|c| c.exists()) else { return };
            println!("cargo:rerun-if-changed={}", caminho.display());
            let Some(json) = std::fs::read_to_string(&caminho)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            else {
                println!("cargo:warning=JSON do Google ilegível: {}", caminho.display());
                return;
            };
            let campo = |c: &str| json["installed"][c].as_str().map(str::to_string);
            match (campo("client_id"), campo("client_secret")) {
                (Some(i), Some(s)) => (i, s),
                _ => return,
            }
        }
    };
    println!("cargo:rustc-env=SKALD_GOOGLE_CLIENT_ID={id}");
    println!("cargo:rustc-env=SKALD_GOOGLE_CLIENT_SECRET={segredo}");
}
