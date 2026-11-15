#!/usr/bin/env bash
# Ambiente para o Skáld no Android. Uso: scripts/android.sh build --apk --target aarch64
#                                         scripts/android.sh dev
# Caminhos padrão podem ser trocados pelas variáveis abaixo.
set -euo pipefail
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/27.3.13750724}"
export JAVA_HOME="${JAVA_HOME:-$HOME/Android/jdk-21}"
export GSTREAMER_ROOT_ANDROID="${GSTREAMER_ROOT_ANDROID:-$HOME/Android/gstreamer/1.28.9}"
export PATH="$HOME/.cargo/bin:$PATH"
for d in "$ANDROID_HOME" "$NDK_HOME" "$JAVA_HOME" "$GSTREAMER_ROOT_ANDROID"; do
  [ -d "$d" ] || { echo "Pasta não encontrada: $d (veja docs/android.md)" >&2; exit 1; }
done
cd "$(dirname "$0")/.."
if [ -s "$HOME/.nvm/nvm.sh" ]; then . "$HOME/.nvm/nvm.sh" >/dev/null; nvm use >/dev/null; fi
# Sem pkg-config no Android: os crates *-sys ligam a libgstreamer_android.so (ver build.rs).
for dep in GSTREAMER_1_0 GLIB_2_0 GOBJECT_2_0 GIO_2_0 GMODULE_2_0; do
  export "SYSTEM_DEPS_${dep}_NO_PKG_CONFIG=1" "SYSTEM_DEPS_${dep}_LIB=gstreamer_android"
done
# O tauri chama o cargo antes do Gradle; a .so do GStreamer precisa existir antes.
if [ "${1:-}" != "init" ]; then
  # Mesmos argumentos da tarefa gstreamerNdkBuild de app/build.gradle.kts.
  app="$PWD/src-tauri/gen/android/app"
  # O gstreamer-1.0.mk grava gst-android-build/ na pasta atual: roda de dentro de build/.
  mkdir -p "$app/build/gstreamer"
  (cd "$app/build/gstreamer" && "$NDK_HOME/ndk-build" -s "NDK_PROJECT_PATH=$app" "APP_BUILD_SCRIPT=$app/jni/Android.mk" \
    "NDK_APPLICATION_MK=$app/jni/Application.mk" "NDK_OUT=$app/build/gstreamer/obj" \
    "NDK_LIBS_OUT=$app/build/gstreamer/libs" "GSTREAMER_ROOT_ANDROID=$GSTREAMER_ROOT_ANDROID" -j"$(nproc)")
fi
exec npx tauri android "$@"
