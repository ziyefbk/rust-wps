# dot 接手说明

## 目标和当前状态

这是 Windows 本地离线办公原型，Rust 核心、Tauri 桌面壳、Tiptap 编辑器。开发顺序为 **DOC/DOCX/PDF → Excel → PPT**。三阶段已有可运行的受限原型，完整 Office/WPS 兼容仍未完成；不要把文字提取或 ZIP 解析成功当作版式和编辑兼容。各格式的能力和缺口以 [ROADMAP.md](ROADMAP.md) 为准。

本次已完成 Word 显式字体名从选区编辑、光标输入、撤销、草稿，到 DOCX 导出与外部导入的闭环。外部 DOCX 按脚本区分 ascii/hAnsi/eastAsia/cs 字体，处理有限主题字体语言；引用缺失的主题字体会警告。`standard-v2` 新文档默认样式与旧 V1 模型并存。字体名输入是字面值，最多 31 个 UTF-16 单元；本机字体不可用时由系统替代。旧 DOC 字体表尚未实现。

## 首次运行与检查

本轮增加 DOCX 隐藏文字导入保护，详情与命令见 [HIDDEN_TEXT.md](HIDDEN_TEXT.md)。Rust 1.99/1.88 各 153 项核心、1,255 项 vendor、前端构建、39 项无浏览器前端/模型检查和真实语料独立对照通过。完整严格 Clippy 有既有跨格式告警；仅对该既有 lint 作命令行排除后通过。Linux Chromium 启动被 socket 权限阻止，桌面测试因缺 glib/GTK/WebKit 构建依赖受阻；164 项浏览器相关用例及 Windows 原生回归未完成，不可用历史结果替代。本轮未改 Excel/PPT 功能或 vendor。

在 Windows 安装 Rust、Node/npm、MSVC 构建工具和 WebView2。仓库根目录执行：

```powershell
npm ci
cargo test --locked --quiet
npm run build
npx playwright test
npx tauri build --no-bundle --debug
```

CLI 为 `target/debug/rust-wps.exe`，桌面程序为 `src-tauri/target/debug/rust-wps-desktop.exe`。UI 测试使用 Edge 与 `dist/` 的 Vite preview；**测试仍在运行时不要重建前端或桌面**。测试需要 `python-docx`、`openpyxl`、`python-pptx`、`pypdf` 等独立读取器；本机验证使用 Codex bundled Python。`tmp/`、`dist/`、`target/` 均不纳入 Git。

真实 DOC/DOCX 样本按 `scripts/corpus.json` 的来源和 SHA-256 下载，执行 `scripts/check-corpus.ps1` 和 `scripts/check-corpus-independent.py <输出目录>`；样本不随仓库分发。原生 PDF/字体检查需先执行 `npx tauri build --no-bundle --debug --features native-smoke`，再运行 `node scripts/native-word-font-family-smoke.mjs`；最后重建普通桌面程序。脚本需要 Windows 已安装 Courier New，证据写入忽略的 `tmp/native-word-font-family/`。

解析库 `vendor/office_oxide/` 是固定的 0.1.13 包，改动见 [vendor/README.md](../vendor/README.md)。不要用全目录 `cargo fmt` 重排其原始文件。改字体、样式或 DOC 分页时，核对两份补丁是否仍能 `git apply --reverse --check --directory=vendor/office_oxide`；DOCX 补丁可用 `python scripts/refresh-vendor-typography-patch.py` 从本地已核对哈希的 registry 原包重建。

## 优先继续的工作

1. **DOC/DOCX/PDF**：扩大有来源和授权的真实文件语料；独立比对 Word/WPS 的段落、分页、图片、表格和视觉结果。处理复杂样式、页眉页脚、域、浮动对象、主题字体动态关联，以及本机缺字回退。DOC 二进制字体表尚未解析；PDF 尚无正文编辑、注释和页面操作。原生中文输入法、系统剪贴板/对话框与打印的验收不足。
2. **Excel**：在现有 XLSX 稀疏编辑、数字格式、有限公式、工作表及 XLS 只读基础上，补筛选、字体/边框等样式、完整依赖与增量重算、更多外部文件转换；保持原文件安全和公式缓存身份。
3. **PPT**：在现有 PPTX 文本/图片/页操作/播放/PDF、PPT 旧格式只读基础上，补基本形状、混合对象层次与模板继承，再扩大旧版和复杂对象兼容。

新增格式能力时，先定义受支持模型和拒绝边界，再用真实文件与独立消费者验证导出、重开、原件不变。现有具体回归结果、限制和原生证据位置见 [REGRESSION.md](REGRESSION.md)。
