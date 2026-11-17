package com.skald.app

import android.content.Context
import org.freedesktop.gstreamer.GStreamer

/** Prepara o motor do Skáld (GStreamer, biblioteca do Rust, pastas) uma vez por processo,
 *  antes do Tauri (MainActivity.onCreate). Subir o Tauri sem tela não funciona: os plugins
 *  dele esperam uma atividade ao iniciar (ver CLAUDE.md, etapa 62). */
object Motor {
  @Volatile private var preparado = false

  /** GStreamer, biblioteca do Rust e pastas do app. Idempotente; na thread principal. */
  @Synchronized
  fun preparar(ctx: Context) {
    if (preparado) return
    val app = ctx.applicationContext
    // O GStreamer precisa do Context (plugins estáticos, certificados, fontes) antes
    // de o Rust criar o player; depois disso o gst::init() do Rust não faz nada.
    System.loadLibrary("gstreamer_android")
    GStreamer.init(app)
    System.loadLibrary("skald_lib")
    Midia.contexto = app
    @Suppress("DEPRECATION")
    val musica = android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_MUSIC)
    Midia.iniciarNativo(app.dataDir.absolutePath, app.cacheDir.absolutePath, musica.absolutePath)
    preparado = true
  }
}
