package com.skald.app

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import android.os.Bundle
import android.util.SizeF
import android.widget.RemoteViews

/** Widget do player (2×1 a 5×2). Uma linha: a música atual com anterior, tocar/pausar e
 *  próxima. Duas linhas: o player em cima e as capas das playlists recentes embaixo.
 *  O estado é guardado em SharedPreferences para o widget seguir certo com o app fechado. */
class WidgetPlayer : AppWidgetProvider() {
  companion object {
    private const val PREFS = "widget_player"
    private const val ACAO = "com.skald.app.WIDGET_ACAO"
    private const val EXTRA = "acao"
    private const val EXTRA_VALOR = "valor"
    /** A partir desta largura (≈ 3 células) entram capa e título. */
    private const val LARGO_DP = 180f
    /** A partir desta altura (≈ 2 células) entram as playlists. */
    private const val ALTO_DP = 100f
    private const val CAPA_PX = 192
    private val CAPAS = intArrayOf(R.id.w_pl1, R.id.w_pl2, R.id.w_pl3, R.id.w_pl4, R.id.w_pl5)

    private var capaCarregada: String? = null
    private var capa: Bitmap? = null
    /** Miniaturas das playlists já lidas, por caminho. */
    private val capasPlaylists = HashMap<String, Bitmap?>()

    /** Chamado por `Midia.playlists`: uma linha por playlist, `id<TAB>caminho da capa`. */
    fun playlists(ctx: Context, texto: String) {
      ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString("playlists", texto).apply()
      // Só ficam no cache as capas da lista atual.
      val atuais = texto.lineSequence().map { it.substringAfter('\t', "") }.toSet()
      capasPlaylists.keys.retainAll(atuais)
      redesenhar(ctx)
    }

    /** Chamado por `Midia.atualizar` (thread principal) quando a música ou o estado mudam. */
    fun atualizar(ctx: Context, e: Midia.Estado) {
      ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
        .putString("titulo", e.titulo)
        .putString("artista", e.artista)
        .putString("capa", e.capa)
        .putInt("estado", e.estado)
        .apply()
      redesenhar(ctx)
    }

    fun redesenhar(ctx: Context) {
      val gerente = AppWidgetManager.getInstance(ctx)
      val ids = gerente.getAppWidgetIds(ComponentName(ctx, WidgetPlayer::class.java))
      for (id in ids) gerente.updateAppWidget(id, vistas(ctx, gerente, id))
    }

    private fun vistas(ctx: Context, gerente: AppWidgetManager, id: Int): RemoteViews {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        // O lançador escolhe o layout pelo tamanho, sem esperar o app. Larguras de 2 a 5
        // células (≈ 110, 180, 250, 320 dp); com 2 linhas, uma capa a mais por célula.
        val mapa = LinkedHashMap<SizeF, RemoteViews>()
        for (w in floatArrayOf(100f, LARGO_DP, 250f, 320f)) {
          mapa[SizeF(w, 40f)] = paraTamanho(ctx, w, 40f, quantasCapas(w))
          mapa[SizeF(w, ALTO_DP)] = paraTamanho(ctx, w, ALTO_DP, quantasCapas(w))
        }
        return RemoteViews(mapa)
      }
      val o = gerente.getAppWidgetOptions(id)
      val largura = o.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 250).toFloat()
      val altura = o.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT, 40).toFloat()
      return paraTamanho(ctx, largura, altura, quantasCapas(largura))
    }

    /** Capas pequenas, como uma faixa: 3 em 2 células, 4 em 3, 5 a partir de 4. */
    private fun quantasCapas(largura: Float): Int = when {
      largura >= 250f -> 5
      largura >= LARGO_DP -> 4
      else -> 3
    }

    private fun paraTamanho(ctx: Context, largura: Float, altura: Float, capas: Int): RemoteViews {
      val largo = largura >= LARGO_DP
      return if (altura >= ALTO_DP) montar(ctx, if (largo) R.layout.widget_alto else R.layout.widget_alto_estreito, capas)
      else montar(ctx, if (largo) R.layout.widget_largo else R.layout.widget_estreito, 0)
    }

    private fun montar(ctx: Context, layout: Int, capas: Int): RemoteViews {
      val p = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
      // Com o processo morto o "tocando" guardado é velho: mostra pausado.
      val tocando = p.getInt("estado", 0) == 1 && Midia.atual != null
      val titulo = p.getString("titulo", null)?.takeIf { it.isNotEmpty() }
      val v = RemoteViews(ctx.packageName, layout)
      v.setOnClickPendingIntent(R.id.w_raiz, abrirApp(ctx))
      v.setOnClickPendingIntent(R.id.w_anterior, acao(ctx, "anterior", 11))
      v.setOnClickPendingIntent(R.id.w_alternar, acao(ctx, "alternar", 12))
      v.setOnClickPendingIntent(R.id.w_proxima, acao(ctx, "proxima", 13))
      v.setImageViewResource(R.id.w_alternar, if (tocando) R.drawable.widget_pausar else R.drawable.widget_tocar)
      if (capas > 0) mostrarPlaylists(ctx, v, p.getString("playlists", "") ?: "", capas)
      if (layout == R.layout.widget_largo || layout == R.layout.widget_alto) {
        v.setTextViewText(R.id.w_titulo, titulo ?: ctx.getString(R.string.widget_parado))
        v.setTextViewText(R.id.w_artista, if (titulo == null) "Skáld" else p.getString("artista", "") ?: "")
        val b = carregarCapa(p.getString("capa", null))
        if (b != null) v.setImageViewBitmap(R.id.w_capa, b) else v.setImageViewResource(R.id.w_capa, R.drawable.widget_capa_vazia)
      }
      return v
    }

    /** Capas das playlists: até `capas` vagas; as que sobram ficam invisíveis (mesmo tamanho). */
    private fun mostrarPlaylists(ctx: Context, v: RemoteViews, texto: String, capas: Int) {
      val lista = texto.lineSequence().mapNotNull { linha ->
        val partes = linha.split('\t', limit = 2)
        partes[0].toLongOrNull()?.let { it to partes.getOrNull(1).orEmpty() }
      }.toList()
      for ((i, vista) in CAPAS.withIndex()) {
        val item = lista.getOrNull(i)
        when {
          i >= capas -> v.setViewVisibility(vista, android.view.View.GONE)
          item == null -> v.setViewVisibility(vista, android.view.View.INVISIBLE)
          else -> {
            v.setViewVisibility(vista, android.view.View.VISIBLE)
            val b = item.second.takeIf { it.isNotEmpty() }?.let { c -> capasPlaylists.getOrPut(c) { reduzir(c)?.let(::quadrada) } }
            if (b != null) v.setImageViewBitmap(vista, b) else v.setImageViewResource(vista, R.drawable.widget_capa_vazia)
            v.setOnClickPendingIntent(vista, acao(ctx, "playlist", 20 + i, item.first))
          }
        }
      }
    }

    /** Capa da música atual, reduzida e guardada enquanto não muda. */
    private fun carregarCapa(caminho: String?): Bitmap? {
      if (caminho != capaCarregada) {
        capaCarregada = caminho
        capa = caminho?.let { reduzir(it) }
      }
      return capa
    }

    /** Recorte quadrado do centro (a altura da capa segue a largura no widget). */
    private fun quadrada(b: Bitmap): Bitmap {
      val lado = minOf(b.width, b.height)
      if (b.width == b.height) return b
      return Bitmap.createBitmap(b, (b.width - lado) / 2, (b.height - lado) / 2, lado, lado)
    }

    /** Imagem reduzida (o RemoteViews tem limite de memória para bitmaps). */
    private fun reduzir(caminho: String): Bitmap? = runCatching {
      val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
      BitmapFactory.decodeFile(caminho, o)
      var amostra = 1
      while (o.outWidth / (amostra * 2) >= CAPA_PX && o.outHeight / (amostra * 2) >= CAPA_PX) amostra *= 2
      BitmapFactory.decodeFile(caminho, BitmapFactory.Options().apply { inSampleSize = amostra })
    }.getOrNull()

    private fun acao(ctx: Context, nome: String, codigo: Int, valor: Long = 0): PendingIntent =
      PendingIntent.getBroadcast(
        ctx, codigo,
        Intent(ctx, WidgetPlayer::class.java).setAction(ACAO).putExtra(EXTRA, nome).putExtra(EXTRA_VALOR, valor),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
      )

    private fun abrirApp(ctx: Context): PendingIntent =
      PendingIntent.getActivity(
        ctx, 10,
        Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
      )
  }

  override fun onUpdate(ctx: Context, gerente: AppWidgetManager, ids: IntArray) {
    for (id in ids) gerente.updateAppWidget(id, vistas(ctx, gerente, id))
  }

  override fun onAppWidgetOptionsChanged(ctx: Context, gerente: AppWidgetManager, id: Int, opcoes: Bundle) {
    gerente.updateAppWidget(id, vistas(ctx, gerente, id))
  }

  override fun onReceive(ctx: Context, intent: Intent) {
    if (intent.action != ACAO) return super.onReceive(ctx, intent)
    val nome = intent.getStringExtra(EXTRA) ?: return
    if (Midia.pronto) {
      // O Rust está carregado (app aberto ou tocando em segundo plano).
      Midia.comando(nome, intent.getLongExtra(EXTRA_VALOR, 0))
    } else {
      // Processo novo, sem o Rust: abre o app (ele volta com a última música, pausada).
      ctx.startActivity(
        Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      )
    }
  }
}
