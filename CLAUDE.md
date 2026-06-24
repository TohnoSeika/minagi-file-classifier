# Minagi 文件分类助手 —— Tauri 版

这是桃华帮 Minagi 从 Electron 迁移到 Tauri 的文件分类工具。更轻、更快、更优雅 ✨

## 项目结构

```
Minagi_File_Classifier_Tauri_Builder_20260616-2.0/
├── src/                          # 前端 (HTML/CSS/JS)
│   ├── index.html               # 主页面
│   ├── scripts/                 # JS 脚本
│   └── styles/                  # CSS 样式
├── src-tauri/                    # Tauri 后端 (Rust)
│   ├── tauri.conf.json          # Tauri 配置（唯一配置源）
│   ├── Cargo.toml               # Rust 依赖
│   ├── src/                     # Rust 源码
│   ├── nsis/                    # NSIS 安装器定制
│   │   ├── installer.nsh        # 自定义卸载页面
│   │   └── template/            # NSIS 模板文件
│   │       ├── installer.nsi    # ★ 主安装脚本（自定义模板）
│   │       ├── English.nsh      # 英文文案
│   │       ├── FileAssociation.nsh
│   │       └── utils.nsh
│   └── icons/                   # 应用图标
├── build.ps1                    # ★ 固化打包脚本
├── LICENSE                      # MIT 许可证
├── README.md                    # 项目说明
├── .gitignore                   # Git 忽略规则
├── CLAUDE.md                    # 本文件
└── dist/                        # 打包输出目录
```

## 🔨 打包规则（非常重要！）

### 必须使用 build.ps1

**打包命令永远只有一条：**

```powershell
.\build.ps1
```

**禁止**手敲 `cargo tauri build` 或任何其他打包命令。`build.ps1` 锁死了所有步骤，保证每次输出完全一致。

### 脚本做了什么

1. 从 `tauri.conf.json` 读取版本号
2. 验证关键配置文件完整
3. 清理上次 NSIS 残留
4. 执行 `cargo tauri build --target x86_64-pc-windows-msvc`
5. 复制安装包到 `dist/`，命名为 `Minagi_文件分类助手_v{版本号}.exe`

### 版本号规则

- 版本号**唯一来源**：`src-tauri/tauri.conf.json` 的 `version` 字段
- `src-tauri/Cargo.toml` 的 `version` 必须保持一致
- 脚本自动读取，无需手动传参

### NSIS 安装器模板

- 自定义模板位于 `src-tauri/nsis/template/installer.nsi`
- `tauri.conf.json` 中 `bundle.windows.nsis.template` 指向此文件
- 如需修改安装逻辑，**只改这个模板文件**，并记录修改理由
- 桃华的定制内容用注释块标注 `; 桃华定制：`

## 技术要点

- **安装模式**：`currentUser`（不需要管理员权限）
- **安装器类型**：NSIS（配置在 tauri.conf.json 的 bundle.targets）
- **Rust 优化**：release profile 开启 LTO + strip + opt-level=s
- **前端**：纯 HTML/CSS/JS，无框架，无构建步骤

## 开发备注

- 调试时用 `cargo tauri dev`（不是 build）
- 图标文件路径：`src-tauri/icons/icon.ico`（`tauri.conf.json` 中配置，相对于 `src-tauri/` 目录）
- 应用标识符：`minagi-file-classifier`（AppData 路径、registry 目录名等用这个）
- **重要**：与旧版 Electron 1.6 的 `name` 一致，因此 AppData 共用 `%APPDATA%\minagi-file-classifier\MinagiData\`，无需迁移逻辑
