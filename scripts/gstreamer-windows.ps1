# Copia para src-tauri/gstreamer/ só o que o Skáld usa do GStreamer (MSVC x86_64):
# os plugins da lista abaixo e, de forma recursiva, as DLLs de que eles dependem.
# O instalador (tauri.windows.conf.json) põe bin/ ao lado do skald.exe e plugins/ em
# gstreamer-1.0/. Rodar antes de `npm run tauri build` no Windows (npm run build:windows).

param(
    [string]$GstRoot = $env:GSTREAMER_1_0_ROOT_MSVC_X86_64
)

$ErrorActionPreference = 'Stop'

if (-not $GstRoot -or -not (Test-Path (Join-Path $GstRoot 'bin\gstreamer-1.0-0.dll'))) {
    throw "GStreamer (MSVC x86_64) não encontrado. Defina GSTREAMER_1_0_ROOT_MSVC_X86_64 ou passe -GstRoot."
}

# dumpbin vem com o Visual Studio Build Tools (o mesmo que o Rust usa para linkar).
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$dumpbin = Get-ChildItem (Join-Path $vs 'VC\Tools\MSVC') -Recurse -Filter dumpbin.exe |
    Where-Object FullName -like '*Hostx64\x64*' | Select-Object -First 1 -ExpandProperty FullName
if (-not $dumpbin) { throw 'dumpbin.exe não encontrado (instale o VS Build Tools com C++).' }

# Elementos usados: playbin/uridecodebin, fakesink, equalizer-10bands, rganalysis,
# audioconvert/audioresample, saída de áudio e os decodificadores dos formatos aceitos
# (MP3, FLAC, OGG/Opus/Vorbis, M4A/AAC, WAV).
$plugins = @(
    'coreelements', 'playback', 'typefindfunctions',
    'audioconvert', 'audioresample', 'volume',
    'autodetect', 'wasapi', 'wasapi2', 'directsound',
    'equalizer', 'replaygain',
    'audioparsers', 'id3demux', 'apetag',
    'mpg123', 'flac', 'ogg', 'vorbis', 'opus', 'opusparse',
    'isomp4', 'libav', 'wavparse'
)

$destino = Join-Path $PSScriptRoot '..\src-tauri\gstreamer'
$destBin = Join-Path $destino 'bin'
$destPlugins = Join-Path $destino 'plugins'
if (Test-Path $destino) { Remove-Item $destino -Recurse -Force }
New-Item -ItemType Directory -Force $destBin, $destPlugins | Out-Null

$fila = [System.Collections.Generic.Queue[string]]::new()
$copiadas = @{}
$sistema = @{}

foreach ($p in $plugins) {
    $arq = Join-Path $GstRoot "lib\gstreamer-1.0\gst$p.dll"
    if (-not (Test-Path $arq)) { throw "Plugin ausente: gst$p.dll" }
    Copy-Item $arq $destPlugins
    $fila.Enqueue($arq)
}
# O que o próprio skald.exe importa do GStreamer.
foreach ($dll in 'gstreamer-1.0-0.dll', 'gobject-2.0-0.dll', 'glib-2.0-0.dll', 'gio-2.0-0.dll') {
    $fila.Enqueue((Join-Path $GstRoot "bin\$dll"))
}

while ($fila.Count -gt 0) {
    $arq = $fila.Dequeue()
    $nome = Split-Path $arq -Leaf
    if ($arq -like "$GstRoot*bin*") {
        if ($copiadas.ContainsKey($nome.ToLower())) { continue }
        $copiadas[$nome.ToLower()] = $true
        Copy-Item $arq $destBin
    }
    $deps = & $dumpbin /nologo /dependents $arq | ForEach-Object { $_.Trim() } | Where-Object { $_ -match '^[^\s\\:]+\.dll$' }
    foreach ($d in $deps) {
        $local = Join-Path $GstRoot "bin\$d"
        if (Test-Path $local) {
            if (-not $copiadas.ContainsKey($d.ToLower())) { $fila.Enqueue($local) }
        } else {
            $sistema[$d.ToLower()] = $true
        }
    }
}

# Dependências fora do GStreamer precisam existir em qualquer Windows 10.
$faltam = $sistema.Keys | Where-Object {
    $_ -notlike 'api-ms-win-*' -and $_ -notlike 'ext-ms-*' -and
    -not (Test-Path (Join-Path $env:SystemRoot "System32\$_"))
}
if ($faltam) { Write-Warning ("Fora do Windows e do GStreamer: " + ($faltam -join ', ')) }

$mb = [math]::Round(((Get-ChildItem $destino -Recurse -File | Measure-Object Length -Sum).Sum / 1MB), 1)
Write-Host "GStreamer pronto: $($copiadas.Count) DLLs, $($plugins.Count) plugins, $mb MB em $destino"
