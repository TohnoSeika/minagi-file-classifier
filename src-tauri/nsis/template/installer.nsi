Unicode true
ManifestDPIAware true
; Add in `dpiAwareness` `PerMonitorV2` to manifest for Windows 10 1607+ (note this should not affect lower versions since they should be able to ignore this and pick up `dpiAware` `true` set by `ManifestDPIAware true`)
; Currently undocumented on NSIS's website but is in the Docs folder of source tree, see
; https://github.com/kichik/nsis/blob/5fc0b87b819a9eec006df4967d08e522ddd651c9/Docs/src/attributes.but#L286-L300
; https://github.com/tauri-apps/tauri/pull/10106
ManifestDPIAwareness PerMonitorV2

!if "lzma" == "none"
  SetCompress off
!else
  ; Set the compression algorithm. We default to LZMA.
  SetCompressor /SOLID "lzma"
!endif

!include MUI2.nsh
!include FileFunc.nsh
!include x64.nsh
!include WordFunc.nsh
!include "utils.nsh"
!include "FileAssociation.nsh"
!include "Win\COM.nsh"
!include "Win\Propkey.nsh"
!include "StrFunc.nsh"
${StrCase}
${StrLoc}
${StrRep}


!define WEBVIEW2APPGUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

!define MANUFACTURER "minagi-file-classifier"
!define PRODUCTNAME "Minagi 文件分类助手"
; 桃华修复：版本号必须由 Tauri 注入，写死会让安装包元数据永远停在旧版本
!define VERSION "{{version}}"
!define VERSIONWITHBUILD "{{version_with_build}}"
!define HOMEPAGE ""
!define INSTALLMODE "currentUser"
!define LICENSE ""
!define INSTALLERICON "{{installer_icon}}"
!define SIDEBARIMAGE ""
!define HEADERIMAGE ""
!define UNINSTALLERICON ""
!define UNINSTALLERHEADERIMAGE ""
!define MAINBINARYNAME "minagi-file-classifier"
!define MAINBINARYSRCPATH "{{main_binary_path}}"
!define BUNDLEID "minagi-file-classifier"
!define COPYRIGHT ""
!define OUTFILE "nsis-output.exe"
!define ARCH "x64"
!define ADDITIONALPLUGINSPATH "{{additional_plugins_path}}"
!define ALLOWDOWNGRADES "true"
!define DISPLAYLANGUAGESELECTOR "false"
!define INSTALLWEBVIEW2MODE "downloadBootstrapper"
!define WEBVIEW2INSTALLERARGS "/silent"
!define WEBVIEW2BOOTSTRAPPERPATH ""
!define WEBVIEW2INSTALLERPATH ""
!define MINIMUMWEBVIEW2VERSION ""
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUKEY "Software\${MANUFACTURER}"
!define MANUPRODUCTKEY "${MANUKEY}\${PRODUCTNAME}"
!define UNINSTALLERSIGNCOMMAND ""
!define ESTIMATEDSIZE "{{estimated_size}}"
!define STARTMENUFOLDER ""

Var PassiveMode
Var UpdateMode
Var NoShortcutMode
Var WixMode
; 桃华定制：标记旧版为 Electron NSIS 安装（区别于 WiX/MSI 和 Tauri NSIS）
Var ElectronMode
Var ElectronHive
Var OldMainBinaryName
; 桃华定制：保存旧安装路径，避免卸载后注册表丢失找不到
Var OldInstallPath

; ═══════════════════════════════════════════════════
; 桃华修复：旧 Electron 版残留清理
; 安装目录是用户自己选的，不能再对 $INSTDIR / $OldInstallPath 用
; *.dll、*.pak 这类通配符——选到 D:\Tools 这种共享目录时，
; 通配符会把别人的文件也一起删掉。这里只删能叫出名字的文件，
; 而且要先确认目录里真的有旧版 exe 才动手。
; ═══════════════════════════════════════════════════
!macro DeleteElectronLeftovers dir
  Delete "${dir}\Minagi文件分类助手.exe"
  Delete "${dir}\Uninstall Minagi文件分类助手.exe"
  ; Electron / Chromium 运行时文件
  Delete "${dir}\chrome_100_percent.pak"
  Delete "${dir}\chrome_200_percent.pak"
  Delete "${dir}\resources.pak"
  Delete "${dir}\icudtl.dat"
  Delete "${dir}\snapshot_blob.bin"
  Delete "${dir}\v8_context_snapshot.bin"
  Delete "${dir}\ffmpeg.dll"
  Delete "${dir}\libEGL.dll"
  Delete "${dir}\libGLESv2.dll"
  Delete "${dir}\d3dcompiler_47.dll"
  Delete "${dir}\dxcompiler.dll"
  Delete "${dir}\dxil.dll"
  Delete "${dir}\vk_swiftshader.dll"
  Delete "${dir}\vulkan-1.dll"
  Delete "${dir}\chrome_crashpad_handler.exe"
  Delete "${dir}\notification_helper.exe"
  Delete "${dir}\version"
  Delete "${dir}\vk_swiftshader_icd.json"
  Delete "${dir}\LICENSE"
  Delete "${dir}\LICENSES.chromium.html"
  RMDir /r "${dir}\resources"
  RMDir /r "${dir}\locales"
  RMDir /r "${dir}\swiftshader"
!macroend

!macro CleanOldElectronIfPresent dir
  ; 空路径或者盘根（D:\）一律不碰
  StrLen $R8 "${dir}"
  ${If} $R8 > 3
    ${If} ${FileExists} "${dir}\Minagi文件分类助手.exe"
    ${OrIf} ${FileExists} "${dir}\Uninstall Minagi文件分类助手.exe"
      !insertmacro DeleteElectronLeftovers "${dir}"
    ${EndIf}
  ${EndIf}
!macroend

Name "${PRODUCTNAME}"
BrandingText "${COPYRIGHT}"
OutFile "${OUTFILE}"

; We don't actually use this value as default install path,
; it's just for nsis to append the product name folder in the directory selector
; https://nsis.sourceforge.io/Reference/InstallDir
!define PLACEHOLDER_INSTALL_DIR "placeholder\${PRODUCTNAME}"
InstallDir "${PLACEHOLDER_INSTALL_DIR}"

VIProductVersion "${VERSIONWITHBUILD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

# additional plugins
!if "${ADDITIONALPLUGINSPATH}" != ""
  !addplugindir "${ADDITIONALPLUGINSPATH}"
!endif

; Uninstaller signing command
!if "${UNINSTALLERSIGNCOMMAND}" != ""
  !uninstfinalize '${UNINSTALLERSIGNCOMMAND}'
!endif

; Handle install mode, `perUser`, `perMachine` or `both`
!if "${INSTALLMODE}" == "perMachine"
  RequestExecutionLevel admin
!endif

!if "${INSTALLMODE}" == "currentUser"
  RequestExecutionLevel user
!endif

!if "${INSTALLMODE}" == "both"
  !define MULTIUSER_MUI
  !define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"
  !define MULTIUSER_INSTALLMODE_COMMANDLINE
  !if "${ARCH}" == "x64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !else if "${ARCH}" == "arm64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !endif
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_KEY "${UNINSTKEY}"
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_VALUENAME "CurrentUser"
  !define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
  !define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation
  !define MULTIUSER_EXECUTIONLEVEL Highest
  !include MultiUser.nsh
!endif

; Installer icon
!if "${INSTALLERICON}" != ""
  !define MUI_ICON "${INSTALLERICON}"
!endif

; Installer sidebar image
!if "${SIDEBARIMAGE}" != ""
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBARIMAGE}"
!endif

; Enable header images for installer and uninstaller pages when either image is configured.
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!else if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
!endif

; Installer header image
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_BITMAP "${HEADERIMAGE}"
!endif

; Uninstaller header image
!if "${UNINSTALLERHEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE_UNBITMAP "${UNINSTALLERHEADERIMAGE}"
!endif

; Uninstaller icon
!if "${UNINSTALLERICON}" != ""
  !define MUI_UNICON "${UNINSTALLERICON}"
!endif

; Define registry key to store installer language
!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

; Installer pages, must be ordered as they appear
; 1. Welcome Page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_WELCOME

; 2. License Page (if defined)
!if "${LICENSE}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MUI_PAGE_LICENSE "${LICENSE}"
!endif

; 3. Install mode (if it is set to `both`)
!if "${INSTALLMODE}" == "both"
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MULTIUSER_PAGE_INSTALLMODE
!endif

; 4. Custom page to ask user if he wants to reinstall/uninstall
;    only if a previous installation was detected

; 桃华定制：移除重装选择页面，改为在 .onInit 中自动静默处理旧版本
; ═══════════════════════════════════════════════════
; 桃华定制：自动处理旧版本安装（静默卸载后继续）
; 替代原来的 PageReinstall / PageLeaveReinstall 三个函数
; 无论全新安装还是覆盖安装，都不显示选择页面 ✨
; ═══════════════════════════════════════════════════
Function AutoHandlePreviousInstall
  ; 桃华定制：扫描 WiX 注册表（兼容旧 Electron 版 "Minagi 文件分类助手"）
  ; 使用 DisplayName 模糊匹配，同时检查 64 位和 32 位注册表视图
  ; ── 先查 64 位视图 ──
  ${If} ${RunningX64}
    SetRegView 64
  ${EndIf}
  StrCpy $0 0
  wix_scan:
    EnumRegKey $1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall" $0
    StrCmp $1 "" wix_scan_done
    IntOp $0 $0 + 1
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "DisplayName"
    ; 桃华定制：用 StrLoc 模糊匹配 DisplayName，不要求 Publisher 一致
    ${StrLoc} $R1 "$R0" "Minagi 文件分类助手" ">"
    StrCmp $R1 "" wix_scan
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
    ${StrCase} $R1 $R0 "L"
    ${StrLoc} $R0 $R1 "msiexec" ">"
    ; 桃华修复：StrLoc 找不到时返回空串，找到时返回位置（可能是 0）。
    ; 旧写法只在位置为 0 时继续，第一个同名项不是 MSI 就整轮扫描结束了
    StrCmp $R0 "" wix_scan
    StrCpy $WixMode 1
    StrCpy $R6 "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1"
    Goto uninstall_prev
  wix_scan_done:

  ; ── 再查 32 位视图（WOW6432Node，某些旧版 Electron 装在这里）──
  ${If} ${RunningX64}
    SetRegView 32
    StrCpy $0 0
    wix_scan32:
      EnumRegKey $1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall" $0
      StrCmp $1 "" wix_scan32_done
      IntOp $0 $0 + 1
      ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "DisplayName"
      ${StrLoc} $R1 "$R0" "Minagi 文件分类助手" ">"
      StrCmp $R1 "" wix_scan32
      ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
      ${StrCase} $R1 $R0 "L"
      ${StrLoc} $R0 $R1 "msiexec" ">"
      StrCmp $R0 "" wix_scan32
      StrCpy $WixMode 1
      StrCpy $R6 "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1"
      Goto uninstall_prev
    wix_scan32_done:
  ${EndIf}

  ; 桃华定制：检测旧 Electron NSIS 安装
  ; 旧版注册表键名是随机 GUID，无法直接读取
  ; 必须枚举 HKCU Uninstall 并匹配 DisplayName 和 UninstallString
  StrCpy $0 0
  electron_loop:
    EnumRegKey $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall" $0
    StrCmp $1 "" electron_scan_done
    IntOp $0 $0 + 1
    ; 检查 DisplayName 是否包含 "文件分类助手"（限定只匹配旧版分类助手，不误伤其他 Minagi 项目）
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$1" "DisplayName"
    ${StrLoc} $R1 "$R0" "文件分类助手" ">"
    StrCmp $R1 "" electron_loop
    ; 检查 UninstallString 是否包含 "Uninstall Minagi文件分类助手"（精确匹配旧版 Electron 分类助手）
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
    ${StrLoc} $R1 "$R0" "Uninstall Minagi文件分类助手" ">"
    StrCmp $R1 "" electron_loop
    ; 确认是 NSIS 类型（非 msiexec）
    ReadRegStr $R2 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
    ${StrCase} $R1 $R2 "L"
    ${StrLoc} $R0 $R1 "msiexec" ">"
    StrCmp $R0 "" +2      ; msiexec 未找到 → NSIS 类型，继续匹配
    Goto electron_loop     ; msiexec 找到 → WiX 类型，跳过
    ; 找到旧 Electron NSIS 安装
    StrCpy $ElectronMode 1
    StrCpy $ElectronHive "HKCU"
    ; 尝试读取 InstallLocation
    ReadRegStr $OldInstallPath HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$1" "InstallLocation"
    StrCmp $OldInstallPath "" electron_extract_from_uninst
    StrCpy $R6 "Software\Microsoft\Windows\CurrentVersion\Uninstall\$1"
    Goto uninstall_prev

  electron_extract_from_uninst:
    ; InstallLocation 为空（Electron Builder 不写此值），从 UninstallString 提取安装目录
    ; UninstallString 格式: "D:\path\to\Uninstall Minagi文件分类助手.exe" /currentuser
    ; 去除引号和参数，只保留 exe 完整路径
    ${StrRep} $R0 $R2 '"' ""
    ; 去除 /currentuser 参数
    ${StrLoc} $R1 $R0 "/" ">"
    StrCmp $R1 "" electron_get_parent
    IntOp $R1 $R1 - 1
    StrCpy $R0 $R0 $R1
  electron_get_parent:
    ; 从 exe 路径获取父目录
    ${GetParent} $R0 $OldInstallPath
    StrCpy $R6 "Software\Microsoft\Windows\CurrentVersion\Uninstall\$1"
    Goto uninstall_prev
  electron_scan_done:

  ; 检查是否有已安装的 Tauri/NSIS 版本
  ReadRegStr $R0 SHCTX "${UNINSTKEY}" ""
  ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
  ${If} "$R0$R1" == ""
    Return  ; 全新安装，无需处理
  ${EndIf}

  ; 版本比较 —— 降级检查
  ${If} $WixMode <> 1
    ReadRegStr $R0 SHCTX "${UNINSTKEY}" "DisplayVersion"
    nsis_tauri_utils::SemverCompare "${VERSION}" $R0
    Pop $R0
    !if "${ALLOWDOWNGRADES}" == "false"
      ${If} $R0 = -1
        MessageBox MB_ICONEXCLAMATION "$(newerVersionInstalled)"
        Quit
      ${EndIf}
    !endif
  ${EndIf}

uninstall_prev:
  HideWindow
  ClearErrors

  ${If} $WixMode = 1
    ; 桃华定制：在卸载旧 WiX 版之前，先读取并保存安装路径
    ReadRegStr $OldInstallPath HKLM "$R6" "InstallLocation"
    ReadRegStr $R1 HKLM "$R6" "UninstallString"
    ExecWait '$R1' $0
    ; 桃华定制：确保清除旧 WiX 注册表项
    ${If} ${RunningX64}
      SetRegView 64
    ${EndIf}
    DeleteRegKey HKLM "$R6"
    ${If} ${RunningX64}
      SetRegView 32
    ${EndIf}
    DeleteRegKey HKLM "$R6"
  ${ElseIf} $ElectronMode = 1
    ; 桃华定制：静默卸载旧 Electron NSIS 版
    ; 读取 UninstallString 并去除引号，避免 ExecWait 嵌套引号导致命令失败
    ${If} $ElectronHive == "HKLM64"
      SetRegView 64
      ReadRegStr $R1 HKLM "$R6" "UninstallString"
      SetRegView 32
    ${ElseIf} $ElectronHive == "HKLM"
      ReadRegStr $R1 HKLM "$R6" "UninstallString"
    ${Else}
      ReadRegStr $R1 HKCU "$R6" "UninstallString"
    ${EndIf}
    ; 去除引号和 /currentuser 参数，只保留 exe 路径
    ${StrRep} $R1 $R1 '"' ""
    ${StrLoc} $R2 $R1 "/" ">"
    StrCmp $R2 "" +3
    IntOp $R2 $R2 - 1
    StrCpy $R1 $R1 $R2
    ; ❌ 不执行旧卸载程序——旧版卸载程序可能有过度清理的 bug，会误删其他 Minagi 应用的快捷方式
    ; ExecWait '"$R1" /S' $0
    ; 直接用手动清理替代，下面的代码更彻底也更安全

    ; 桃华修复：不再用 *.dll / *.pak 通配符扫用户目录——
    ; $OldInstallPath 为空或指向盘根时，通配符会落到盘根上。
    ; 先确认目录里真有旧版 exe，再按清单删除
    !insertmacro CleanOldElectronIfPresent "$OldInstallPath"

    ; 清理旧版注册表项
    ${If} $ElectronHive == "HKLM64"
      SetRegView 64
      DeleteRegKey HKLM "$R6"
      SetRegView 32
    ${ElseIf} $ElectronHive == "HKLM"
      DeleteRegKey HKLM "$R6"
    ${Else}
      DeleteRegKey HKCU "$R6"
    ${EndIf}
    ; 清理旧版快捷方式
    RMDir /r "$SMPROGRAMS\Minagi文件分类助手"
    RMDir /r "$SMPROGRAMS\Minagi 文件分类助手"
    ; 清理旧版桌面快捷方式（无空格版本）
    Delete "$DESKTOP\Minagi文件分类助手.lnk"
  ${Else}
    ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
    ; 桃华定制：保存 NSIS 旧版的安装路径
    StrCpy $OldInstallPath $4
    ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
    StrCpy $R1 "$R1 /P /UPDATE _?=$4"  ; 静默卸载 + 保留用户配置
    ExecWait '$R1' $0
  ${EndIf}

  BringToFront

  ; 桃华定制：跳过卸载错误检查
  ; CheckIfAppIsRunning 已在安装段处理进程锁定，
  ; 旧版 Electron 通过强制文件清理处理，
  ; Tauri NSIS 静默卸载可能返回非零码但已成功，因此不再报错。
FunctionEnd

; 5. Choose install directory page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_DIRECTORY

; 6. Start menu shortcut page
Var AppStartMenuFolder
!if "${STARTMENUFOLDER}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !define MUI_STARTMENUPAGE_DEFAULTFOLDER "${STARTMENUFOLDER}"
!else
  !define MUI_PAGE_CUSTOMFUNCTION_PRE Skip
!endif
!insertmacro MUI_PAGE_STARTMENU Application $AppStartMenuFolder

; 7. Installation page
!insertmacro MUI_PAGE_INSTFILES

; 8. Finish page
;
; Don't auto jump to finish page after installation page,
; because the installation page has useful info that can be used debug any issues with the installer.
!define MUI_FINISHPAGE_NOAUTOCLOSE
; Use show readme button in the finish page as a button create a desktop shortcut
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(createDesktop)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateOrUpdateDesktopShortcut
; Show run app after installation.
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION RunMainBinary
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH

Function RunMainBinary
  nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
FunctionEnd

; Uninstaller Pages
; 1. Confirm uninstall page
Var DeleteAppDataCheckbox
Var DeleteAppDataCheckboxState
!define /ifndef WS_EX_LAYOUTRTL         0x00400000
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ConfirmShow
Function un.ConfirmShow ; Add add a `Delete app data` check box
  ; $1 inner dialog HWND
  ; $2 window DPI
  ; $3 style
  ; $4 x
  ; $5 y
  ; $6 width
  ; $7 height
  FindWindow $1 "#32770" "" $HWNDPARENT ; Find inner dialog
  System::Call "user32::GetDpiForWindow(p r1) i .r2"
  ${If} $(^RTL) = 1
    StrCpy $3 "${__NSD_CheckBox_EXSTYLE} | ${WS_EX_LAYOUTRTL}"
    IntOp $4 50 * $2
  ${Else}
    StrCpy $3 "${__NSD_CheckBox_EXSTYLE}"
    IntOp $4 0 * $2
  ${EndIf}
  IntOp $5 100 * $2
  IntOp $6 400 * $2
  IntOp $7 25 * $2
  IntOp $4 $4 / 96
  IntOp $5 $5 / 96
  IntOp $6 $6 / 96
  IntOp $7 $7 / 96
  System::Call 'user32::CreateWindowEx(i r3, w "${__NSD_CheckBox_CLASS}", w "$(deleteAppDataCustom)", i ${__NSD_CheckBox_STYLE}, i r4, i r5, i r6, i r7, p r1, i0, i0, i0) i .s'
  Pop $DeleteAppDataCheckbox
  SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $1
  SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $1 1
  ; 桃华定制：默认不勾选，即保留用户配置和数据
  SendMessage $DeleteAppDataCheckbox ${BM_SETCHECK} ${BST_UNCHECKED} 0
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE un.ConfirmLeave
Function un.ConfirmLeave
  SendMessage $DeleteAppDataCheckbox ${BM_GETCHECK} 0 0 $DeleteAppDataCheckboxState
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_PRE un.SkipIfPassive
!insertmacro MUI_UNPAGE_CONFIRM

; 2. Uninstalling Page
!insertmacro MUI_UNPAGE_INSTFILES

;Languages
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_RESERVEFILE_LANGDLL
  !include "English.nsh"

; 桃华修复：Tauri 每次打包都会在构建目录生成自己的 English.nsh，
; 模板里那份同名文件根本进不了打包流程。卸载页的复选框文案在这里定义才生效，
; 而且要和下面的逻辑一致：勾选 = 删除数据（默认不勾选 = 保留）
LangString deleteAppDataCustom ${LANG_ENGLISH} "删除应用数据（区域设置、背景图、历史记录）"

Function .onInit
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/NS" $NoShortcutMode
  ${IfNot} ${Errors}
    StrCpy $NoShortcutMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif

  !insertmacro SetContext

  ; 桃华定制：自动检测并静默卸载旧版本，不显示重装选择页面
  Call AutoHandlePreviousInstall

  ${If} $INSTDIR == "${PLACEHOLDER_INSTALL_DIR}"
    ; Set default install location
    !if "${INSTALLMODE}" == "perMachine"
      ${If} ${RunningX64}
        !if "${ARCH}" == "x64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else if "${ARCH}" == "arm64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else
          StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
        !endif
      ${Else}
        StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
      ${EndIf}
    !else if "${INSTALLMODE}" == "currentUser"
      StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
    !endif

    Call RestorePreviousInstallLocation
  ${EndIf}


  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_INIT
  !endif
FunctionEnd


Section EarlyChecks
  ; Abort silent installer if downgrades is disabled
  !if "${ALLOWDOWNGRADES}" == "false"
  ${If} ${Silent}
    ; If downgrading
    ${If} $R0 = -1
      System::Call 'kernel32::AttachConsole(i -1)i.r0'
      ${If} $0 <> 0
        System::Call 'kernel32::GetStdHandle(i -11)i.r0'
        System::call 'kernel32::SetConsoleTextAttribute(i r0, i 0x0004)' ; set red color
        FileWrite $0 "$(silentDowngrades)"
      ${EndIf}
      Abort
    ${EndIf}
  ${EndIf}
  !endif

SectionEnd

Section WebView2
  ; Check if Webview2 is already installed and skip this section
  ${If} ${RunningX64}
    ReadRegStr $4 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${Else}
    ReadRegStr $4 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}
  ${If} $4 == ""
    ReadRegStr $4 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}

  ${If} $4 == ""
    ; Webview2 installation
    ;
    ; Skip if updating
    ${If} $UpdateMode <> 1
      !if "${INSTALLWEBVIEW2MODE}" == "downloadBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        DetailPrint "$(webview2Downloading)"
        NSISdl::download "https://go.microsoft.com/fwlink/p/?LinkId=2124703" "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Pop $0
        ${If} $0 == "success"
          DetailPrint "$(webview2DownloadSuccess)"
        ${Else}
          DetailPrint "$(webview2DownloadError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "embedBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebview2Setup.exe" "${WEBVIEW2BOOTSTRAPPERPATH}"
        DetailPrint "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "offlineInstaller"
        Delete "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe" "${WEBVIEW2INSTALLERPATH}"
        DetailPrint "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        Goto install_webview2
      !endif

      Goto webview2_done

      install_webview2:
        DetailPrint "$(installingWebview2)"
        ; $6 holds the path to the webview2 installer
        ExecWait "$6 ${WEBVIEW2INSTALLERARGS} /install" $1
        ${If} $1 = 0
          DetailPrint "$(webview2InstallSuccess)"
        ${Else}
          DetailPrint "$(webview2InstallError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
      webview2_done:
    ${EndIf}
  ${Else}
    !if "${MINIMUMWEBVIEW2VERSION}" != ""
      ${VersionCompare} "${MINIMUMWEBVIEW2VERSION}" "$4" $R0
      ${If} $R0 = 1
        update_webview:
          DetailPrint "$(installingWebview2)"
          ${If} ${RunningX64}
            ReadRegStr $R1 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate" "path"
          ${Else}
            ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 == ""
            ReadRegStr $R1 HKCU "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 != ""
            ; Chromium updater docs: https://source.chromium.org/chromium/chromium/src/+/main:docs/updater/user_manual.md
            ; Modified from "HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Microsoft EdgeWebView\ModifyPath"
            ExecWait `"$R1" /install appguid=${WEBVIEW2APPGUID}&needsadmin=true` $1
            ${If} $1 = 0
              DetailPrint "$(webview2InstallSuccess)"
            ${Else}
              MessageBox MB_ICONEXCLAMATION|MB_ABORTRETRYIGNORE "$(webview2InstallError)" IDIGNORE ignore IDRETRY update_webview
              Quit
              ignore:
            ${EndIf}
          ${EndIf}
      ${EndIf}
    !endif
  ${EndIf}
SectionEnd

Section Install
  SetOutPath $INSTDIR

  !ifmacrodef NSIS_HOOK_PREINSTALL
    !insertmacro NSIS_HOOK_PREINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  ; Copy main executable
  File "${MAINBINARYSRCPATH}"

  ; Copy resources

  ; Copy external binaries

  ; Create file associations

  ; Register deep links

  ; Create uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Save $INSTDIR in registry for future installations
  WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR

  !if "${INSTALLMODE}" == "both"
    ; Save install mode to be selected by default for the next installation such as updating
    ; or when uninstalling
    WriteRegStr SHCTX "${UNINSTKEY}" $MultiUser.InstallMode 1
  !endif

  ; Remove old main binary if it doesn't match new main binary name
  ReadRegStr $OldMainBinaryName SHCTX "${UNINSTKEY}" "MainBinaryName"
  ${If} $OldMainBinaryName != ""
  ${AndIf} $OldMainBinaryName != "${MAINBINARYNAME}.exe"
    Delete "$INSTDIR\$OldMainBinaryName"
  ${EndIf}
  ; 桃华修复：只有目录里确实躺着旧 Electron 版时才清理，
  ; 而且只删已知文件名，避免误伤共享安装目录里的其他程序
  !insertmacro CleanOldElectronIfPresent "$INSTDIR"

  ; 桃华定制：枚举桌面快捷方式，删除目标路径位于本安装目录内的旧快捷方式（包括乱码名称）
  ; 通配符无法匹配 ANSI 模式创建的乱码文件名，必须枚举后检查目标路径的父目录
  FindFirst $R0 $R1 "$DESKTOP\*.lnk"
  desk_loop:
    StrCmp $R1 "" desk_done
    Push $R0
    Push $R1
    ; 读取快捷方式目标路径
    StrCpy $R9 0
    !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r4 ""
    ${If} $4 P<> 0
      ${IUnknown::QueryInterface} $4 '("${IID_IPersistFile}", .r5)'
      ${If} $5 P<> 0
        ${IPersistFile::Load} $5 '("$DESKTOP\$R1", ${STGM_READ})'
        System::Alloc 1024
        Pop $6
        ${IShellLink::GetPath} $4 '(.r6, 1024, 0, ${SLGP_RAWPATH})'
        ; 获取目标路径的父目录
        ${GetParent} $6 $R3
        StrCmp $R3 "$INSTDIR" desk_delete 0
        ${If} $OldInstallPath != ""
          StrCmp $R3 $OldInstallPath desk_delete 0
        ${EndIf}
        Goto desk_skip
        desk_delete:
        StrCpy $R9 1
        desk_skip:
        System::Free $6
        ${IUnknown::Release} $5 ""
      ${EndIf}
      ${IUnknown::Release} $4 ""
    ${EndIf}
    ${If} $R9 = 1
      Pop $R1
      Pop $R0
      Delete "$DESKTOP\$R1"
      FindNext $R0 $R1
      Goto desk_loop
    ${EndIf}
    Pop $R1
    Pop $R0
    FindNext $R0 $R1
    Goto desk_loop
  desk_done:

  ; 开始菜单：清理旧版快捷方式
  RMDir /r "$SMPROGRAMS\Minagi文件分类助手"
  RMDir /r "$SMPROGRAMS\Minagi 文件分类助手"
  Delete "$SMPROGRAMS\Minagi文件分类助手.lnk"
  Delete "$SMPROGRAMS\Minagi 文件分类助手.lnk"
  ; 同时清理 All Users 下的旧快捷方式
  SetShellVarContext all
  FindFirst $R0 $R1 "$DESKTOP\*.lnk"
  alldesk_loop:
    StrCmp $R1 "" alldesk_done
    Push $R0
    Push $R1
    StrCpy $R9 0
    !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r4 ""
    ${If} $4 P<> 0
      ${IUnknown::QueryInterface} $4 '("${IID_IPersistFile}", .r5)'
      ${If} $5 P<> 0
        ${IPersistFile::Load} $5 '("$DESKTOP\$R1", ${STGM_READ})'
        System::Alloc 1024
        Pop $6
        ${IShellLink::GetPath} $4 '(.r6, 1024, 0, ${SLGP_RAWPATH})'
        ${GetParent} $6 $R3
        StrCmp $R3 "$INSTDIR" alldesk_delete 0
        ${If} $OldInstallPath != ""
          StrCmp $R3 $OldInstallPath alldesk_delete 0
        ${EndIf}
        Goto alldesk_skip
        alldesk_delete:
        StrCpy $R9 1
        alldesk_skip:
        System::Free $6
        ${IUnknown::Release} $5 ""
      ${EndIf}
      ${IUnknown::Release} $4 ""
    ${EndIf}
    ${If} $R9 = 1
      Pop $R1
      Pop $R0
      Delete "$DESKTOP\$R1"
      FindNext $R0 $R1
      Goto alldesk_loop
    ${EndIf}
    Pop $R1
    Pop $R0
    FindNext $R0 $R1
    Goto alldesk_loop
  alldesk_done:
  RMDir /r "$SMPROGRAMS\Minagi文件分类助手"
  RMDir /r "$SMPROGRAMS\Minagi 文件分类助手"
  Delete "$SMPROGRAMS\Minagi文件分类助手.lnk"
  Delete "$SMPROGRAMS\Minagi 文件分类助手.lnk"
  SetShellVarContext current

  ; 桃华修复：快捷方式交给下面的 CreateOrUpdate* 统一创建。
  ; 以前这里先无条件建一遍，完成页的「创建桌面快捷方式」复选框
  ; 和 /NS 参数就都形同虚设了
  ; Save current MAINBINARYNAME for future updates
  WriteRegStr SHCTX "${UNINSTKEY}" "MainBinaryName" "${MAINBINARYNAME}.exe"

  ; Registry information for add/remove programs
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr SHCTX "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
  WriteRegStr SHCTX "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoModify" "1"
  WriteRegDWORD SHCTX "${UNINSTKEY}" "NoRepair" "1"

  ${GetSize} "$INSTDIR" "/M=uninstall.exe /S=0K /G=0" $0 $1 $2
  IntOp $0 $0 + ${ESTIMATEDSIZE}
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD SHCTX "${UNINSTKEY}" "EstimatedSize" "$0"

  !if "${HOMEPAGE}" != ""
    WriteRegStr SHCTX "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
    WriteRegStr SHCTX "${UNINSTKEY}" "URLUpdateInfo" "${HOMEPAGE}"
    WriteRegStr SHCTX "${UNINSTKEY}" "HelpLink" "${HOMEPAGE}"
  !endif

  ; Create start menu shortcut
  !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
    Call CreateOrUpdateStartMenuShortcut
  !insertmacro MUI_STARTMENU_WRITE_END

  ; Create desktop shortcut for silent and passive installers
  ; because finish page will be skipped
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    Call CreateOrUpdateDesktopShortcut
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTINSTALL
    !insertmacro NSIS_HOOK_POSTINSTALL
  !endif

  ; Auto close this page for passive mode
  ${If} $PassiveMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function .onInstSuccess
  ; ── 桃华注：AppData 共用 minagi-file-classifier，无需迁移标记 ──

  ; Check for `/R` flag only in silent and passive installers because
  ; GUI installer has a toggle for the user to (re)start the app
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    ${GetOptions} $CMDLINE "/R" $R0
    ${IfNot} ${Errors}
      ${GetOptions} $CMDLINE "/ARGS" $R0
      nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" "$R0"
    ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  !insertmacro SetContext

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_UNINIT
  !endif

  !insertmacro MUI_UNGETLANGUAGE

  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
FunctionEnd

Section Uninstall

  !ifmacrodef NSIS_HOOK_PREUNINSTALL
    !insertmacro NSIS_HOOK_PREUNINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  ; Delete the app directory and its content from disk
  ; Copy main executable
  Delete "$INSTDIR\${MAINBINARYNAME}.exe"

  ; Delete resources

  ; Delete external binaries

  ; Delete app associations

  ; Delete deep links


  ; Delete uninstaller
  Delete "$INSTDIR\uninstall.exe"

  ; 桃华修复：安装目录是用户自己选的，不能整个目录强删。
  ; 只清掉本程序的文件，目录空了才删——共享目录里的其他内容要留下
  RMDir "$INSTDIR"

  ; Remove shortcuts if not updating
  ${If} $UpdateMode <> 1
    !insertmacro DeleteAppUserModelId

    ; Remove start menu shortcut
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    ${EndIf}
    !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${EndIf}

    ; Remove desktop shortcuts
    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$DESKTOP\${PRODUCTNAME}.lnk"
      Delete "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}

  ; Remove registry information for add/remove programs
  !if "${INSTALLMODE}" == "both"
    DeleteRegKey SHCTX "${UNINSTKEY}"
  !else if "${INSTALLMODE}" == "perMachine"
    DeleteRegKey HKLM "${UNINSTKEY}"
  !else
    DeleteRegKey HKCU "${UNINSTKEY}"
  !endif

  ; Removes the Autostart entry for ${PRODUCTNAME} from the HKCU Run key if it exists.
  ; This ensures the program does not launch automatically after uninstallation if it exists.
  ; If it doesn't exist, it does nothing.
  ; We do this when not updating (to preserve the registry value on updates)
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  ${EndIf}

  ; 桃华定制：勾选「Delete the application data」时删除数据
  ; 默认不勾选 = 保留数据
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ; Clear the install location $INSTDIR from registry
    DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty SHCTX "${MANUKEY}"

    ; Clear the install language from registry
    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
    DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${MANUKEY}"

    SetShellVarContext current
    RmDir /r "$APPDATA\${BUNDLEID}"
    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTUNINSTALL
    !insertmacro NSIS_HOOK_POSTUNINSTALL
  !endif

  ; Auto close if passive mode or updating
  ${If} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function RestorePreviousInstallLocation
  ; 桃华定制：优先使用卸载前保存的旧安装路径
  ${If} $OldInstallPath != ""
    StrCpy $INSTDIR $OldInstallPath
    Return
  ${EndIf}

  ; 优先查找 Tauri 版自己的安装记录
  ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" 0 restore_done

  ; 桃华定制：从 AppData 标记文件恢复安装路径（卸载后重装场景）
  SetShellVarContext current
  ${If} ${FileExists} "$APPDATA\minagi-file-classifier\MinagiData\.install_path"
    FileOpen $5 "$APPDATA\minagi-file-classifier\MinagiData\.install_path" r
    FileRead $5 $4
    FileClose $5
    ${StrRep} $4 $4 "$\r$\n" ""
    ${StrRep} $4 $4 "$\n" ""
    StrCmp $4 "" 0 restore_done
  ${EndIf}
  ${If} ${FileExists} "$APPDATA\Minagi文件分类助手\MinagiData\.install_path"
    FileOpen $5 "$APPDATA\Minagi文件分类助手\MinagiData\.install_path" r
    FileRead $5 $4
    FileClose $5
    ${StrRep} $4 $4 "$\r$\n" ""
    ${StrRep} $4 $4 "$\n" ""
    StrCmp $4 "" 0 restore_done
  ${EndIf}

  ; 回退：枚举 HKCU Uninstall，匹配 "Uninstall Minagi文件分类助手" 并从 UninstallString 提取路径
  StrCpy $0 0
  ${Do}
    EnumRegKey $1 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall" $0
    StrCmp $1 "" done_scan
    IntOp $0 $0 + 1
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
    ${StrLoc} $R1 "$R0" "Uninstall Minagi文件分类助手" ">"
    StrCmp $R1 "" next_key_hkcu
    ; 从 UninstallString 提取安装目录
    ${StrRep} $R0 $R0 '"' ""
    ${StrLoc} $R1 $R0 "/" ">"
    StrCmp $R1 "" restore_extract_parent
    IntOp $R1 $R1 - 1
    StrCpy $R0 $R0 $R1
  restore_extract_parent:
    ${GetParent} $R0 $4
    Goto restore_done
next_key_hkcu:
  ${Loop}

done_scan:
  Goto restore_end

restore_done:
  StrCpy $INSTDIR $4
restore_end:
FunctionEnd

Function Skip
  Abort
FunctionEnd

Function SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd
Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd

Function CreateOrUpdateStartMenuShortcut
  ; We used to use product name as MAINBINARYNAME
  ; migrate old shortcuts to target the new MAINBINARYNAME
  StrCpy $R0 0

  !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  ${If} $R0 = 1
    Return
  ${EndIf}

  ; Skip creating shortcut if in update mode or no shortcut mode
  ; but always create if migrating from wix
  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  !if "${STARTMENUFOLDER}" != ""
    CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  !else
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !endif
FunctionEnd

Function CreateOrUpdateDesktopShortcut
  ; We used to use product name as MAINBINARYNAME
  ; migrate old shortcuts to target the new MAINBINARYNAME
  !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Return
  ${EndIf}

  ; Skip creating shortcut if in update mode or no shortcut mode
  ; but always create if migrating from wix
  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
FunctionEnd
