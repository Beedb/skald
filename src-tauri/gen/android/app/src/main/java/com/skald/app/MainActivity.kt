package com.skald.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.DocumentsContract
import org.json.JSONObject
import android.provider.Settings
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.freedesktop.gstreamer.GStreamer

class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  /** Margens seguras (barra de status, recorte da câmera, barra de navegação) em px de CSS. */
  @Volatile private var insetsJson = "{\"topo\":0,\"base\":0,\"esquerda\":0,\"direita\":0}"

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

  /** Seletor de pastas do Android; a resposta vai para o evento `skald-pasta` (caminho ou null). */
  private val seletorPasta = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
    val caminho = uri?.let { caminhoDaArvore(it) }
    val detalhe = if (caminho != null) JSONObject.quote(caminho) else "null"
    webView?.evaluateJavascript("window.dispatchEvent(new CustomEvent('skald-pasta', { detail: $detalhe }))", null)
  }

  /** content://…/tree/primary%3AMúsicas → /storage/emulated/0/Músicas (cartão SD: /storage/XXXX-XXXX/…).
   *  Com acesso aos arquivos o Rust lê o caminho direto, como no desktop. */
  private fun caminhoDaArvore(uri: Uri): String? {
    val id = runCatching { DocumentsContract.getTreeDocumentId(uri) }.getOrNull() ?: return null
    if (id.startsWith("raw:")) return id.removePrefix("raw:")
    val volume = id.substringBefore(':')
    val resto = id.substringAfter(':', "")
    @Suppress("DEPRECATION")
    val raiz = when (volume) {
      "primary" -> Environment.getExternalStorageDirectory().absolutePath
      "home" -> Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS).absolutePath
      "downloads" -> Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS).absolutePath
      else -> if (Regex("[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}").matches(volume)) "/storage/$volume" else return null
    }
    return if (resto.isEmpty()) raiz else "$raiz/$resto"
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
    // GStreamer, Rust e pastas (talvez já feito pelo Android Auto, sem tela).
    Motor.preparar(this)
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
    // O app desenha por baixo das barras do sistema (edge-to-edge, obrigatório no Android 15+);
    // a interface recebe as margens e afasta o conteúdo delas (variáveis --seguro-* no CSS).
    ViewCompat.setOnApplyWindowInsetsListener(webView) { v, insets ->
      val b = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val d = resources.displayMetrics.density
      insetsJson = "{\"topo\":${b.top / d},\"base\":${b.bottom / d},\"esquerda\":${b.left / d},\"direita\":${b.right / d}}"
      avisar("skald-insets", true)
      ViewCompat.onApplyWindowInsets(v, insets)
    }
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

    /** Margens seguras atuais (JSON com topo, base, esquerda e direita em px de CSS). */
    @JavascriptInterface
    fun insets(): String = insetsJson

    /** Abre o seletor de pastas; resposta no evento `skald-pasta`. */
    @JavascriptInterface
    fun escolherPasta() {
      runOnUiThread { seletorPasta.launch(null) }
    }

    /** Pasta Music do armazenamento compartilhado (ex.: /storage/emulated/0/Music). */
    @JavascriptInterface
    fun pastaMusica(): String =
      @Suppress("DEPRECATION")
      Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_MUSIC).absolutePath
  }
}
