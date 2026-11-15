package com.skald.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import org.freedesktop.gstreamer.GStreamer

class MainActivity : TauriActivity() {
  private var webView: WebView? = null

  /** Leitura das músicas: READ_MEDIA_AUDIO a partir do Android 13, antes READ_EXTERNAL_STORAGE. */
  private val permissaoAudio =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) Manifest.permission.READ_MEDIA_AUDIO
    else Manifest.permission.READ_EXTERNAL_STORAGE

  private val pedido = registerForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
    webView?.evaluateJavascript(
      "window.dispatchEvent(new CustomEvent('skald-permissao', { detail: $ok }))", null
    )
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // O GStreamer precisa do Context (plugins estáticos, certificados, fontes) antes
    // de o Rust criar o player; depois disso o gst::init() do Rust não faz nada.
    System.loadLibrary("gstreamer_android")
    GStreamer.init(this)
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    webView.addJavascriptInterface(Ponte(), "SkaldAndroid")
  }

  /** Ponte para a interface (window.SkaldAndroid). Os métodos rodam fora da thread da UI. */
  inner class Ponte {
    @JavascriptInterface
    fun temPermissao(): Boolean =
      ContextCompat.checkSelfPermission(this@MainActivity, permissaoAudio) == PackageManager.PERMISSION_GRANTED

    /** Mostra o pedido do sistema; a resposta chega no evento `skald-permissao` (detail = concedida). */
    @JavascriptInterface
    fun pedirPermissao() {
      runOnUiThread { pedido.launch(permissaoAudio) }
    }

    /** Pasta Music do armazenamento compartilhado (ex.: /storage/emulated/0/Music). */
    @JavascriptInterface
    fun pastaMusica(): String =
      @Suppress("DEPRECATION")
      Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MUSIC).absolutePath
  }
}
