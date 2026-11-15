package com.skald.app

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import org.freedesktop.gstreamer.GStreamer

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    // O GStreamer precisa do Context (plugins estáticos, certificados, fontes) antes
    // de o Rust criar o player; depois disso o gst::init() do Rust não faz nada.
    System.loadLibrary("gstreamer_android")
    GStreamer.init(this)
    super.onCreate(savedInstanceState)
  }
}
