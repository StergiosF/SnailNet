; SnailNet installer hooks (PLAN.md §17.1), included by the Tauri NSIS
; template through bundle.windows.nsis.installerHooks in
; tauri.release.conf.json. The template already has LogicLib and runs
; elevated (installMode perMachine), so these run as Administrator.
;
; The service exe registers and removes itself (`snailnet-service install`
; / `uninstall [--remove-data]`, crates/snailnet-service/src/install.rs).
; Starting and stopping go through net.exe, which waits until the service
; has actually started or stopped (sc.exe returns at once).

!define SNAILNET_SERVICE "SnailNet"

; Before files are copied: on an upgrade or an update (the updater runs
; this installer with /UPDATE) the running service holds
; snailnet-service.exe and WinDivert.dll open, so stop it first. Stopping
; closes the last WinDivert handle, which unloads the driver. Nothing to
; do on a fresh install; net.exe just fails quietly.
!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping the ${SNAILNET_SERVICE} service if it runs..."
  nsExec::ExecToLog '"$SYSDIR\net.exe" stop ${SNAILNET_SERVICE}'
  Pop $0
!macroend

; After files are copied: register (or update) the service to run the
; exe in $INSTDIR, which also creates C:\ProgramData\SnailNet with its
; ACLs, then start it. The finish page launches the app, whose first run
; turns on "Start with Windows" for that user.
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Registering the ${SNAILNET_SERVICE} service..."
  nsExec::ExecToLog '"$INSTDIR\snailnet-service.exe" install'
  Pop $0
  ${If} $0 != 0
    DetailPrint "snailnet-service install failed ($0)"
    ${IfNot} ${Silent}
      MessageBox MB_ICONEXCLAMATION|MB_OK "SnailNet could not register its background service (error $0).$\r$\n$\r$\nThe app will say the service isn't running. Running the installer again usually fixes it."
    ${EndIf}
  ${Else}
    DetailPrint "Starting the ${SNAILNET_SERVICE} service..."
    nsExec::ExecToLog '"$SYSDIR\net.exe" start ${SNAILNET_SERVICE}'
    Pop $0
    ${If} $0 != 0
      DetailPrint "net start ${SNAILNET_SERVICE} failed ($0)"
    ${EndIf}
  ${EndIf}
!macroend

; Before files are removed. In update mode the files are about to be
; replaced, so only stop the service (the new installer registers it
; again). Otherwise stop and remove it; ask whether to keep the settings
; in C:\ProgramData\SnailNet (rules, modes, network speeds). Silent and
; passive uninstalls (a reinstall from the installer's own page) keep them.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode = 1
    nsExec::ExecToLog '"$SYSDIR\net.exe" stop ${SNAILNET_SERVICE}'
    Pop $0
  ${ElseIf} ${FileExists} "$INSTDIR\snailnet-service.exe"
    StrCpy $1 ""
    ${IfNot} ${Silent}
    ${AndIf} $PassiveMode <> 1
      MessageBox MB_ICONQUESTION|MB_YESNO|MB_DEFBUTTON1 "Keep your SnailNet settings?$\r$\n$\r$\nYour rules, modes and measured speeds stay in C:\ProgramData\SnailNet, so a later install picks them up. Choose No to delete them." IDYES snailnet_keep
      StrCpy $1 " --remove-data"
      snailnet_keep:
    ${EndIf}
    DetailPrint "Removing the ${SNAILNET_SERVICE} service..."
    nsExec::ExecToLog '"$INSTDIR\snailnet-service.exe" uninstall$1'
    Pop $0
    ${If} $0 != 0
      DetailPrint "snailnet-service uninstall failed ($0)"
    ${EndIf}
  ${EndIf}
!macroend
