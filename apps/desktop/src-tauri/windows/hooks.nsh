!include "x64.nsh"
!include "LogicLib.nsh"

; electron-builder registered the Electron build (0.1.x) under a GUID derived from the app id
; `com.jifercraft.launcher`. Tauri registers under the product name, so both would otherwise coexist.
!define LEGACY_ELECTRON_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\c7f2ce25-a403-564c-946e-a2af6fc12d96"

!macro NSIS_HOOK_PREINSTALL
  ; Preservamos los registros que utilizaremos para no romper variables globales del instalador
  Push $R0
  Push $R1
  Push $1

  ; 1. Minecraft moderno y Adoptium no tienen piedad con sistemas de 32 bits
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "JiferCraft Launcher requiere una versión de Windows de 64 bits (x64 o ARM64).$\r$\nMinecraft moderno y sus runtimes de Java ya no ofrecen soporte para arquitecturas de 32 bits."
    Abort
  ${EndIf}

  ; 2. Comprobamos el espacio libre para no dejar al usuario tirado a mitad de una descarga de 1 GB.
  ; Solo nos interesa la memoria libre disponible ($1), descartamos el total de disco con nulo.
  System::Call 'kernel32::GetDiskFreeSpaceEx(t "$LOCALAPPDATA", *l .r1, p 0, p 0)'
  System::Int64Op $1 < 1073741824
  Pop $R0
  ${If} $R0 != 0
    MessageBox MB_ICONEXCLAMATION "¡Ojo al disco! Te queda menos de 1 GB libre en tu unidad principal.$\r$\nMinecraft, los entornos Java y los assets necesitan al menos 1.5 a 2 GB para instalarse y correr con holgura. Te recomendamos hacer un poco de hueco para que todo vaya como la seda."
  ${EndIf}

  ; 3. Si javaw.exe se quedó colgado de una sesión anterior de Minecraft, le damos un toque.
  ; Usamos la ruta canónica $SYSDIR\cmd.exe para evitar ataques de PATH hijacking.
  nsExec::Exec '"$SYSDIR\cmd.exe" /c "tasklist /FI \"IMAGENAME eq javaw.exe\" /NH | findstr /I javaw.exe"'
  Pop $R0
  ${If} $R0 == 0
    MessageBox MB_ICONINFORMATION "Detectamos un proceso de Java (javaw.exe) activo en segundo plano.$\r$\nSi es una partida anterior colgada de Minecraft u otro launcher, te sugerimos cerrarla para evitar bloqueos en librerías nativas."
  ${EndIf}

  ; 4. Desinstalación limpia de la versión previa en Electron
  ; Per-user installs only: a per-machine Electron install would need elevation this installer does not have.
  ReadRegStr $R0 HKCU "${LEGACY_ELECTRON_KEY}" "QuietUninstallString"
  ${If} $R0 != ""
    DetailPrint "Removing the previous Electron-based JiferCraft Launcher..."
    ; Its uninstaller removes the program files and shortcuts only; %APPDATA%\.jifercloud (worlds,
    ; instances, settings) is never touched.
    ExecWait $R0
    ; The uninstaller relaunches itself from a temp copy and returns immediately, so wait for its
    ; registry entry to disappear (up to 30 s) before installing over the same shortcuts.
    StrCpy $R1 0
    ${Do}
      ReadRegStr $R0 HKCU "${LEGACY_ELECTRON_KEY}" "QuietUninstallString"
      ${If} $R0 == ""
      ${OrIf} $R1 >= 60
        ${Break}
      ${EndIf}
      Sleep 500
      IntOp $R1 $R1 + 1
    ${Loop}
  ${EndIf}

  ; Restauramos los registros en orden inverso exacto
  Pop $1
  Pop $R1
  Pop $R0
!macroend
