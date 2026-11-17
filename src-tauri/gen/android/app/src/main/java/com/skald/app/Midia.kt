package com.skald.app

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.core.content.ContextCompat

/** Ponte nativa da reprodução: o Rust (midia_android.rs) chama `atualizar` e recebe os
 *  botões por `comando`. O serviço de mídia mostra a notificação e mantém o app vivo. */
object Midia {
  /** Ações: alternar, tocar, pausar, proxima, anterior, buscar (valor = posição em ms),
   *  playlist (valor = id da playlist). */
  @JvmStatic external fun comando(acao: String, valor: Long)

  /** Android Auto: filhos de um nó da biblioteca, uma linha por item
   *  (`id<TAB>título<TAB>subtítulo<TAB>capa<TAB>p|t`). Só com o Rust carregado. */
  @JvmStatic external fun navegar(pai: String): String

  /** O Rust (e o GStreamer) só existem depois que a atividade abriu neste processo. */
  val rustCarregado: Boolean get() = contexto != null

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

  /** Playlists recentes para o widget (do Rust): uma linha por playlist, `id<TAB>capa`. */
  @JvmStatic
  fun playlists(texto: String) {
    principal.post {
      contexto?.let { WidgetPlayer.playlists(it, texto) }
      // O Rust acabou de carregar (ou as playlists mudaram): o Android Auto relê a biblioteca.
      MidiaService.instancia?.notifyChildrenChanged(MidiaService.RAIZ)
      MidiaService.instancia?.notifyChildrenChanged("playlists")
    }
  }

  @JvmStatic
  fun atualizar(titulo: String, artista: String, album: String, capa: String?, duracaoMs: Long, posicaoMs: Long, estado: Int) {
    val e = Estado(titulo, artista, album, capa, duracaoMs, posicaoMs, estado)
    val antes = atual
    atual = e
    principal.post {
      Foco.aoMudar(contexto, e.estado)
      // O widget só muda com música, capa ou estado (não a cada salto de posição).
      if (antes == null || antes.titulo != e.titulo || antes.artista != e.artista ||
        antes.capa != e.capa || antes.estado != e.estado
      ) contexto?.let { WidgetPlayer.atualizar(it, e) }
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
