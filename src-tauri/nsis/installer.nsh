; --- Minagi 文件分类助手 NSIS 自定义脚本 ---
; 在卸载向导中添加「保留用户配置和数据」复选框
; 默认勾选；取消勾选则删除配置目录
; 桃华帮 Minagi 写的 Tauri 版 ✨

!ifdef BUILD_UNINSTALLER

  !include "nsDialogs.nsh"
  !include "LogicLib.nsh"

  ; ── 自定义卸载页面 ──
  !macro customUninstallPage
    UninstPage custom un.KeepDataCreate un.KeepDataLeave
  !macroend

  Var keepDataCheckbox

  Function un.KeepDataCreate
    nsDialogs::Create 1018

    ${NSD_CreateLabel} 0 0 100% 24u "卸载选项"
    Pop $0

    ${NSD_CreateLabel} 0 30u 100% 16u "Minagi 文件分类助手 程序文件已从您的计算机中移除。"
    Pop $0

    ${NSD_CreateCheckbox} 0 56u 100% 14u "保留用户配置和数据（区域路径、背景图、历史记录）"
    Pop $keepDataCheckbox
    ${NSD_SetState} $keepDataCheckbox ${BST_CHECKED}

    ${NSD_CreateLabel} 18u 74u 100% 12u "取消勾选将删除所有区域路径设置、背景图片与操作历史记录。"
    Pop $0

    nsDialogs::Show
  FunctionEnd

  Function un.KeepDataLeave
    ${NSD_GetState} $keepDataCheckbox $0
    ${If} $0 != ${BST_CHECKED}
      ; 删除用户数据目录
      RMDir /r "$APPDATA\minagi-file-classifier"
    ${EndIf}
  FunctionEnd

!endif
