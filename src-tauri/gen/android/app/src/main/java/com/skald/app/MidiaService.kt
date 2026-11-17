package com.skald.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import android.os.SystemClock
import android.support.v4.media.MediaBrowserCompat
import android.support.v4.media.MediaDescriptionCompat
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.media.MediaBrowserServiceCompat
import androidx.media.app.NotificationCompat.MediaStyle
import java.io.File

/** Reprodução em segundo plano: serviço em primeiro plano com MediaSession (notificação,
 *  tela de bloqueio, fones Bluetooth). O áudio continua no GStreamer, do lado do Rust.
 *  Também é o MediaBrowserService do Android Auto (navegar pela biblioteca no carro). */
class MidiaService : MediaBrowserServiceCompat() {
  companion object {
    @Volatile var instancia: MidiaService? = null
      private set
    private const val CANAL = "reproducao"
    private const val ID = 1
    const val RAIZ = "raiz"
    /** Pacote do Android Auto, que precisa de permissão para ler as capas. */
    private const val ANDROID_AUTO = "com.google.android.projection.gearhead"
    // Dicas de layout do Android Auto (androidx.media.utils.MediaConstants).
    private const val ESTILO_TOCAVEL = "android.media.browse.CONTENT_STYLE_PLAYABLE_HINT"
    private const val ESTILO_PASTA = "android.media.browse.CONTENT_STYLE_BROWSABLE_HINT"
    private const val LISTA = 1
    private const val GRADE = 2
  }

  private lateinit var sessao: MediaSessionCompat
  private var capaCarregada: String? = null
  private var capa: Bitmap? = null
  private var emPrimeiroPlano = false
  /** Já passou por onStartCommand (senão foi aberto só pelo bind do Android Auto). */
  private var iniciado = false

  override fun onCreate() {
    super.onCreate()
    instancia = this
    val gerente = getSystemService(NotificationManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      gerente.createNotificationChannel(
        NotificationChannel(CANAL, "Reprodução", NotificationManager.IMPORTANCE_LOW).apply {
          description = "Música tocando e controles"
          setShowBadge(false)
        }
      )
    }
    sessao = MediaSessionCompat(this, "Skald").apply {
      setCallback(object : MediaSessionCompat.Callback() {
        override fun onPlay() = Midia.comando("tocar", 0)
        override fun onPause() = Midia.comando("pausar", 0)
        override fun onStop() = Midia.comando("pausar", 0)
        override fun onSkipToNext() = Midia.comando("proxima", 0)
        override fun onSkipToPrevious() = Midia.comando("anterior", 0)
        override fun onSeekTo(pos: Long) = Midia.comando("buscar", pos)
        override fun onPlayFromMediaId(mediaId: String?, extras: Bundle?) {
          if (mediaId == null) return
          if (Midia.rustCarregado) Midia.comando(mediaId, 0) else avisarAppFechado()
        }
      })
      setSessionActivity(abrirApp())
      isActive = true
    }
    sessionToken = sessao.sessionToken
  }

  // ---------- Android Auto ----------

  override fun onGetRoot(cliente: String, uid: Int, dicas: Bundle?): BrowserRoot {
    if (!Midia.rustCarregado) avisarAppFechado()
    return BrowserRoot(RAIZ, Bundle().apply {
      putInt(ESTILO_PASTA, LISTA)
      putInt(ESTILO_TOCAVEL, LISTA)
    })
  }

  override fun onLoadChildren(pai: String, resultado: Result<MutableList<MediaBrowserCompat.MediaItem>>) {
    if (!Midia.rustCarregado) {
      // Sem o Rust não há biblioteca: um item explica (e Midia.playlists recarrega ao abrir).
      val aviso = MediaDescriptionCompat.Builder()
        .setMediaId("abrir")
        .setTitle("Abra o Skáld no celular")
        .setSubtitle("A biblioteca aparece aqui depois disso")
        .build()
      resultado.sendResult(
        if (pai == RAIZ) mutableListOf(MediaBrowserCompat.MediaItem(aviso, MediaBrowserCompat.MediaItem.FLAG_PLAYABLE))
        else mutableListOf()
      )
      return
    }
    // Ler o banco e gerar miniaturas pode demorar: fora da thread principal.
    resultado.detach()
    Thread {
      val itens = runCatching { Midia.navegar(pai) }.getOrDefault("")
        .lineSequence().filter { it.isNotEmpty() }.mapNotNull { item(it) }.toMutableList()
      resultado.sendResult(itens)
    }.start()
  }

  private fun item(linha: String): MediaBrowserCompat.MediaItem? {
    val c = linha.split('\t')
    if (c.size < 5) return null
    val d = MediaDescriptionCompat.Builder()
      .setMediaId(c[0])
      .setTitle(c[1])
      .setSubtitle(c[2].ifEmpty { null })
      .setIconUri(c[3].takeIf { it.isNotEmpty() }?.let { uriDaCapa(it) })
      // A dica vale para os filhos da pasta: álbuns em grade, o resto em lista.
      .setExtras(Bundle().apply { putInt(ESTILO_TOCAVEL, if (c[0] == "albuns") GRADE else LISTA) })
      .build()
    val tipo = if (c[4] == "p") MediaBrowserCompat.MediaItem.FLAG_BROWSABLE else MediaBrowserCompat.MediaItem.FLAG_PLAYABLE
    return MediaBrowserCompat.MediaItem(d, tipo)
  }

  /** Capa local como content:// (FileProvider), legível pelo Android Auto. */
  private fun uriDaCapa(caminho: String): Uri? = runCatching {
    val uri = FileProvider.getUriForFile(this, "$packageName.fileprovider", File(caminho))
    grantUriPermission(ANDROID_AUTO, uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
    uri
  }.getOrNull()

  /** Sem o Rust neste processo não há como tocar: o Android Auto mostra a mensagem. */
  private fun avisarAppFechado() {
    sessao.setPlaybackState(
      PlaybackStateCompat.Builder()
        .setState(PlaybackStateCompat.STATE_ERROR, 0, 0f)
        .setErrorMessage(PlaybackStateCompat.ERROR_CODE_APP_ERROR, "Abra o Skáld no celular para tocar no carro.")
        .build()
    )
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    iniciado = true
    when (intent?.action) {
      "anterior", "alternar", "proxima" -> Midia.comando(intent.action!!, 0)
    }
    Midia.atual?.let { aplicar(it) } ?: run {
      // Iniciado sem estado (não deveria acontecer): cumpre o startForeground e sai.
      entrarEmPrimeiroPlano(construir(null))
      encerrar()
    }
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    instancia = null
    sessao.isActive = false
    sessao.release()
    super.onDestroy()
  }

  /** Publica o estado na sessão e atualiza a notificação. */
  fun aplicar(e: Midia.Estado) {
    if (e.capa != capaCarregada) {
      capaCarregada = e.capa
      capa = e.capa?.let { runCatching { BitmapFactory.decodeFile(it) }.getOrNull() }
    }
    sessao.setMetadata(
      MediaMetadataCompat.Builder()
        .putString(MediaMetadataCompat.METADATA_KEY_TITLE, e.titulo)
        .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, e.artista)
        .putString(MediaMetadataCompat.METADATA_KEY_ALBUM, e.album)
        .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, e.duracaoMs)
        .apply { capa?.let { putBitmap(MediaMetadataCompat.METADATA_KEY_ALBUM_ART, it) } }
        .build()
    )
    val tocando = e.estado == 1
    sessao.setPlaybackState(
      PlaybackStateCompat.Builder()
        .setActions(
          PlaybackStateCompat.ACTION_PLAY or PlaybackStateCompat.ACTION_PAUSE or
            PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
            PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or PlaybackStateCompat.ACTION_SEEK_TO or
            PlaybackStateCompat.ACTION_STOP
        )
        .setState(
          if (tocando) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED,
          e.posicaoMs, if (tocando) 1f else 0f, SystemClock.elapsedRealtime()
        )
        .build()
    )
    val n = construir(e)
    if (!emPrimeiroPlano) entrarEmPrimeiroPlano(n)
    else getSystemService(NotificationManager::class.java).notify(ID, n)
  }

  /** Nada carregado: tira a notificação e para o serviço. */
  fun encerrar() {
    ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
    emPrimeiroPlano = false
    iniciado = false
    stopSelf()
  }

  private fun entrarEmPrimeiroPlano(n: android.app.Notification) {
    // Aberto só pelo Android Auto (bind), o serviço ainda não foi "iniciado": sem isto ele
    // seria destruído quando o carro desconectar, mesmo tocando.
    if (!iniciado) ContextCompat.startForegroundService(this, Intent(this, MidiaService::class.java))
    ServiceCompat.startForeground(
      this, ID, n,
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK else 0
    )
    emPrimeiroPlano = true
  }

  private fun construir(e: Midia.Estado?): android.app.Notification {
    val tocando = e?.estado == 1
    return NotificationCompat.Builder(this, CANAL)
      .setSmallIcon(R.drawable.ic_notificacao)
      .setContentTitle(e?.titulo ?: "Skáld")
      .setContentText(e?.artista ?: "")
      .setSubText(e?.album?.takeIf { it.isNotEmpty() })
      .setLargeIcon(capa)
      .setContentIntent(abrirApp())
      .setOnlyAlertOnce(true)
      .setShowWhen(false)
      .setOngoing(tocando)
      .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
      .addAction(android.R.drawable.ic_media_previous, "Anterior", acao("anterior", 1))
      .addAction(
        if (tocando) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
        if (tocando) "Pausar" else "Tocar", acao("alternar", 2)
      )
      .addAction(android.R.drawable.ic_media_next, "Próxima", acao("proxima", 3))
      .setStyle(MediaStyle().setMediaSession(sessao.sessionToken).setShowActionsInCompactView(0, 1, 2))
      .build()
  }

  private fun acao(nome: String, codigo: Int): PendingIntent =
    PendingIntent.getService(
      this, codigo, Intent(this, MidiaService::class.java).setAction(nome),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )

  private fun abrirApp(): PendingIntent =
    PendingIntent.getActivity(
      this, 0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )
}
