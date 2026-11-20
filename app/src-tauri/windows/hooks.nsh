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

!define SNAILNET_DRIVER "WinDivert"

; Unload the WinDivert driver so WinDivert64.sys can be replaced or
; removed. Closing the last WinDivert handle does NOT unload it: WinDivert
; 2.2 leaves its kernel service running until `sc stop WinDivert` or a
; restart, and Windows keeps a loaded driver's file locked ("Error opening
; file for writing: ...\WinDivert64.sys" on the v0.0.6 -> v0.0.7 update).
; Call after the SnailNet service has stopped (it holds the handles).
; sc.exe returns at once, so wait up to 5 s for the file to come free. If
; it never does (another program uses WinDivert too), move the file aside:
; Windows lets a loaded driver's file be renamed, the new one is written
; under the real name, and the old one goes at the next restart.
!macro SNAILNET_UNLOAD_DRIVER
  Push $2
  Push $3
  DetailPrint "Unloading the ${SNAILNET_DRIVER} driver..."
  nsExec::ExecToLog '"$SYSDIR\sc.exe" stop ${SNAILNET_DRIVER}'
  Pop $2
  Delete "$INSTDIR\WinDivert64.sys.old"
  ${If} ${FileExists} "$INSTDIR\WinDivert64.sys"
    StrCpy $2 0
    ${Do}
      ClearErrors
      FileOpen $3 "$INSTDIR\WinDivert64.sys" a
      ${IfNot} ${Errors}
        FileClose $3
        ${ExitDo}
      ${EndIf}
      IntOp $2 $2 + 1
      ${If} $2 >= 25
        DetailPrint "${SNAILNET_DRIVER} is still loaded; replacing its file after a restart."
        Rename "$INSTDIR\WinDivert64.sys" "$INSTDIR\WinDivert64.sys.old"
        Delete /REBOOTOK "$INSTDIR\WinDivert64.sys.old"
        ${ExitDo}
      ${EndIf}
      Sleep 200
    ${Loop}
  ${EndIf}
  Pop $3
  Pop $2
!macroend

; Before files are copied: on an upgrade or an update (the updater runs
; this installer with /UPDATE) the running service holds
; snailnet-service.exe and WinDivert.dll open, so stop it first, then
; unload the driver (above). Nothing to do on a fresh install; net.exe and
; sc.exe just fail quietly.
!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Stopping the ${SNAILNET_SERVICE} service if it runs..."
  nsExec::ExecToLog '"$SYSDIR\net.exe" stop ${SNAILNET_SERVICE}'
  Pop $0
  !insertmacro SNAILNET_UNLOAD_DRIVER
  ; Earlier installs left the app as snailnet.exe. Copying SnailNet.exe
  ; over it keeps the old lower-case name (NTFS keeps a file's case), and
  ; Task Manager then lists the app as "snailnet". Rename it first.
  ${If} ${FileExists} "$INSTDIR\snailnet.exe"
    Rename "$INSTDIR\snailnet.exe" "$INSTDIR\SnailNet.exe"
  ${EndIf}
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
    !insertmacro SNAILNET_UNLOAD_DRIVER
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
    ; The service is gone, so its handles are closed: unload the driver so
    ; WinDivert64.sys can be deleted with the rest.
    !insertmacro SNAILNET_UNLOAD_DRIVER
  ${EndIf}
!macroend
