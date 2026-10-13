# dot 接手说明

## 目标和当前状态

这是 Windows 本地离线办公原型，Rust 核心、Tauri 桌面壳、Tiptap 编辑器。开发顺序为 **DOC/DOCX/PDF → Excel → PPT**。三阶段已有可运行的受限原型，完整 Office/WPS 兼容仍未完成；不要把文字提取或 ZIP 解析成功当作版式和编辑兼容。各格式的能力和缺口以 [ROADMAP.md](ROADMAP.md) 为准。

本轮新增受审计的单节页面尺寸/正文边距和共享打印几何，旧无属性模型保持原样。纸张范围 A4/Letter/A5 横纵向、整数 twip；页面缩小造成显式对象超限时拒绝；普通未定位自动表格按正文区明确规范化，见 [页面几何](PAGE_GEOMETRY.md)。

本轮新增正文单图段落的有效间距/对齐与有限 Latin 基线字体：图片专用段落样式修复小图行框，UI 选区控制、撤销/草稿、严格模型与可信重开贯通。缺省旧模型仍保留原字节，当时尚未覆盖源页面，后续单节子集见 PAGE_GEOMETRY.md；复杂混排仍不在承诺内，见 [单图段落](IMAGE_PARAGRAPHS.md)。

本轮新增受审计的正文行内 PNG/JPEG 显示尺寸、比例/重置、撤销/草稿、DOCX 与可信重开；媒体字节保持不变；后续单图段落扩展保留有限间距/对齐，整体排版仍未完整恢复。真实原始图片正对照、媒体方向限制、旧字节兼容与独立物理尺寸验证见 [图片尺寸](IMAGE_DIMENSIONS.md)。

本轮新增简单表格整体对齐与有限左缩进，默认/显式零、选区/混合、撤销/草稿、导出/可信重开贯通。独立消费者发现旧模式文字边缘参照与现代边框参照不同；导入按设置/边距/样式证明收窄，原始六表自动对齐有明确规范化正对照，原始七表缩进仍拒绝。详见 [表格位置](TABLE_POSITION.md)。

此前新增简单矩形表格的有限固定列宽：正整数 twip 数组、当前列数值设置、原子增删行列、撤销/草稿、固定 DOCX 读写及可信模型重开。源布局必须由同一有界快照中的明确 fixed/grid/tcW 证明，自动表格明确规范化；复杂或模糊几何拒绝。范围见 [固定列宽](TABLE_WIDTHS.md)。

本次新增段前/段后间距与倍数行距的选区/当前段落编辑、撤销、草稿及 DOCX 往返；包含列表/单元格，修复继承和自闭合空段丢失。外部 DOCX 有效缺省实体化为 0/0/240；可信自有模型与旧 DOC 路线不变。范围见 [段落间距](PARAGRAPH_SPACING.md)。此前已完成 Word 显式字体名从选区编辑、光标输入、撤销、草稿，到 DOCX 导出与外部导入的闭环。外部 DOCX 按脚本区分 ascii/hAnsi/eastAsia/cs 字体，处理有限主题字体语言；引用缺失的主题字体会警告。`standard-v2` 新文档默认样式与旧 V1 模型并存。字体名输入是字面值，最多 31 个 UTF-16 单元；本机字体不可用时由系统替代。旧 DOC 字体表尚未实现。

最近新增 [DOCX 附属内容保护](SIDE_STORIES.md)：以实际 OPC 关系及主 XML 引用阻止非标准名称的页眉页脚等内容静默丢失，IR 全六个页眉页脚字段再次检查。没有新增这些内容的编辑能力；最新本轮验证见 REGRESSION。简单固定列宽及有限表格对齐/缩进已接入；现代缩进参照、自定义单元格边距等仍规范化或拒绝，具体边界见 TABLE_POSITION。间距和固定列宽 UI/原生渲染验证仍需补齐。

## 首次运行与检查

最新页面轮：Rust 1.99/1.88 各 315 项核心、165 项无浏览器检查及构建通过；原生 PDF 的纯几何/验证/发布函数提取测试各 15 项通过，但实际 Tauri/WebView2 未运行。页面独立 18 支持、3 缺页规范化、49 拒绝、73 非法模型、4 篡改、2 未改造原件与 40 PDF；普通自动表格规范化参考单列。117 份旧无页面模型整文件字节不变，图片、表格、间距、语料与安全回归全复测。严格 Clippy 仍只有既有两工具链各一项限定排除；11 个浏览器规格仅发现。完整证据与未验证边界见 REGRESSION。

上一轮单图段落：Rust 1.99/1.88 各 287 项核心、141 项无浏览器检查及构建通过。21 个正向源（含 4 个规范化探针）、36 拒绝、88 非法模型、46 PDF 与原始图片正对照完成；14 份受控文档的 15 个图片框和后续文字基线与源差为 0.0 pt。旧图片/表格/间距/安全语料全部复测，13 份历史仅尺寸 DOCX 整文件字节未变。原始严格 Clippy 仅既有单项，限定排除后通过；9 项浏览器规格只发现未执行，详细证据和边界见 REGRESSION。

上一轮图片尺寸：Rust 1.99/1.88 各 256 项核心、123 项无浏览器前端检查及前端构建通过。独立 13 支持/51 拒绝/27 非法模型/26 LibreOffice PDF，加 1 份固定未改造上游图片正对照。旧表格位置/宽度/间距/历史字节、真实语料及内容保护全复测通过。原始严格 Clippy 仍各仅既有跨格式单项，限定排除后通过；vendor/锁文件未改。11 项新浏览器规格只发现未执行，原生限制不变，详见 REGRESSION。该轮独立测出每图后多约 12.3 pt 间距；本轮按真实声明修复并额外处理小图字体行框，最新计数见 REGRESSION，仍不能声称完整纵向布局保持。

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

1. **DOC/DOCX/PDF**：继续扩大有证据的页面/对象布局子集，当前单节纸张与边距已覆盖有限 A4/Letter/A5；扩大有来源和授权的真实文件语料；独立比对 Word/WPS 的段落、分页、图片、表格和视觉结果。处理复杂样式、页眉页脚、域、浮动对象、主题字体动态关联，以及本机缺字回退。DOC 二进制字体表尚未解析；PDF 尚无正文编辑、注释和页面操作。原生中文输入法、系统剪贴板/对话框与打印的验收不足。
2. **Excel**：在现有 XLSX 稀疏编辑、数字格式、有限公式、工作表及 XLS 只读基础上，补筛选、字体/边框等样式、完整依赖与增量重算、更多外部文件转换；保持原文件安全和公式缓存身份。
3. **PPT**：在现有 PPTX 文本/图片/页操作/播放/PDF、PPT 旧格式只读基础上，补基本形状、混合对象层次与模板继承，再扩大旧版和复杂对象兼容。

无新间距属性的 V1/V2 所有编码部件必须保持历史字节；有新属性时才增加最小 settings 部件，维持模型哈希/重编码保护。不要把 `import-docx` 当可信模型重开，使用 `read-docx-model`。

新增格式能力时，先定义受支持模型和拒绝边界，再用真实文件与独立消费者验证导出、重开、原件不变。现有具体回归结果、限制和原生证据位置见 [REGRESSION.md](REGRESSION.md)。
