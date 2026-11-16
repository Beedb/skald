package com.skald.app

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.core.content.ContextCompat

/** Ponte nativa da reprodução: o Rust (midia_android.rs) chama `atualizar` e recebe os
 *  botões por `comando`. O serviço de mídia mostra a notificação e mantém o app vivo. */
object Midia {
  /** Ações: alternar, tocar, pausar, proxima, anterior, buscar (valor = posição em ms). */
  @JvmStatic external fun comando(acao: String, valor: Long)

  data class Estado(
    val titulo: String,
    val artista: String,
    val album: String,
    val capa: String?,
    val duracaoMs: Long,
    val posicaoMs: Long,
    /** 0 = parado, 1 = tocando, 2 = pausado. */
    val estado: Int,
  )

  @Volatile var atual: Estado? = null
    private set
  @Volatile var contexto: Context? = null

  private val principal = Handler(Looper.getMainLooper())

  @JvmStatic
  fun atualizar(titulo: String, artista: String, album: String, capa: String?, duracaoMs: Long, posicaoMs: Long, estado: Int) {
    val e = Estado(titulo, artista, album, capa, duracaoMs, posicaoMs, estado)
    atual = e
    principal.post {
      Foco.aoMudar(contexto, e.estado)
      val servico = MidiaService.instancia
      when {
        e.estado == 0 -> servico?.encerrar()
        servico != null -> servico.aplicar(e)
        // Só começa ao tocar (com o app aberto); depois segue em segundo plano.
        e.estado == 1 -> contexto?.let {
          ContextCompat.startForegroundService(it, Intent(it, MidiaService::class.java))
        }
      }
    }
  }
}
