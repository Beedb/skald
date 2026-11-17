package com.skald.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/** Serviço em primeiro plano durante o login no Google e a sincronização pelo Drive. Sem ele a
 *  Samsung (Freecess) congela o processo em segundo plano: o retorno do navegador não chega e os
 *  envios param. O Rust liga e desliga por `Midia.segundoPlano(texto)`. */
class SyncService : Service() {
  companion object {
    private const val CANAL = "sincronizacao"
    private const val ID = 2
    private const val TEXTO = "texto"

    /** `texto` nulo encerra o serviço. */
    fun definir(ctx: Context, texto: String?) {
      val intent = Intent(ctx, SyncService::class.java)
      if (texto == null) ctx.stopService(intent)
      else ContextCompat.startForegroundService(ctx, intent.putExtra(TEXTO, texto))
    }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val gerente = getSystemService(NotificationManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      gerente.createNotificationChannel(
        NotificationChannel(CANAL, "Sincronização", NotificationManager.IMPORTANCE_LOW)
      )
    }
    val aviso = NotificationCompat.Builder(this, CANAL)
      .setSmallIcon(R.drawable.ic_notificacao)
      .setContentTitle("Skáld")
      .setContentText(intent?.getStringExtra(TEXTO) ?: "Sincronizando…")
      .setOngoing(true)
      .setSilent(true)
      .build()
    val tipo = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0
    ServiceCompat.startForeground(this, ID, aviso, tipo)
    return START_NOT_STICKY
  }
}
