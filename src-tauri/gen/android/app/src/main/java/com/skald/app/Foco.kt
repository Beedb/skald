package com.skald.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioManager
import androidx.core.content.ContextCompat
import androidx.media.AudioAttributesCompat
import androidx.media.AudioFocusRequestCompat
import androidx.media.AudioManagerCompat

/** Foco de áudio (etapa 42). Ao tocar, o Skáld pede o foco e o Android pausa a outra mídia;
 *  quando outro app toca (ex.: um vídeo), o Skáld pausa. Perda curta (ligação) pausa e
 *  retoma sozinho na volta; "pode abaixar" (notificação, GPS) o próprio Android abaixa o
 *  volume. Tudo na thread principal (chamado de `Midia.atualizar`). */
object Foco {
  private var pedido: AudioFocusRequestCompat? = null
  private var gerente: AudioManager? = null
  /** Pausado por uma perda curta: volta a tocar quando o foco voltar. */
  private var retomarNaVolta = false
  /** Pausa pedida pelo próprio foco: o aviso "pausado" que vem dela não devolve o foco. */
  private var pausouPorFoco = false
  /** Último estado visto: os avisos de posição repetem o estado e não contam. */
  private var ultimo = 0

  private val ouvinte = AudioManager.OnAudioFocusChangeListener { mudanca ->
    when (mudanca) {
      AudioManager.AUDIOFOCUS_LOSS -> {
        // Outro app tomou o áudio de vez: pausa e solta o foco.
        retomarNaVolta = false
        pausouPorFoco = true
        Midia.comando("pausar", 0)
        abandonar()
      }
      AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
        if (Midia.atual?.estado == 1) {
          retomarNaVolta = true
          pausouPorFoco = true
          Midia.comando("pausar", 0)
        }
      }
      AudioManager.AUDIOFOCUS_GAIN -> {
        if (retomarNaVolta) {
          retomarNaVolta = false
          Midia.comando("tocar", 0)
        }
      }
    }
  }

  /** Fone com fio tirado ou fone Bluetooth desconectado: o som iria para o alto-falante.
   *  O Android avisa com `ACTION_AUDIO_BECOMING_NOISY`; o Skáld pausa (não retoma sozinho). */
  private val barulho = object : BroadcastReceiver() {
    override fun onReceive(c: Context?, i: Intent?) {
      if (i?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY && Midia.atual?.estado == 1) {
        retomarNaVolta = false
        Midia.comando("pausar", 0)
      }
    }
  }
  private var barulhoCtx: Context? = null

  private fun ouvirFone(contexto: Context?, ligar: Boolean) {
    if (ligar && barulhoCtx == null && contexto != null) {
      val app = contexto.applicationContext
      ContextCompat.registerReceiver(
        app, barulho, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), ContextCompat.RECEIVER_EXPORTED
      )
      barulhoCtx = app
    } else if (!ligar) {
      barulhoCtx?.let { runCatching { it.unregisterReceiver(barulho) } }
      barulhoCtx = null
    }
  }

  /** Cada mudança de estado do player: 0 parado, 1 tocando, 2 pausado. */
  fun aoMudar(contexto: Context?, estado: Int) {
    if (estado == ultimo) return
    ultimo = estado
    // Só escuta o fone enquanto toca.
    ouvirFone(contexto, estado == 1)
    when (estado) {
      1 -> {
        retomarNaVolta = false
        pausouPorFoco = false
        if (pedido == null && contexto != null && !pedir(contexto)) {
          // O sistema negou (ex.: numa ligação): não toca por cima.
          Midia.comando("pausar", 0)
        }
      }
      // Pausa da pessoa: mantém o foco (outro app que tocar avisa e nada muda).
      2 -> if (!pausouPorFoco) retomarNaVolta = false
      else -> {
        retomarNaVolta = false
        abandonar()
      }
    }
  }

  private fun pedir(contexto: Context): Boolean {
    val am = contexto.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val p = AudioFocusRequestCompat.Builder(AudioManagerCompat.AUDIOFOCUS_GAIN)
      .setAudioAttributes(
        AudioAttributesCompat.Builder()
          .setUsage(AudioAttributesCompat.USAGE_MEDIA)
          .setContentType(AudioAttributesCompat.CONTENT_TYPE_MUSIC)
          .build()
      )
      .setOnAudioFocusChangeListener(ouvinte)
      .build()
    val ok = AudioManagerCompat.requestAudioFocus(am, p) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
    if (ok) {
      pedido = p
      gerente = am
    }
    return ok
  }

  private fun abandonar() {
    val p = pedido ?: return
    gerente?.let { AudioManagerCompat.abandonAudioFocusRequest(it, p) }
    pedido = null
    gerente = null
  }
}
