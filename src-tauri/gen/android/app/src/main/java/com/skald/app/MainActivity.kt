package com.skald.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.Settings
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

  /** Evento JS que recebe a resposta do pedido em andamento. */
  private var eventoPedido = "skald-permissao"
  /** Voltando da tela "acesso a todos os arquivos" das configurações. */
  private var aguardandoAcessoTotal = false

  private val pedido = registerForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
    avisar(eventoPedido, ok)
  }

  private fun avisar(evento: String, ok: Boolean) {
    webView?.evaluateJavascript("window.dispatchEvent(new CustomEvent('$evento', { detail: $ok }))", null)
  }

  private fun temPermissaoAudio(): Boolean =
    ContextCompat.checkSelfPermission(this, permissaoAudio) == PackageManager.PERMISSION_GRANTED

  /** Android 11+: "acesso a todos os arquivos". Antes disso, a permissão de leitura já basta
   *  (com requestLegacyExternalStorage no Android 10). */
  private fun temAcessoTotal(): Boolean =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) Environment.isExternalStorageManager()
    else ContextCompat.checkSelfPermission(this, Manifest.permission.READ_EXTERNAL_STORAGE) ==
      PackageManager.PERMISSION_GRANTED

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // O GStreamer precisa do Context (plugins estáticos, certificados, fontes) antes
    // de o Rust criar o player; depois disso o gst::init() do Rust não faz nada.
    System.loadLibrary("gstreamer_android")
    GStreamer.init(this)
    super.onCreate(savedInstanceState)
  }

  override fun onResume() {
    super.onResume()
    if (aguardandoAcessoTotal) {
      aguardandoAcessoTotal = false
      avisar("skald-acesso-total", temAcessoTotal())
    }
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    webView.addJavascriptInterface(Ponte(), "SkaldAndroid")
  }

  /** Ponte para a interface (window.SkaldAndroid). Os métodos rodam fora da thread da UI. */
  inner class Ponte {
    @JavascriptInterface
    fun temPermissao(): Boolean = temPermissaoAudio()

    /** Pedido do sistema para ler músicas; resposta no evento `skald-permissao`. */
    @JavascriptInterface
    fun pedirPermissao() {
      runOnUiThread {
        eventoPedido = "skald-permissao"
        pedido.launch(permissaoAudio)
      }
    }

    @JavascriptInterface
    fun temAcessoTotal(): Boolean = this@MainActivity.temAcessoTotal()

    /** Acesso a todos os arquivos (para ler .lrc); resposta no evento `skald-acesso-total`. */
    @JavascriptInterface
    fun pedirAcessoTotal() {
      runOnUiThread {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
          aguardandoAcessoTotal = true
          try {
            startActivity(
              Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName"))
            )
          } catch (e: Exception) {
            startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
          }
        } else {
          eventoPedido = "skald-acesso-total"
          pedido.launch(Manifest.permission.READ_EXTERNAL_STORAGE)
        }
      }
    }

    /** Pasta Music do armazenamento compartilhado (ex.: /storage/emulated/0/Music). */
    @JavascriptInterface
    fun pastaMusica(): String =
      @Suppress("DEPRECATION")
      Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MUSIC).absolutePath
  }
}
