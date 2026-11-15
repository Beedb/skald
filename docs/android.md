# Skáld no Android (preparação)

O projeto Android fica em `src-tauri/gen/android` (gerado por `tauri android init` e
ajustado à mão). O áudio continua no GStreamer: o `ndk-build` junta o GStreamer e os
plugins numa biblioteca só, `libgstreamer_android.so`, e o Rust se liga a ela.

## Ferramentas

| Item | Onde (padrão de `scripts/android.sh`) |
|------|------|
| Android SDK (platform 36/37, build-tools) | `~/Android/Sdk` |
| NDK 27.3.13750724 | `~/Android/Sdk/ndk/27.3.13750724` |
| JDK 21 | `~/Android/jdk-21` |
| GStreamer 1.28.9 para Android (universal) | `~/Android/gstreamer/1.28.9` |
| Targets Rust | `rustup target add aarch64-linux-android x86_64-linux-android` |

O SDK do GStreamer vem de
`https://gstreamer.freedesktop.org/data/pkg/android/1.28.9/gstreamer-1.0-android-universal-1.28.9.tar.xz`
(cerca de 1 GB; conferir o `.sha256sum`). Os caminhos podem ser trocados pelas variáveis
`ANDROID_HOME`, `NDK_HOME`, `JAVA_HOME` e `GSTREAMER_ROOT_ANDROID`; o Gradle também aceita
`gstreamer.dir=...` em `src-tauri/gen/android/local.properties`.

## Comandos

```sh
scripts/android.sh build --apk --debug --target aarch64   # APK de teste para celular
scripts/android.sh build --apk --target aarch64           # release (precisa de assinatura)
scripts/android.sh dev                                    # celular via adb ou emulador
```

## Como as peças se ligam

1. `app/jni/Android.mk` lista os plugins (playbin, decodificadores de MP3/FLAC/OGG/Opus/
   AAC/WAV, equalizador, ReplayGain, saída OpenSL ES). Plugin novo usado no Rust precisa
   entrar aqui.
2. A tarefa Gradle `gstreamerNdkBuild` (em `app/build.gradle.kts`) roda antes do Rust e
   grava em `app/build/gstreamer/`: `libs/<abi>/` (.so), `java/` (`GStreamer.java`) e
   `assets/` (certificados e fontes).
3. `buildSrc/.../BuildTask.kt` desliga o pkg-config dos crates `*-sys` (variáveis
   `SYSTEM_DEPS_*`) e `src-tauri/build.rs` acrescenta a pasta da `.so` ao linker.
4. `MainActivity.kt` chama `GStreamer.init(this)` antes do `super.onCreate`, assim o
   `gst::init()` do Rust já encontra tudo pronto.

## Permissão e pasta Music

`MainActivity.kt` expõe `window.SkaldAndroid` (`temPermissao`, `pedirPermissao`, cuja
resposta chega no evento `skald-permissao`, e `pastaMusica`); o lado da interface fica em
`src/android.ts`. No Android o botão "Importar pasta" pede `READ_MEDIA_AUDIO` (ou
`READ_EXTERNAL_STORAGE` até o Android 12) e chama `import_folder` com a pasta Music, que
passa a ser monitorada como no desktop. "Arquivos" some, porque o seletor do Android
devolve `content://`, que o `lofty` não lê. Com essa permissão o sistema mostra só os
arquivos de mídia. Por isso o Skáld pede antes "acesso a todos os arquivos"
(`MANAGE_EXTERNAL_STORAGE`, tela das configurações do Android; resposta no evento
`skald-acesso-total`), que deixa ler os `.lrc` ao lado das músicas. Recusado, cai no
`READ_MEDIA_AUDIO` e avisa que os `.lrc` ficam de fora. Até o Android 10 a leitura comum
basta (`requestLegacyExternalStorage`).

## APK de release para teste

`scripts/android.sh build --apk --target aarch64` gera
`app/build/outputs/apk/universal/release/app-universal-release.apk` (~26 MB), assinado
com a chave do Skáld: `~/Android/chaves/skald-release.jks` (senha em
`~/Android/chaves/keystore.properties`, copiado para `gen/android/keystore.properties`,
ignorado pelo git). Sem esse arquivo o release sai sem assinatura. **Guardar cópia da
chave**: sem ela não dá para publicar atualizações que instalem por cima.

Ícones: `npx tauri icon docs/icone.png` gera os do Android (fundo adaptável preto em
`res/values/ic_launcher_background.xml`); desfazer as mudanças em `icons/icon.icns` e
`icons/ios/`.

## Segundo plano e notificação de mídia

`src-tauri/src/midia_android.rs` faz o papel do `mpris.rs` no Android (mesma API, entra
como `mod mpris`): um worker manda faixa, capa (cache `midia/`), duração, posição e estado
para `Midia.atualizar` (Kotlin) por JNI, pegando a classe pelo class loader da atividade.
`Midia.kt` liga o `MidiaService` (serviço em primeiro plano `mediaPlayback`, com
`MediaSession` e notificação `MediaStyle`) ao começar a tocar e o desliga sem faixa. Os
botões da notificação, da tela de bloqueio e dos fones chamam `Midia.comando` →
`Java_com_skald_app_Midia_comando` (Rust) → `avancar`/`voltar`/`alternar_pausa`/`buscar`.
`proguard-rules.pro` mantém `Midia` (chamada só pelo JNI).

## O que ficou de fora no Android (`#[cfg(desktop)]`)

Bandeja, iniciar com o sistema, notificação de troca de música (`notify-rust`) e MPRIS
(já era só Linux). Os comandos continuam existindo e respondem "indisponível".

## Próximas etapas (Fase Android)

- Interface para tela estreita (lateral vira gaveta, player em tela cheia, toque).
- Monitoramento de pastas: testar o `notify` (inotify) nas pastas compartilhadas.
- Ícones e assinatura do APK de release.
