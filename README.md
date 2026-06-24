# Minagi 文件分类助手 ✨

> 把文件拖进去，它就帮你分类好了——简单、漂亮、又快。

一个 Windows 桌面工具，提供 **6 个可自定义的拖拽区域**，拖入文件或文件夹即可自动复制/剪切到目标位置。  
使用 Tauri ，更轻、更快、更优雅。

---

## ✨ 特性

- 📂 **6 区域拖拽分类** —— 每个区域可独立设置名称、目标路径、背景图
- 📋 **复制 / ✂️ 剪切模式** —— 一键切换，进度条实时显示
- 🎨 **视觉自定义** —— 主题色、背景图、界面透明度、区域独立背景
- 📸 **配置快照** —— 每次修改区域设置时自动存档，最多保留 7 份，随时可以回退
- 🕐 **操作历史** —— 最多保留 50 条操作记录，方便回溯
- 🪟 **系统托盘** —— 支持最小化/关闭到托盘，后台常驻
- 🎵 **完成提示音** —— 操作完成时发出提示
- 🚀 **性能** —— 安装包约 2MB，内存占用约 50MB，瞬间启动

---

## 📖 使用方法

> 下面是软件内置操作说明的内容～ 一看就懂哦 ✨

| 操作 | 说明 |
|:---|:---|
| 🖱️ **拖拽文件** | 把文件或文件夹拖到六个区域之一，软件自动复制或剪切到目标位置 |
| 📂 **双击区域** | 双击任意区域，打开该区域设定的目标文件夹 |
| 📌 **粘贴路径** | 复制文件夹路径，粘贴到区域下方输入框，点击确定即可设定目标 |
| ⚙️ **设置** | 点击右上角的齿轮按钮，自定义主题色、背景图、透明度等 |
| 🎨 **区域个性化** | 在设置中为每个区域单独命名、设定路径和背景图 |
| 📸 **配置快照** | 每次修改区域设置时自动存档，最多保留 7 份，随时可以恢复 |
| 🕐 **操作历史** | 点击时钟图标查看操作记录，最多保存 50 条 |
| 🪟 **系统托盘** | 关闭窗口时隐藏到托盘，后台常驻不打扰 |

---

## 🖼 截图

<!-- TODO: 添加截图 -->

---

## 📦 下载

从 [Releases](https://github.com/Tohno-Seika/minagi-file-classifier/releases) 页面下载最新版本的安装包即可。

---

## 🔧 从源码构建

### 环境要求

- [Rust](https://www.rust-lang.org/)（推荐使用 [rustup](https://rustup.rs/) 安装）
- [Node.js](https://nodejs.org/)（Tauri CLI 所需）
- Windows 10/11（目前仅支持 Windows）

### 构建步骤

```powershell
# 1. 克隆仓库
git clone https://github.com/Tohno-Seika/minagi-file-classifier.git
cd minagi-file-classifier

# 2. 运行构建脚本（自动读取版本号，打包输出到 dist/ 目录）
.\build.ps1
```

> 也可以使用 `cargo tauri dev` 进行开发调试。

---

## 📁 项目结构

```
minagi-file-classifier/
├── src/                    # 前端 (HTML/CSS/JS)
│   ├── index.html         # 主页面
│   ├── scripts/           # JS 脚本
│   └── styles/            # CSS 样式
├── src-tauri/              # Tauri 后端 (Rust)
│   ├── src/               # Rust 源码
│   ├── nsis/              # NSIS 安装器定制模板
│   ├── icons/             # 应用图标
│   ├── capabilities/      # Tauri 权限配置
│   ├── Cargo.toml         # Rust 依赖
│   └── tauri.conf.json    # Tauri 配置
├── build.ps1              # 固化打包脚本
└── README.md              # 本文件
```

---

## 📜 许可证

本项目使用 **MIT 许可证**。  
详情请参见 [LICENSE](./LICENSE) 文件。

---

> 本软件为免费软件，不会收取任何费用。  
> Developed by Tohno Seika
