# TopBar Translate

鼠标触顶即可翻译的轻量桌面翻译工具（Windows 10/11、macOS）。Rust + Tauri 2。

- 鼠标移到屏幕顶部居中（默认 200×6px，停留 200ms）自动展开翻译框，移开 600ms 自动收起；正在输入 / 打开下拉菜单时不会收起
- 📌 固定窗口；全局快捷键（默认 `Ctrl+Alt+T` / `⌘⌥T`，可自定义）唤起
- 自动识别语言：中文 → 英语，其他语言 → 中文（简体），可手动选择 / 交换
- 默认翻译源 **Bilibili Index-Translate** 免费公网 API，无需 Key
- 可添加 OpenAI、DeepSeek、Gemini、Claude、DeepL、Microsoft Translator、Ollama、自定义 OpenAI 兼容服务；API Key 存在 Windows 凭据管理器 / macOS 钥匙串
- 直接粘贴或拖入图片即可图片翻译（Index-Translate 原生视觉能力）
- Windows 托盘 / macOS 菜单栏图标；开机启动；浅色 / 深色主题

## 下载

每次推送到 `main` 都会由 GitHub Actions 构建并发布到 Releases（`build-N`）：

- Windows：`TopBar Translate_x.y.z_x64-setup.exe`（或 `.msi`）
- macOS：`TopBar Translate_x.y.z_universal.dmg`（Intel + Apple Silicon）。应用未签名，首次打开前执行：
  `xattr -cr "/Applications/TopBar Translate.app"`

## 本地开发

```bash
npm install
npm run dev      # 开发运行
npm run build    # 打包
```

## 结构

```
ui/                 前端（原生 HTML/CSS/JS，无构建步骤）
  index.html        顶部翻译面板
  settings.html     设置 / API 管理
src-tauri/src/
  panel.rs          顶部 Hover 检测、定位、自动收起
  translate.rs      各翻译平台适配器与错误归类
  lang.rs           语言列表、自动识别、目标语言规则
  config.rs         设置持久化、Base URL 校验、系统安全存储
  lib.rs            命令、托盘、快捷键、开机启动
```

## 路线图

- V1.1：截图框选翻译、翻译历史、语音输入、自动故障切换
- V1.2：多模型对比、术语库、自定义 Prompt、用量统计
