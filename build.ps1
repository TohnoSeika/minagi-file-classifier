# ═══════════════════════════════════════════════════════
# Minagi 文件分类助手 —— 固化打包脚本
# 桃华帮 Minagi 写的哦 ✨
# 以后每次打包都跑这个脚本，结果完全一致
# ═══════════════════════════════════════════════════════

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

# ── Step 0: 读取版本号 ──
Write-Host "🌸 桃华的固化打包脚本启动啦~" -ForegroundColor Magenta
Write-Host ""

$TauriConfPath = "$ScriptDir\src-tauri\tauri.conf.json"
if (-not (Test-Path $TauriConfPath)) {
    Write-Host "❌ 找不到 tauri.conf.json，路径不对吗？" -ForegroundColor Red
    exit 1
}

$TauriConf = Get-Content $TauriConfPath -Raw | ConvertFrom-Json
$Version = $TauriConf.version
$ProductName = $TauriConf.productName
$Identifier = $TauriConf.identifier

if (-not $Version) {
    Write-Host "❌ tauri.conf.json 里没找到版本号！" -ForegroundColor Red
    exit 1
}

Write-Host "📦 产品：$ProductName" -ForegroundColor Cyan
Write-Host "🔖 版本：$Version" -ForegroundColor Cyan
Write-Host "🏷️  标识：$Identifier" -ForegroundColor Cyan
Write-Host ""

# ── Step 1: 验证关键文件 ──
Write-Host "🔍 Step 1/5: 验证关键文件..." -ForegroundColor Yellow

$RequiredFiles = @(
    "$ScriptDir\src-tauri\tauri.conf.json",
    "$ScriptDir\src-tauri\nsis\template\installer.nsi",
    "$ScriptDir\src-tauri\icons\icon.ico",
    "$ScriptDir\src\index.html"
)

$AllGood = $true
foreach ($File in $RequiredFiles) {
    if (Test-Path $File) {
        Write-Host "  ✓ $File" -ForegroundColor Green
    } else {
        Write-Host "  ✗ $File —— 缺失！" -ForegroundColor Red
        $AllGood = $false
    }
}

if (-not $AllGood) {
    Write-Host "❌ 关键文件缺失，打包中止。" -ForegroundColor Red
    exit 1
}
Write-Host ""

# ── Step 2: 清理上一次的 NSIS 输出 ──
Write-Host "🧹 Step 2/5: 清理上次 NSIS 残留..." -ForegroundColor Yellow

# 清理带 target 架构的路径（脚本指定了 --target）
$NsisOutput = "$ScriptDir\src-tauri\target\x86_64-pc-windows-msvc\release\nsis"
$BundleNsis = "$ScriptDir\src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis"
# 也清理不带 target 的旧路径（防止手动 build 的产物残留）
$OldNsisOutput = "$ScriptDir\src-tauri\target\release\nsis"
$OldBundleNsis = "$ScriptDir\src-tauri\target\release\bundle\nsis"

foreach ($Dir in @($NsisOutput, $BundleNsis, $OldNsisOutput, $OldBundleNsis)) {

foreach ($Dir in @($NsisOutput, $BundleNsis)) {
    if (Test-Path $Dir) {
        Remove-Item -Recurse -Force $Dir -ErrorAction SilentlyContinue
        Write-Host "  ✓ 已清理 $Dir" -ForegroundColor Green
    }
}
Write-Host ""

# ── Step 3: 执行 Cargo Tauri Build ──
Write-Host "🔨 Step 3/5: 开始编译打包（这需要几分钟）..." -ForegroundColor Yellow
Write-Host "  命令: cargo tauri build --target x86_64-pc-windows-msvc" -ForegroundColor DarkGray
Write-Host ""

Push-Location "$ScriptDir\src-tauri"
try {
    cargo tauri build --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) {
        throw "cargo tauri build 返回了错误码 $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
Write-Host ""

# ── Step 4: 复制到 dist/ ──
Write-Host "📋 Step 4/5: 复制安装包到 dist/ ..." -ForegroundColor Yellow

$DistDir = "$ScriptDir\dist"
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
    Write-Host "  ✓ 创建 dist/ 目录" -ForegroundColor Green
}

# Tauri NSIS 命名规则: {productName}_{version}_x64-setup.exe
$SourceInstaller = "$BundleNsis\${ProductName}_${Version}_x64-setup.exe"
$TargetName = "Minagi_文件分类助手_v${Version}.exe"
$TargetPath = "$DistDir\$TargetName"

if (-not (Test-Path $SourceInstaller)) {
    Write-Host "❌ 找不到生成的安装包：" -ForegroundColor Red
    Write-Host "   $SourceInstaller" -ForegroundColor Red
    Write-Host ""
    Write-Host "   可能原因：" -ForegroundColor DarkYellow
    Write-Host "   1. cargo tauri build 虽然没报错但没生成 NSIS 包" -ForegroundColor DarkYellow
    Write-Host "   2. 安装包被输出到其他位置了" -ForegroundColor DarkYellow
    Write-Host ""
    Write-Host "   正在搜索可能的安装包位置..." -ForegroundColor DarkGray
    $PossibleExes = Get-ChildItem -Path "$ScriptDir\src-tauri\target\x86_64-pc-windows-msvc\release" -Recurse -Filter "*setup*.exe" -ErrorAction SilentlyContinue
    if ($PossibleExes) {
        foreach ($Exe in $PossibleExes) {
            Write-Host "   找到: $($Exe.FullName)" -ForegroundColor DarkGray
        }
    }
    exit 1
}

Copy-Item -Path $SourceInstaller -Destination $TargetPath -Force
$FileSize = [math]::Round((Get-Item $TargetPath).Length / 1MB, 2)
Write-Host "  ✓ 安装包已复制" -ForegroundColor Green
Write-Host ""

# ── Step 5: 打印结果 ──
Write-Host "✨ Step 5/5: 打包完成！" -ForegroundColor Magenta
Write-Host ""
Write-Host "  📁 输出文件: $TargetPath" -ForegroundColor White
Write-Host "  📏 文件大小: $FileSize MB" -ForegroundColor White
Write-Host "  🔖 版本号:   $Version" -ForegroundColor White
Write-Host ""
Write-Host "🌸 桃华的报告：固定脚本打包完成，每次结果都一样哦~" -ForegroundColor Magenta
