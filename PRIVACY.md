# 隐私政策 / Privacy Policy

适用于：顶部悬浮翻译（TopBar Translate）
Applies to: 顶部悬浮翻译 (TopBar Translate)

最后更新 / Last updated: 2026-11-03

---

## 中文

**简而言之：本应用不收集你的个人数据。只有你主动翻译的内容会发送给你选择的翻译服务。**

1. **翻译的文字和图片**
   当你点击翻译（或开启自动翻译后停止输入）时，应用会把你输入的文字，或你粘贴、截取的图片，发送给你在设置中选择的翻译服务。默认服务是哔哩哔哩 Index-Translate（index-translate.bilibili.com），无需账号。你也可以改用自己配置的服务（如 OpenAI、DeepSeek、Google Gemini、Anthropic Claude、DeepL、Microsoft Translator、本机 Ollama 或自定义接口）。开启「自动故障切换」或「多引擎对比」后，同一段内容也会发送给你已配置并启用的其他服务。开启「单词查词」后，单个英文单词会发送到 dictionaryapi.dev 以获取音标和释义。若所选服务不支持图片，或你在设置中选择「始终本地 OCR」，图片会先由系统自带的文字识别（Windows OCR / macOS Vision）在本机识别，图片本身不会上传，只发送识别出的文字。这些服务按各自的隐私政策处理请求。请求中不包含开发者添加的任何设备标识或账号信息。

2. **截图**
   只有在你按下截图快捷键或截图按钮后，应用才会截取屏幕。框选区域的图片仅在本机处理，只有在你确认后才会作为翻译内容发送给所选翻译服务（见第 1 条）。应用不会在后台截屏，也不会保存截图文件。

3. **麦克风与语音输入**
   只有在你按下麦克风按钮时才会录音（最长 60 秒）。语音由本机下载的 Whisper 模型离线识别为文字，**录音不会上传到任何服务器**，识别完成后即丢弃。语音模型从 Hugging Face（huggingface.co）或其镜像（hf-mirror.com）下载，下载时仅请求模型文件。

4. **本地存储**
   设置、翻译历史记录和语音模型保存在本机的应用数据目录中。你可以在设置中关闭历史记录、设定保留条数和天数，或随时清空。你填写的 API 密钥保存在 Windows 凭据管理器（macOS 钥匙串）中，只会发送给对应的翻译服务。卸载应用时，应用数据会一并删除。

5. **不收集的内容**
   - 不需要注册或登录任何账号；
   - 没有统计分析、广告或崩溃上报；
   - 开发者不会收到你的翻译内容、截图、录音或任何个人数据；
   - 不向第三方出售或共享数据（你主动选择的翻译服务除外）。

6. **联系方式**
   如有问题，请发邮件至 heqiong300624@gmail.com。

---

## English

**In short: the app does not collect personal data. Only the content you choose to translate is sent to the translation service you select.**

1. **Text and images you translate**
   When you translate (or stop typing with auto-translate on), the app sends the text you entered, or the image you pasted or captured, to the translation service selected in Settings. The default is Bilibili Index-Translate (index-translate.bilibili.com), which needs no account. You can switch to a service you configure yourself (such as OpenAI, DeepSeek, Google Gemini, Anthropic Claude, DeepL, Microsoft Translator, a local Ollama server, or a custom endpoint). With automatic failover or engine comparison turned on, the same text is also sent to the other services you configured and enabled. With word lookup turned on, a single English word is sent to dictionaryapi.dev to get its pronunciation and definitions. If the selected service can't read images, or you choose "Always local OCR" in Settings, the image is first read on your device by the system's built-in text recognition (Windows OCR / macOS Vision); the image itself is not uploaded, only the recognized text is sent. Each service handles requests under its own privacy policy. The app adds no device identifier or account information to these requests.

2. **Screenshots**
   The app captures the screen only after you press the screenshot shortcut or button. The selected region is processed on your device and is sent to your translation service only as the content you asked to translate (see point 1). The app never captures the screen in the background and does not save screenshot files.

3. **Microphone and voice input**
   Audio is recorded only while you use the microphone button (up to 60 seconds). Speech is transcribed offline by a Whisper model downloaded to your device; **recordings are never uploaded** and are discarded after transcription. Voice models are downloaded from Hugging Face (huggingface.co) or its mirror (hf-mirror.com); only the model file is requested.

4. **Local storage**
   Settings, translation history and voice models are stored in the app's local data folder. You can turn history off, limit it by number of entries or days, or clear it at any time in Settings. API keys you enter are kept in Windows Credential Manager (macOS Keychain) and are sent only to the matching service. App data is removed when you uninstall the app.

5. **What the app does not do**
   - No sign-up or account of any kind;
   - No analytics, advertising or crash reporting;
   - The developer never receives your translations, screenshots, recordings or any personal data;
   - No selling or sharing of data with third parties (other than the translation and dictionary services described above).

6. **Contact**
   Questions: heqiong300624@gmail.com
