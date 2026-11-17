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

/** Widget do player (2×1 a 5×1). Mostra a música atual com anterior, tocar/pausar e próxima.
 *  O estado é guardado em SharedPreferences para o widget seguir certo com o app fechado. */
class WidgetPlayer : AppWidgetProvider() {
  companion object {
    private const val PREFS = "widget_player"
    private const val ACAO = "com.skald.app.WIDGET_ACAO"
    private const val EXTRA = "acao"
    /** A partir desta largura (≈ 3 células) entram capa e título. */
    private const val LARGO_DP = 180f
    private const val CAPA_PX = 192

    private var capaCarregada: String? = null
    private var capa: Bitmap? = null

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
        // O lançador escolhe o layout pelo tamanho, sem esperar o app.
        return RemoteViews(
          mapOf(
            SizeF(100f, 40f) to montar(ctx, R.layout.widget_estreito),
            SizeF(LARGO_DP, 40f) to montar(ctx, R.layout.widget_largo),
          )
        )
      }
      val largura = gerente.getAppWidgetOptions(id).getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 250)
      return montar(ctx, if (largura >= LARGO_DP) R.layout.widget_largo else R.layout.widget_estreito)
    }

    private fun montar(ctx: Context, layout: Int): RemoteViews {
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
      if (layout == R.layout.widget_largo) {
        v.setTextViewText(R.id.w_titulo, titulo ?: ctx.getString(R.string.widget_parado))
        v.setTextViewText(R.id.w_artista, if (titulo == null) "Skáld" else p.getString("artista", "") ?: "")
        val b = carregarCapa(p.getString("capa", null))
        if (b != null) v.setImageViewBitmap(R.id.w_capa, b) else v.setImageViewResource(R.id.w_capa, R.drawable.widget_capa_vazia)
      }
      return v
    }

    /** Capa reduzida (o RemoteViews tem limite de memória para bitmaps). */
    private fun carregarCapa(caminho: String?): Bitmap? {
      if (caminho != capaCarregada) {
        capaCarregada = caminho
        capa = caminho?.let {
          runCatching {
            val o = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(it, o)
            var amostra = 1
            while (o.outWidth / (amostra * 2) >= CAPA_PX && o.outHeight / (amostra * 2) >= CAPA_PX) amostra *= 2
            BitmapFactory.decodeFile(it, BitmapFactory.Options().apply { inSampleSize = amostra })
          }.getOrNull()
        }
      }
      return capa
    }

    private fun acao(ctx: Context, nome: String, codigo: Int): PendingIntent =
      PendingIntent.getBroadcast(
        ctx, codigo,
        Intent(ctx, WidgetPlayer::class.java).setAction(ACAO).putExtra(EXTRA, nome),
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
    if (Midia.contexto != null) {
      // O Rust está carregado (app aberto ou tocando em segundo plano).
      Midia.comando(nome, 0)
    } else {
      // Processo novo, sem o Rust: abre o app (ele volta com a última música, pausada).
      ctx.startActivity(
        Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      )
    }
  }
}
