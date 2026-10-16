; AutoFolder 설치 프로그램에 끼워 넣는 부분(tauri.conf.json → bundle.windows.nsis.installerHooks).
;
; 예전 이름 "파일 정리 도우미"로 설치된 것을 AutoFolder로 넘긴다.
; 설치 프로그램은 이름으로 설치 폴더·"프로그램 추가/제거" 항목·바로 가기·자동 실행을 만들기 때문에,
; 이름만 바꾸면 업데이트 때 예전 것이 옆에 그대로 남는다(바로 가기를 누르면 예전 버전이 켜짐).
;
; 지키는 것:
; - 사용자 자료(설정·규칙·기록 = %APPDATA%\com.hwichemi.file-organizer)는 건드리지 않는다.
;   식별자가 같아서 새 이름의 프로그램이 그대로 이어 쓴다.
; - 예전 설치 폴더는 통째로 지우지 않는다(사용자가 고른 폴더일 수 있음).
;   우리가 넣었던 파일만 지우고, 빈 폴더일 때만 폴더를 지운다. 새 설치 폴더와 같으면 아무것도 지우지 않는다.
; - 바로 가기는 지우지 않고 이름만 AutoFolder로 바꿔 새 실행 파일을 가리키게 한다.
;   자동 업데이트는 /UPDATE로 설치해서 바로 가기를 새로 만들지 않기 때문(지우기만 하면 바로 가기가 사라짐).
;   사용자가 예전에 지운 바로 가기는 새로 만들지 않는다.

!define OLD_PRODUCTNAME "파일 정리 도우미"
!define OLD_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${OLD_PRODUCTNAME}"
; 예전 MANUFACTURER(= publisher)도 "파일 정리 도우미"였다
!define OLD_MANUKEY "Software\${OLD_PRODUCTNAME}"
!define OLD_MANUPRODUCTKEY "${OLD_MANUKEY}\${OLD_PRODUCTNAME}"
!define RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define STARTUP_APPROVED_RUNKEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"

; DIR 안의 예전 바로 가기(예전 실행 파일을 가리킬 때만)를 AutoFolder 바로 가기로 바꾼다. $R8 = 예전 설치 폴더
!macro AF_MOVE_SHORTCUT DIR
  !insertmacro IsShortcutTarget "${DIR}\${OLD_PRODUCTNAME}.lnk" "$R8\${MAINBINARYNAME}.exe"
  Pop $0
  ${If} $0 = 1
    ${If} ${FileExists} "${DIR}\${PRODUCTNAME}.lnk"
      Delete "${DIR}\${OLD_PRODUCTNAME}.lnk"
    ${Else}
      Rename "${DIR}\${OLD_PRODUCTNAME}.lnk" "${DIR}\${PRODUCTNAME}.lnk"
      !insertmacro SetShortcutTarget "${DIR}\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "${DIR}\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; 예전 설치 폴더(따옴표 없이 적혀 있음)
  ReadRegStr $R8 HKCU "${OLD_MANUPRODUCTKEY}" ""
  ${If} $R8 != ""
    ; 예전 프로그램이 켜져 있으면 끈다(업데이트 중이면 보통 이미 꺼져 있다)
    !insertmacro CheckIfAppIsRunning "$R8\${MAINBINARYNAME}.exe" "${OLD_PRODUCTNAME}"

    ; 시작 메뉴·바탕화면 바로 가기 → AutoFolder
    !insertmacro AF_MOVE_SHORTCUT "$SMPROGRAMS"
    !insertmacro AF_MOVE_SHORTCUT "$DESKTOP"
    ; 작업 표시줄에 고정한 것: 이름은 그대로 두고 새 실행 파일을 가리키게만(고정이 풀리지 않게)
    !insertmacro IsShortcutTarget "$APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\${OLD_PRODUCTNAME}.lnk" "$R8\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro SetShortcutTarget "$APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\${OLD_PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    ${EndIf}

    ; 예전 설치 폴더의 파일(우리가 넣은 것만). 새 설치 폴더와 같으면 건너뛴다.
    ${If} $R8 != $INSTDIR
      Delete "$R8\${MAINBINARYNAME}.exe"
      Delete "$R8\uninstall.exe"
      Delete "$R8\resources\THIRD_PARTY.txt"
      Delete "$R8\resources\TERMS.txt"
      Delete "$R8\resources\Everything64.dll"
      RMDir "$R8\resources"
      RMDir "$R8"
    ${EndIf}

    ; "프로그램 추가/제거"의 예전 항목과 예전 설치 기록
    DeleteRegKey HKCU "${OLD_UNINSTKEY}"
    DeleteRegKey HKCU "${OLD_MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${OLD_MANUKEY}"
  ${EndIf}

  ; 예전 이름의 자동 실행. 프로그램이 켜질 때 설정에 맞춰 새 이름(AutoFolder)으로 다시 등록한다.
  DeleteRegValue HKCU "${RUNKEY}" "${OLD_PRODUCTNAME}"
  DeleteRegValue HKCU "${STARTUP_APPROVED_RUNKEY}" "${OLD_PRODUCTNAME}"
!macroend
