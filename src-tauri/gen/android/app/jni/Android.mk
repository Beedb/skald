# Gera libgstreamer_android.so (GStreamer + plugins estáticos numa biblioteca só),
# à qual o Rust (skald_lib) se liga. Raiz do SDK em GSTREAMER_ROOT_ANDROID.
LOCAL_PATH := $(call my-dir)

ifndef GSTREAMER_ROOT_ANDROID
$(error GSTREAMER_ROOT_ANDROID não definido: veja docs/android.md)
endif

ifeq ($(TARGET_ARCH_ABI),armeabi-v7a)
GSTREAMER_ROOT := $(GSTREAMER_ROOT_ANDROID)/armv7
else ifeq ($(TARGET_ARCH_ABI),arm64-v8a)
GSTREAMER_ROOT := $(GSTREAMER_ROOT_ANDROID)/arm64
else ifeq ($(TARGET_ARCH_ABI),x86)
GSTREAMER_ROOT := $(GSTREAMER_ROOT_ANDROID)/x86
else ifeq ($(TARGET_ARCH_ABI),x86_64)
GSTREAMER_ROOT := $(GSTREAMER_ROOT_ANDROID)/x86_64
else
$(error ABI sem suporte: $(TARGET_ARCH_ABI))
endif

GSTREAMER_NDK_BUILD_PATH := $(GSTREAMER_ROOT)/share/gst-android/ndk-build/

# Só o que o Skáld usa: playbin, decodificadores dos formatos aceitos (MP3, FLAC,
# OGG/Opus, WAV; AAC via MediaCodec em androidmedia), equalizador, scaletempo (audiofx, velocidade dos podcasts), ReplayGain e saída OpenSL ES.
GSTREAMER_PLUGINS := coreelements typefindfunctions playback app \
	audioconvert audioresample volume audiorate autodetect \
	id3demux apetag audioparsers wavparse isomp4 ogg \
	mpg123 flac vorbis opus \
	equalizer replaygain level audiofx \
	opensles androidmedia
GSTREAMER_EXTRA_DEPS := gstreamer-audio-1.0 gstreamer-pbutils-1.0
GSTREAMER_JAVA_SRC_DIR := $(LOCAL_PATH)/../build/gstreamer/java
GSTREAMER_ASSETS_DIR := $(LOCAL_PATH)/../build/gstreamer/assets

include $(GSTREAMER_NDK_BUILD_PATH)/gstreamer-1.0.mk
