# Minagi 文件分类助手 ✨

[![CI](https://github.com/TohnoSeika/minagi-file-classifier/actions/workflows/ci.yml/badge.svg)](https://github.com/TohnoSeika/minagi-file-classifier/actions/workflows/ci.yml)

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

![软件界面](screenshots/Screenshot-01.jpg)

![操作历史](screenshots/Screenshot-02.jpg)

![设置界面1](screenshots/Screenshot-03.jpg)

![设置界面2](screenshots/Screenshot-04.jpg)

![设置界面3](screenshots/Screenshot-05.jpg)

![操作说明](screenshots/Screenshot-06.jpg)

---

## 📦 下载

从 [Releases](https://github.com/TohnoSeika/minagi-file-classifier/releases) 页面下载最新版本的安装包即可。

> 💡 当前版本 **v2.1.0**，安装包约 2MB，无需管理员权限即可安装。

---

## 🔧 从源码构建

### 环境要求

- [Rust](https://www.rust-lang.org/)（推荐使用 [rustup](https://rustup.rs) 安装）
- Tauri CLI（`cargo install tauri-cli --version "^2"`）
- Windows 10/11（目前仅支持 Windows）

> 前端是纯 HTML/CSS/JS、没有构建步骤，所以不需要 Node.js。

### 构建步骤

```powershell
# 1. 克隆仓库
git clone https://github.com/TohnoSeika/minagi-file-classifier.git
cd minagi-file-classifier

# 2. 运行构建脚本（自动读取版本号，打包输出到 dist/ 目录）
.\build.ps1
```

> 也可以使用 `cargo tauri dev` 进行开发调试。

---

## 📁 项目结构

```
minagi-file-classifier/
├── .github/workflows/      # CI（编译检查 + 单元测试）
├── src/                    # 前端 (HTML/CSS/JS)
│   ├── index.html         # 主页面
│   ├── scripts/           # JS 脚本
│   └── styles/            # CSS 样式
├── src-tauri/              # Tauri 后端 (Rust)
│   ├── src/               # Rust 源码（含单元测试）
│   ├── nsis/              # NSIS 安装器定制模板
│   ├── icons/             # 应用图标
│   ├── capabilities/      # Tauri 权限配置
│   ├── Cargo.toml         # Rust 依赖
│   └── tauri.conf.json    # Tauri 配置
├── screenshots/            # 界面截图
├── build.ps1              # 固化打包脚本
└── README.md              # 本文件
```

---

## 📋 更新记录

### v2.1.1（开发中）
- 🛡️ 安装器：旧版 Electron 残留改为按文件名清单清理，不再误删共享目录里的用户文件；修复完成页快捷方式勾选与 `/NS` 静默安装参数
- 🐛 配置、历史与迁移写回改为临时文件 + rename 原子替换，配置损坏时自动回退到最新有效快照，迁移保留未知字段
- 🐛 目录遍历加入路径去重与深度上限，junction / 符号链接套娃不再无限复制；任务标志改用 RAII 守卫，异常后不会卡在「正在处理」
- 🐛 快照：恢复流程加锁、失败可回滚，背景图改用硬链接，自动清理孤儿图片
- 🐛 修复快照信息显示、透明度 IPC 节流、标题明暗取样与进度条误拖窗口
- ♿ 关闭的弹窗移出焦点顺序，开关支持键盘操作，补齐 dialog / switch 语义与 aria 标签，提升小字对比度
- 🔒 应用权限从 `core:default` 收窄到前端实际使用的 4 项
- 🧪 新增 GitHub Actions CI（编译检查 + 单元测试 + 前端语法检查）与 6 个 Rust 单元测试
- 🔧 `build.ps1` 转为 UTF-8 with BOM，目录清理失败时直接中止

### v2.1.0
- 🎨 重做设置与操作历史窗口：标题栏、卡片层次、滚动条与列布局全面整理
- 🪟 顶栏改为紧凑标题栏，窗口四边留白统一
- 🐛 修复窗口最上边缘无法拖动的问题
- 🔒 启用 CSP，背景图改用 asset 协议加载
- 📸 配置快照改为内容比对，重复配置不再重复存档
- 🧹 打包脚本增加版本一致性校验

### v2.0.2
- 🐛 修复了若干 bug

### v2.0.1
- 🐛 修复高 DPI（125%/150%）显示器缩放下窗口拖拽飞出屏幕的问题
- 🔧 优化打包脚本的输出文件命名

### v2.0.0
- 🏗️ 架构迁移：从 Electron 更换为 Tauri 框架
- 🎨 全新界面设计
- 📸 增加配置快照功能（最多 7 份自动存档）
- 🕐 增加操作历史记录功能
- 🪟 增加托盘常驻功能
- 🎵 增加操作完成提示音
- 📖 增加软件内操作说明
- 📊 重新设计文件操作实时进度条，精确显示移动进度
- ⏹️ 增加文件移动过程中取消任务按钮
- 🔧 以及大量功能和界面优化
- 🌐 正式在 GitHub 开源

### v1.6
- 增加了双击拖曳区域打开文件夹的功能

### v1.5
- 优化文件移动逻辑，添加文件移动状态显示
- 软件名称从「Minagi 文件分类器」更换为「Minagi 文件分类助手」

### v1.4
- 实现升级或重新安装后保留软件配置

### v1.3
- 支持对文件夹的复制和剪切操作
- 添加拖曳区域动画效果

### v1.2
- 升级操作模式，可选复制或剪切模式
- 更换全新的软件图标（AIGC）

### v1.1
- 升级拖曳区域和整体界面背景自定义功能

### v1.0
- 🎉 初版发布，实现基础界面和功能

---

## 🤖 AI 辅助声明

本项目的部分代码及界面设计借助 AI 辅助完成。

---

## 📜 许可证

本项目使用 **MIT 许可证**。  
详情请参见 [LICENSE](./LICENSE) 文件。

---

> 本软件为免费软件，不会收取任何费用。  
> Developed by Tohno Seika
