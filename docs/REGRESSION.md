# 当前跨格式回归记录

## 本轮：PDF 提取器普通 unwind 回退（2026-10-13）

基于 `16eb21e93fd8f81a212f2a684f971f169314bfa8`，只在 `src/pdf_snapshot.rs` 的实际提取调用周围捕获普通 unwind；同一有界源读取 / 签名 / 大小校验仍在捕获区外。严格模式明确错误，预览模式为空正文 / 真实字节数 / panic-specific `text_error`。原有成功、空白与返回错误策略不变；没有把 worker 非零退出、超时或协议故障普遍降级。实现、资源和页面验收边界见 [PDF_PANIC_RECOVERY.md](PDF_PANIC_RECOVERY.md)。

### 最终门禁

- Rust **1.99 / 1.88 各 503 项通过**，即原493 +10新检查；工具链不重复累计。原13项源快照测试完整保留，与10新检查合计23项。日志 `tmp/pdf-panic-core-{stable,msrv}-final.log`
- 新检查使用与生产相同的私有 `UnwindSafe` callable：借用 / 拥有 / 非字符串 payload 的严格 / 预览 / JSON策略；控制及 bidi 字符转义与512字节明细上限；精确成功 / 空白 / 原返回错误；签名 / 超限 / I/O 不调用提取器；reader panic 不被吞掉；捕获后正常提取无旧状态
- **460 项无浏览器检查通过**，精确复用 `tmp/pdf-phrase-ui-command.json`，在前端构建完成后运行；日志 `tmp/pdf-panic-ui-aggregate-final.log`。没有新增实际浏览器用例或页面渲染声明
- `cargo +stable fmt --check`、独立 probe 的 rustfmt、Node checker 语法、`git diff --check`、CLI / 公共库构建、`npm run build` / TypeScript 通过。日志 `tmp/pdf-panic-{fmt,build,ui-build}-final.log`；前端仅既有 bundle 大小提示
- 原始严格 all-targets Clippy 两工具链仍各仅旧跨格式单项：1.99 XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`。分别只排除对应既有单项后通过，没有新增豁免。日志 `tmp/pdf-panic-clippy-{stable,msrv}-{raw,scoped}.log`
- 沿用 `../toolchains/{cargo,rustup}`、`CARGO_INCREMENTAL=0`、stable的 `target` 与1.88的 `tmp/msrv-target`，无RUSTFLAGS / profile改变或重复target。前端使用 `USERPROFILE=$PWD/tmp/linux-home`。没有更改依赖、锁文件、vendor、桌面或前端产品源码
- 除有意修改的 `src/pdf_snapshot.rs` 外，其他产品源码 / 依赖 / vendor / 历史PDF checker及manifest共 **279个文件**相对基线逐字节不变，记录 `tmp/pdf-panic-protected-identity.json` SHA **`c7c111640a7063005874519e72dd61a7ba6f67176b2ce7396dcc5a70872d17c6`**
- vendor测试与真实桌面没有重跑，不重复宣称旧计数。已知 Chromium EPERM / 云 localhost / Linux glib-WebKit / 离线 Windows 路线没有重试；没有新页面截图或视觉渲染结果

最终 `src/pdf_snapshot.rs` SHA-256 **`f77cbc907cc2a3a0b8fe3e53206254c913fcfbbfe34a684f948e16972d2be5f3`**。冻结候选 CLI `tmp/pdf-panic-final-cli` **`c92bdffaeeba9fa823d03ed53c141b284645ac70a20ff002c5110e2918274976`**；基线 `tmp/pdf-snapshot-final-cli` 仍为 **`aea080512d18946111e35490c223643225df1a2e423d79360f3bc37bfec54c67`**。

### 未改造原件与恢复证据

最终命令：

```text
node scripts/check-pdf-panic-recovery.mjs --output tmp/pdf-panic-recovery-final --preview-provenance tmp/pdf-panic-preview-provenance.json
```

最终报告 `tmp/pdf-panic-recovery-final/report.json` SHA-256 **`1575e9d7c54634cdea40a6cbd1f326b3f9e03d41320ad3d87bc2207ef3c03c99`**；checker **`7589778b5a9b79319a45d37d48a558b57ffa086a7187084b7c3ed9077d575ddd`**。原manifest及此前两份checker不改写；来源仍为固定Mozilla revision的一个已发表论文和五份上游fixture。

- **6份原件大小 / SHA / Git blob前后不变**。4份原本成功文件的严格JSON仅排除 `elapsed_ms` 后精确相同，原始正文逐字节相同；候选公共预览与严格结果也精确相同，不做空白或Unicode规范化
- 另2份旧编码panic单列：`mixedfonts.pdf` / `noembed-sjis.pdf` 的旧CLI均退出101、空stdout与明确编码诊断；候选严格CLI均退出1、空stdout与明确panic错误；公共预览均退出0、空正文、真实字节数和同一错误。这里的通过是失败隔离 / 预览策略正确，**不是Rust文字支持**
- 公共预览transport `scripts/pdf-preview-probe.rs` 调用真实 `rust_wps::inspect_for_preview`，没有复制产品实现。候选runner **`7ad0cd45ba1098eb104e1816d9f085f86653a3893bc032c843b3f8f4bd36af18`**，链接候选rlib **`0fa6c8b2d654e3a5cbfdc74e9e5c6ae0653f21448db82e0eb140f7c1c605584b`**。构建 / 链接命令、compiler及匹配serde_json依赖身份记录 `tmp/pdf-panic-preview-provenance.json`，SHA **`404c2812b0df06e24600b737937ef56adf3ae720a3b41996b12fa7022dcb3611`**，最终checker核对该记录及每个引用文件
- 两份原件使用真实PDF.js 5.6.205 Node legacy adapter、真实helper与 `ui/main.ts` 恢复 / 查找原函数，**4次完整恢复**均清除 `text_error` 并正确建立区分大小写的字面搜索标记。DOM仅为运输替身，不是实际点击 / IPC / 页面 / WebView2验收
- **12个不完整 / 拒绝控制**涵盖无正文失败保留空候选或先前文字、已完成原页的部分正文替换但保留错误、提前reject以及 **4个真实stopText取消**。在第一页加载中取消不接受该页正文、保留先前文字；在第二个运输页加载中取消保留已完成原页文字。均保留panic错误、隐藏停止控件，不声明完整成功；取消时已取得的页面清理也经过核对
- Poppler与pypdf6.10.0重新独立读取。`noembed-sjis.pdf` 的PDF.js正文精确为 `あいうえお`，与Poppler除页分隔外一致；`mixedfonts.pdf` 相关Latin / 数字内容可读，但页脚顺序与Poppler不同；pypdf字体警告 / 误解码原样保留，不声称完整日文 / 中文映射或全文顺序相同
- 另3份固定Apache POI DOC / DOCX原件的旧 / 新 `inspect` JSON仅排除耗时后精确相同，源与CLI散列不变。补充 `tmp/pdf-panic-doc-route-parity/report.json` SHA **`70808d4740b8b1e7b45120830149a1ae81cca8f06fe74e75533ae0ad3b3a33d4`**；只是旧路由无回归，不新增DOC排版 / 独立解码声明

首次preview probe构建误选已有Rust1.88的serde_json缓存，明确E0514失败，日志 `tmp/pdf-panic-preview-build.log` 保留；之后仅匹配stable依赖重新链接，最终构建成功，没有改产品 / 依赖来适配测试。第一轮成功checker报告 `tmp/pdf-panic-recovery-first/` 与缺少取消控制时的证据保留，但不与最终计数叠加。最终脚本对已存在输出目录和用基线冒充候选均在提取前明确拒绝，最终报告散列不变；负向日志 `tmp/pdf-panic-recovery-final-{existing-output,reject-baseline}.log`，拒绝报告 `tmp/pdf-panic-recovery-final-reject-baseline/report.json`。较早的同类负向证据也保留。

捕获仅限普通unwind，默认panic hook仍可写stderr，abort / OOM / 栈溢出 / payload析构二次panic / 挂起 / 资源膨胀不在保证内；后续 `read_current_pdf` 路径重开的会话身份仍未修复。不改变源文件、不加入OCR / 编辑 / 页面操作，也没有完成整个DOC / DOCX / PDF阶段。

## 本轮：PDF.js 重提取保留字面短语（2026-10-13）

基于 `c4d792784930fb004ee869752b2501bf4957ba63`，移除每个 TextItem 后无条件补空格的拼接规则。显式空白 / `hasEOL` 保留，无空白边界只在有限水平 LTR 连续几何被证明时直连，否则保守分隔；不改文字查找、PDF.js 映射或内容顺序。原页末 `trimEnd()`、取消 / 失败与 1000 页 / 2 Mi UTF-16 代码单元上限保留。精确容差、来源、界限和复现命令见 [PDF_TEXT_ASSEMBLY.md](PDF_TEXT_ASSEMBLY.md)。

### 最终门禁与不变范围

- **460 项无浏览器检查通过**：已发布 446 +14 个新增 PDF helper 用例，最终在构建结束后单独执行，日志 `tmp/pdf-phrase-ui-aggregate-final2.log`。实际完整参数保存于 `tmp/pdf-phrase-ui-command.json`；在原 grep 中加入整个 `pdf-text.spec.mjs`，确保新用例没有被旧四个名称筛掉
- **18 项 PDF helper 检查通过**：新提交的 `tests/pdf-text.config.mjs` 不启动服务器或浏览器；日志 `tmp/pdf-phrase-focused-final2.log`。覆盖显式空白、不同字体连续片段、舍入 / 几何反例、非法变换与尺寸、空 EOL / 空 item / 非文本标记、大小写 / 组合字符 / 断词不规范化、空页 / 失败页、取消 / cleanup，以及分隔 / 换行 / 页边界 / UTF-16 上限
- `npm run build` / TypeScript、Node 语法检查和 `git diff --check` 通过，`tmp/pdf-phrase-build-final2.log`；仅既有 bundle 大小提醒。未新增或执行实际浏览器 / WebView2 用例，也未重试已知受阻路线
- Rust、桌面、vendor、Cargo / npm 依赖与锁文件、`ui/main.ts` 以及固定语料清单共 **226 个文件**相对上述提交逐字节不变，身份记录 `tmp/pdf-phrase-core-identity.json` SHA-256 **`4e1ec4f619b4af243c70b861c5c1b1411886384f9abf64003faba9abc8665429`**。此前 Rust 1.99 / 1.88 各493项、Clippy、vendor及原生结果均为历史证据，本轮没有重跑，不计为本轮新通过；没有新 Rust target 或依赖修改

### 六份原件、真实 helper 与独立读者

`node scripts/check-pdf-phrase-search.mjs --output tmp/pdf-phrase-validation-final2` 的最终报告 SHA-256 **`6f258e85bf4fa4e5362b18d756dd435951b2f00315f9b3bf90c10352a5929eb3`**；checker **`5586d585004d9589a053a53ba6802b26f05c1fdd48373a8f64aff262d0588b8b`**，新 helper **`0ef994e71019b399cff2c3be431fe7de6428d5636b8dc777360d37e02f974f4e`**。来源仍是固定 Mozilla revision 的六份原件，大小 / SHA / Git blob 前后相同；冻结 Rust CLI 仍为 **`aea080512d18946111e35490c223643225df1a2e423d79360f3bc37bfec54c67`**。

- 已发表 14 页论文 `tracemonkey.pdf` 的四条原始字面短语，旧 helper 各 **0** 次、新 helper 各 **1** 次；本轮重读 Rust / Poppler / pypdf 各 **1** 次。完全不折叠空白或改变大小写
- 第6页倒退约47.21用户单位的 `Trace 2` / `Trace 1`，以及第2页基线跳变约45.94的 `snippet.` / `Interpret`，均仍分隔。正反例直接来自保存的真实 TextItems，不把所有 `hasEOL:false` 都当连续文字
- 连字 fixture 三个连续字体片段原先插入两个空格，修复后无额外空格；保留 PDF.js `ſt` 映射与独立原始 `ﬅ` 的差异，不声称 NFKC、字体映射相等或全文兼容。六份 PDF.js 非空白字符流另外核对不变，这只是诊断比较，不是搜索规范化
- 真实 `ui/main.ts` 的 `recoverPdfText` / `renderText` 原函数接入实际 helper / PDF.js 和 DOM 运输替身：初始 Rust 正文、连续两次重提取均保留四条搜索各一个 mark；小写不匹配仍为0，重提取不会累计空格。没有声称真实点击、DOM 渲染、IPC 或原生验收
- 六份 PDF.js helper 均完整提取；两份旧 Rust 编码 panic 仍分别为 `UniJIS-UTF16-H` / `90ms-RKSJ-H`、退出101，stderr原样保存，未冒充为 Rust 支持。pypdf部分字体警告也保留
- checker 采用安装的 PDF.js5.6.205 **Node legacy adapter**、pypdf6.10.0 与 Poppler。现代 Node adapter 已知缺少 `Uint8Array.toHex` 的路径没有重试。旧原件PNG仅重新查看，没有本轮新渲染或应用页面对照
- 已存在输出目录会 EEXIST 拒绝，检查后最终报告散列不变；记录 `tmp/pdf-phrase-existing-output-rejection-final2.log`。早期成功报告 `tmp/pdf-phrase-validation-first/` 与 `tmp/pdf-phrase-validation-final/` 保留，不混入最终计数；最终产品用单一尾部空白状态避免每片段重扫已累积正文，全量重新提取并复测，含两万连续片段和实际恢复 / 查找的重复路径

本轮不涉及源 PDF 改写、OCR、中文映射修复、阅读顺序、页内文字选区或 PDF 编辑；后续 PDF.js 路径重开的会话身份问题仍未解决。尚未完成整个 DOC / DOCX / PDF 阶段。

## 本轮：PDF 同源有界提取快照（2026-10-13）

基于 `2215210b0a4948c4037aac81352b3ee5e991293a`，PDF 提取不再在检查后重开路径。已打开 reader 最多消费 64 MiB + 1 字节；同一实际缓冲区负责大小 / 签名检查、`Inspection.bytes` 与固定 `pdf-extract 0.12.1` 内存提取。DOC / DOCX 原分支、依赖、vendor、UI 和桌面后端没有改动。策略、调查来源和未解决的后续 PDF.js 会话身份见 [PDF_SOURCE_SNAPSHOT.md](PDF_SOURCE_SNAPSHOT.md)。

### 核心与回归门禁

- Rust **1.99 / 1.88 各 493 项通过**：原 480 +13 个 PDF 内部检查，不重复累计工具链。日志 `tmp/pdf-snapshot-core-stable-final.log` / `tmp/pdf-snapshot-core-msrv-final.log`
- 新检查覆盖打开 A 后替换 / 删除路径仍读取 A、增长 / 缩小后的实际字节数、增长超限、实际缓冲区签名、精确读取上限 +1 与恰好上限、有效前缀后 I/O 错误、逐字节短读、空 / 非法签名、损坏 / 两份加密 fixture 的严格与预览策略、有效空白 PDF。大合成缓冲区只检查读取上限，不提交解析器
- **446 项无浏览器检查通过**，完整复用 `tmp/cell-colors-ui-command.json` 的命令与 `USERPROFILE=$PWD/tmp/linux-home`，日志 `tmp/pdf-snapshot-ui-aggregate-final.log`。这是既有跨格式模块 / 事务回归，无新增浏览器用例，不作物理 UI 验收
- `cargo +stable fmt --check`、Python checker 编译、`git diff --check`、`npm run build` / TypeScript 与 CLI 构建通过。构建保留既有 bundle 大小提示，日志 `tmp/pdf-snapshot-{fmt,ui-build-final,build-final}.log`
- 原始严格 all-targets Clippy 仍各仅既有跨格式单项：1.99 XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`；各仅排除对应单项后通过。日志 `tmp/pdf-snapshot-clippy-{stable,msrv}-raw.log`、`tmp/pdf-snapshot-clippy-stable-scoped.log`、`tmp/pdf-snapshot-clippy-msrv-scoped-resumed.log`。MSRV 首次 scoped 命令被执行环境中断，原未完成日志保留为 `...-msrv-scoped.log`；恢复后只重跑该未完成门禁，没有将它计为通过或重跑已完成的全量测试
- 沿用 `../toolchains/{cargo,rustup}`、`CARGO_INCREMENTAL=0`、stable 的 `target` 与 1.88 的 `tmp/msrv-target`，没有新建 target 或改变编译优化 / debug 参数。vendor 和实际桌面未重跑，不重复宣称旧 1,264 项或原生结果。已知 Chromium EPERM / 云 localhost / Linux glib-WebKit / 离线 Windows 路径没有重试

冻结候选 CLI `tmp/pdf-snapshot-final-cli` SHA-256 **`aea080512d18946111e35490c223643225df1a2e423d79360f3bc37bfec54c67`**；旧 `tmp/cell-colors-final-cli` 保持 **`ae22f9aaf5ee6f7015d521ef1d6960452969da2ee459071aab4df0bb81c3e516`**。恢复后的源码身份与独立报告记录的最终 `src/lib.rs` / `src/pdf_snapshot.rs` / Cargo 文件散列一致；后续只有文档与证据整理，没有功能修改。

### 未改造原件与独立对照

`scripts/pdf-snapshot-corpus.json` 固定 Mozilla `pdf.js` revision `288892aadaa27f03ed0cd4724edf678c6bda8e61` 的六份语料原件，每份记录大小、SHA-256、Git blob 与来源。包含一篇已发表论文 `tracemonkey.pdf` 和五份上游测试 fixture，不称为六份用户生产文档，不重新生成 / 重存 / 改造来源，也不分发其二进制。

实际复现命令：

```text
python3 scripts/check-pdf-snapshot.py --candidate tmp/pdf-snapshot-final-cli --output tmp/pdf-snapshot-validation/final-independent
```

脚本要求新输出目录；复跑须换目录，已有证据不会覆盖。结果 `tmp/pdf-snapshot-validation/final-independent/report.json` SHA-256 **`0b1fbb4df2307cf0e0488e88ef241010a3d1eec4f133e6f2a1c7ab75ca1879a0`**；脚本 **`4c8924ba3e97515161cc795613f9d490b995153cdb196101af3e3862c53d2dfa`**，manifest **`add9588f8d40deb4e1eeb09dd95aa2a3f6cd133cf2d4277c92ddb2fb9f273302`**。

- **6 份原件及两个 CLI 散列保持不变**；4 份成功原件的旧 / 新完整 JSON 仅排除 `elapsed_ms` 后精确相等，原始正文逐字节相等，未作空白 / Unicode 规范化
- pypdf **6.10.0** 与 Poppler 独立读取并验证固定页数 / 文字锚点；连字样本按原字形序列核对。论文、旋转页面及普通文本包含明确独立锚点，但不把锚点相同写成全文顺序或页面保真
- **2 份已存在的编码 panic 单列**：`mixedfonts.pdf` 的 `UniJIS-UTF16-H` 与 `noembed-sjis.pdf` 的 `90ms-RKSJ-H`，旧 / 新均退出 101、空 stdout 且相同明确编码诊断。不计为成功提取，不捕获 / 隐藏为新支持。Poppler 在这两份原件中读取对应文字，pypdf 对相关字体仍有告警 / 误解码，证据完整保留
- checker 对已存在输出目录和旧 / 新相同二进制的拒绝已另行检查；基线独立报告保留为 `tmp/pdf-snapshot-validation/baseline-independent/report.json`，不算候选验收
- 额外 **3 份固定 Apache POI DOC / DOCX** 原件的 `inspect` 路由对照：大小 / 源 SHA 与 `scripts/corpus.json` 一致，旧 / 新完整 JSON 仅排除耗时后精确相等，原件和两个 CLI 不变。补充报告 `tmp/pdf-snapshot-validation/doc-route-parity/report.json` SHA-256 **`a9db8db9db892bba61b98933e240f24ada45cacb1d65ef2e7dc7a73da14653d7`**；仅证明旧格式读取路由未回归，不新增排版或 DOC 独立解码声明

历史 scratch 符号链接调查的第三次结果把 105,779 字节 A 的大小与 32,408 字节 B 的正文配对；两原件未变。它证明检查 / 解析对象不同，**没有演示输入上限绕过**。本轮以确定性打开句柄测试验收修复；不声称原地并发写入的原子快照、解压 / CPU 安全、提取器 panic 修复、后续 PDF.js 同会话身份、字体映射、OCR 或整个阶段完成。

## 本轮：直接单元格底色与局部自动文字色（2026-10-13）

基于 `ca151b28eaaf285fae2e5d1f395c614320d9623d`，新增独立可选的逐格 `cellFillRgb` 与 `cellTextColorMode: "auto"`。底色 / 模式分别重置、严格混合状态、精确选区、原子候选 / 撤销、草稿、表格结构维护和可信 DOCX 重开贯通。局部自动色只作用于普通无显式 RGB 的文字；链接保留原颜色。未改变全局样式，不写死白字。范围和消费者 / CSS 差异见 [TABLE_CELL_COLORS.md](TABLE_CELL_COLORS.md)。

### 源审计与安全边界

同一源包快照的实际 OPC 主文档 / 样式 / 主题经独立审计。严格直接 clear + RGB 底色，附带主题时只接受实际唯一 accent1–6 的单一未转换 srgbClr 且与 fallback 相等，导出固定 RGB 并警告丢失主题联动。活动表格 / 段落 / 字符样式与缺省颜色逐层核对；主题、图案、条件、包装或复杂内容的未证明路径保留明确提示的旧规范化，未把 vendor 混合色或缺少文字 mark 当作来源证据。

审查发现列表转换可能让新颜色身份核验失败并拒绝旧可接受源，已修复为整个颜色附加原子撤回、保留旧规范化并提示。新增真实编号 / 项目符号单元格和后部身份不一致对照，防止早期格属性泄漏。活动默认颜色缓存、样式链与累计预算、逐字符流式比较限制了重复扫描与分配；未使用模板不会触发新普遍拒绝。

### 完整门禁

- Rust **1.99 / 1.88 各 480 项通过**：原 460 +16 集成 +4 审计内部检查；两个版本不重复累计。日志 `tmp/cell-colors-core-stable-final.log`、`tmp/cell-colors-core-msrv-final.log`
- **446 项无浏览器前端检查通过**：原 424 +22。覆盖候选 / 实际控件回调、光标 / 正反向 / 多格 / 行列 / 表格节点选区、混合 / 缺省、分别重置、原子历史、锁定 / composing、迟到色盘事件、结构增删与表头转换、非法元数据粘贴、预算与不可变打印声明。精确命令 `tmp/cell-colors-ui-command.json`，最终日志 `tmp/cell-colors-ui-aggregate-final.log`
- `npm run build` / TypeScript、Python / Node 语法与 `git diff --check` 通过，构建日志 `tmp/cell-colors-ui-build-final.log`；保留既有 bundle 大小提示。六个新浏览器规格仅发现，未执行（`tmp/cell-colors-browser-discovery-final.log`）；不把无 DOM 事务或 LibreOffice 视作浏览器 / WebView2 / 原生打印验收
- 原始严格 all-targets Clippy 各仍仅旧跨格式单项：1.99 XLS `chunks_exact_to_as_chunks` 与1.88 PPT `uninlined_format_args`。仅分别排除该已知单项后通过：`tmp/cell-colors-clippy-{stable,msrv}-scoped.log`。新增 type-complexity 通过类型别名修复，早期失败日志保留，没有增加豁免
- `vendor/`、锁文件及桌面后端未改，本轮未重新跑解析库 1,264 项或真实桌面；旧计数不算新验证。没有重试已确认的 Chromium EPERM、云 localhost、Linux glib / WebKit 或离线 Windows 路径

冻结 CLI `tmp/cell-colors-final-cli` SHA-256 `ae22f9aaf5ee6f7015d521ef1d6960452969da2ee459071aab4df0bb81c3e516`；前轮 `tmp/empty-font-final-cli` 保持 `ed77eea6ce7d6f26eede7ee33af3655f42e1eeebbc9060ac086bb57c6a6d9430`。沿用两份既有 target、`CARGO_INCREMENTAL=0` 和相同工具链环境，前端沿用 `USERPROFILE=tmp/linux-home`，未建立重复 Rust target。

### 独立产品与消费者证据

最终独立报告 `tmp/table-cell-colors-independent/run-hpy1jhpc/report.json`，SHA-256 `aaa8c34cc88dd88bc122988a2e5889c9f84bf2192deabc7860e0a1ee138d674d`；脚本 `scripts/check-table-cell-colors.py` SHA-256 `969c0d1b9272349a870107146f9408b171a32b73115642770f55dd40dc759f51`。结果汇总 `acceptance-summary.json` SHA-256 `d379b824fae309ee7068b45fe67f1c628d075f3e4d99bc97b3aec7e4b088b32a`。

- **43 个来源**：42 份独立 python-docx / OOXML 定向样本及 1 份未改造 POI `table-alignment.docx`；25 个来源有被证明的填色，12 个有被证明的自动文字色。其余支持有提示的原有规范化，包括活动条件 / 默认字符色、图案、主题 mismatch / 缺关系、直接 / 继承底纹高亮、重复 / 包装文字色及普通列表绑定；主题改名目标与无关旧主题单独核对
- **101 次产品导出及完整可信模型重开、144 次 Rust 原件 / 输出禁止覆盖**；43 个剥除仅新增颜色字段的模型与旧冻结 CLI 导出全包字节相等，其他全局部件不变；另一次 helper 已存在输出目录拒绝单列。源包、原件及旧 CLI 散列保持不变
- **14 个真实编辑器候选 / 历史结果**：分别基于独立实色源与真实原件导入，执行首次 / 编辑 / undo / redo / 只重置底色 / 只重置模式 / 两项重置。每份输出独立核对 OOXML、文字和显式 RGB，再完整可信重开；另有独立模型编辑与非产品生成的参考，未把产品输出当期望来源
- **32 个非法模型、4 个包篡改拒绝**：含底色 / 自动色 XML 改动后修复哈希仍与模型重编码不符；不把任意命令失败当作成功拒绝
- **41 份本轮新 LibreOfficeDev PDF /21 组比较**：每文件独立新 profile / 进程。19 组受控源→输出 / 编辑参考→输出的文字基线 x/y 最大差 **0.0 pt**；文字与实际 RGB 顺序、底色 RGB 及数量一致。两组真实原件 / 实际编辑原件的 y 最大差也为 **0.0 pt**，但 x 最大差 **145.85 pt** 明确仍为既有宽度规范化，不声称整页相等
- 最终原件与输出 PNG、八格受控联系图实际查看，位置 `final-original-{source,product}.png` 和 `final-controlled-contact-sheet.png`。六条橙色表头带及白色自动文字恢复；明确黑 / 绿文字、第一格改蓝、分别重置均可见，没有可见缺字或裁切。浏览器亮度近似可能把同一橙底显示为黑字，仍不是 DOCX 消费者像素对照


独立补充 `scripts/check-table-cell-color-geometry.py`（SHA-256 `4bea54f066e1392d3bd3af1b8d04eca749413683fa8cb8c85265aadd0e3e4866`）重新打开并校验**同一41份最终PDF**，未新渲染。21组的页码 / 文字 / RGB / 字体 / 字号完全相同，填色矩形上下边缘也完全相同；19组受控比较的四条填色边缘及文字原点**精确相等，不使用容差**。两组原件比较的填色左边缘最大差145.90pt、右边缘145.40pt、文字x145.85pt分别登记，不隐藏原自动宽度改变。补充报告 `geometry-supplement.json` SHA-256 `ac0b5840db242d21f82a24ecc249734fcdc9eebb3c8b3cbb282f8b94579cdfd7`；主报告保持原样。

早期三次失败运行均保留：直接段落 / run 底纹控制最初误以为仍保留填色，后按实际保守范围更正独立预期；helper 比较遇到 ProseMirror mark 属性的空原型与普通对象差别，改为仅规范对象原型而保留全部字段并补回归；混合色新样本的前导空格漏写 `xml:space="preserve"`，消费者正确丢弃源空格，严格比较失败。最后只修复新独立样本作者错误并从冻结产品全量重跑；没有改产品迎合样本、放宽阈值、覆盖失败文件，或把那 41 份失败批次 PDF 混入最终计数。另两份 no-op tint / shade 控制独立保留，不加入主 43 来源 /41 PDF 计数。

### 历史响应与全包字节

最终严格重放 `tmp/contextual-legacy/run-oegw_f99/report.json` SHA-256 `e3bf6e82f37fd0c23a054150bb52a8502767c95f1a6b1903905c98e0fa23bdee`：**1,520 条旧响应、302 份完整 DOCX 字节金样、604 次可信重开和604次禁止覆盖**完成。源文件、旧报告、冻结 CLI 与325份保留 PDF 身份均不变；保留 PDF 只核对散列，**没有本轮重新渲染**。

相对最近发布的空段字体 / Backspace 核心基线 `run-6fm253ps`，**1,514 条响应精确相等 /6 条有意变化，没有新拒绝**。其中4个旧隐式表头底纹导入仅增加未证明底色规范化提示；原件审计增加主题转固定 RGB 提示；原件导入增加18个直接底色与18个独立自动模式及两条对应提示。仅移除经独立原始 XML 核对的这些新属性和明确诊断后，全部完整响应再次精确相同。分类报告 `tmp/cell-colors-legacy-acceptance.json` SHA-256 `4b47f509b5de0e1a1b742f5e46a4773ef662b2d4f66bd134d8ece98d3f77fb29`。

更早上下文间距基线的原始严格报告仍如实保留 `REVIEW_REQUIRED`（1,488 精确 /32 差异），包含此前已发布的有意字体 / 几何 / 诊断变化；没有把它覆盖成“全响应逐字相同”。本轮功能源码 / 依赖84文件身份另记录于 `tmp/cell-colors-final-code-identity.json`。



## 当前：空标题 Backspace 保留段落格式（2026-10-13）

基于 `243a95b905529e299dd57c5c44867f89d50f894e`，修复已有真正空标题在首文字位置退回正文时，上游 `clearNodes` 清除七个段落字段及空行字体的问题。应用键位分支保留 `undoInputRule` 优先，复用经过完整校验的逐段类型事务，转换一次撤销；同一隔离单段单元格也原位处理，其他合段 / 列表 / 选区分支保持原行为。明确范围和剩余整文档删除重置见 [BLOCK_TYPE_CONVERSION.md](BLOCK_TYPE_CONVERSION.md)。没有修改 Rust、vendor、依赖或 node_modules。

### 最终检查与独立证据

- **424项无浏览器前端检查通过**：已发布397 +27个新完整插件 / 中间件检查；命令 `tmp/backspace-ui-command.json`，使用既有 `USERPROFILE=$PWD/tmp/linux-home`，日志 `tmp/backspace-ui-recovered-424.log`。九种不匹配场景与仅移除新增键位的原插件链逐项比较，固定表格宽度 / 行高 / 单元格对齐和默认自动尾段另外明确断言
- `npm run build` / TypeScript、Python / Node语法和 `git diff --check` 通过，`tmp/backspace-build-final.log`。**11个相关浏览器规格仅发现，其中2个为本轮新增，未执行**，`tmp/backspace-browser-discovery.log`；无物理按键、浏览器行几何、原生IME / 剪贴板 / 应用打印验收
- Rust1.99 /1.88最终各460项通过，日志 `tmp/backspace-core-{stable,msrv}-cached.log`。沿用既有两份 target 和 `CARGO_INCREMENTAL=0`，没有新建 target。221个核心 / 桌面 / vendor / 依赖文件与已发布基线逐字节相同，身份 `tmp/backspace-core-identity.json`；vendor1264项、桌面及Clippy未重跑，上轮结果仅为历史证据
- 首次聚合检查因漏设既有测试用 `USERPROFILE` 有5项跨格式独立Python调用失败；第二次遭遇磁盘空间不足。首次核心命令漏设缓存所用 `CARGO_INCREMENTAL=0` 触发不同工件重编译并填满磁盘；只删除该次命令时间范围内产生的可重建 target 工件，恢复经散列核对的工作CLI，再用准确缓存配置全部重跑。失败日志 `tmp/backspace-ui-final.log`、`tmp/backspace-ui-final-424.log`、`tmp/backspace-core-{stable,msrv}.log` 保留，不计为通过；原件、旧报告及冻结CLI未清理

独立最终报告 `tmp/heading-backspace-independent/run-_5ig67sk/report.json` SHA-256 `1bf547aca4adf365b9adb947c7437b7067150f8e0110b81aff681522598c459d`。冻结CLI `tmp/empty-font-final-cli` 保持 `ed77eea6ce7d6f26eede7ee33af3655f42e1eeebbc9060ac086bb57c6a6d9430`。

- **3个独立创作空标题源 +1份固定未改造上游原件，另3份独立目标正文参考**。原件 `NoHeadFoot.docx` revision / SHA / Git blob和准备编辑完整登记：仅工作模型删除首段以暴露原有第二空段，再经注册快捷键转标题；不冒充原文件已有该空标题。源字节、其余文字 / marks / 分页不变
- **8份实际导出DOCX和完整可信重开、16次CLI禁止覆盖 +4次驱动禁止覆盖**。XML / python-docx独立核对七字段、真实零内容、完整段落标记字体、正文样式变化及所有未编辑相邻内容；预期模型只允许记录的首段准备编辑与类型 / level变化，不只比较属性计数
- **6份新LibreOffice PDF**，每文件新进程 / 独立profile；三个独立正文参考与应用结果的 Latin 后续文字基线最大差均 **0.0pt**（原0.3pt容差）。只证明该受控空段后的纵向基线，不声称原件整页、浏览器 / WebView打印、Word / WPS或字体可用性保真
- 早期独立驱动因Node严格对象原型比较（ProseMirror空原型 attrs与普通对象）失败，字段内容并未变化；修正验证器后整轮重新执行。失败报告 `tmp/heading-backspace-independent/run-sqggv2k4/report.json` 保留，不计入最终结果

## 上一轮：真正空段落的有限基线字体（2026-10-13）

基于 `d4480270fdfd9fdb38817458b095ff1139b2ed1b`，新增仅用于零内容 paragraph / heading 的可选 `emptyParagraphFont`，保存字面字体、整数半点字号和粗斜体。正文顶层源按实际 OPC 默认 / 活动样式 / 直接段落标记解析，完整块身份核对后才附加。空行状态 / 重置、标题转换、撤销 / 草稿、可信重开与共享打印声明贯通；首次插入内容原子结束空行覆盖，普通文字继续使用当前 marks / profile。源表格 / 列表字体继承、多脚本及复杂度量没有扩大承诺；详细契约见 [EMPTY_PARAGRAPH_FONTS.md](EMPTY_PARAGRAPH_FONTS.md)。

### 声明隔离与编辑保护

先独立比较默认字体、段落样式、直接 `pPr/rPr` 和空 run。24份输入逐一使用新进程 / 独立 profile 重跑，全部复现：DejaVu Sans12pt 真正空段可由直接段落标记得到28pt锚点距离；只有空 run 的相同字体仍是26.65pt。样式12pt / 标记24pt与样式24pt / 标记6pt分别得到41.95 /21pt，Liberation Serif12pt为27.8pt。因此直接写段落标记即可，不复用单图的专用样式方案，不改全局 Normal，不加入伪空格或空文本 run。隔离证据 `tmp/empty-font-isolation/isolated-report.json` 与最终48份产品输出PDF分别统计。

源必须是真正零内容或无属性 / 子元素 / 文字的 bare `w:r`；空文本 run、带格式空 run、空格、tab、换行、分页与图片均不冒充空段。活动路径 / 命名空间 / 重复声明、默认容器、主题槽、style toggle / 直接 false、方向 / 网格 / 基线度量、继承上限、缓存与累计预算均检查。完整身份或解析失败不附加部分结果；身份已证明而个别段字体信息不足时，可保留其他已证明段，并同时提示未证明项。原有导入阻断不放宽。

完整应用插件链曾复现一次性输入 `# ` 时，标题转换被严格原始输入检查拒绝；预先准备规则状态与空行覆盖规范化的次序需要协调，未观察到旧事务提交或字体复活。现由有限首次输入适配器先正常提交并校验输入，再按新状态执行原注册规则；失败不重复插入。注册 text-input / compositionend / paste 钩子、undoInputRule 与两种尾段插件顺序均覆盖。原始剪贴板 slice 在 fitting 前校验，避免错误元数据被适配时丢弃。分段 / 合段不传播覆盖，删回空段不复活；空标题转换、合法空段粘贴和祖先结构变化可保留原节点。

### 最终检查

- **Rust1.99 /1.88各460项通过**：原435 +23项新集成 +2项解析缓存 / 预算检查；两个工具链不重复累计。日志 `tmp/empty-font-core-{stable,msrv}-gate.log`
- **397项无浏览器前端检查通过**：原346 +51；完整命令 `tmp/empty-font-ui-command.json`，日志 `tmp/empty-font-ui-final-397.log`。新增覆盖原子首次输入 / 失败候选、split / join / delete、标题 / 输入规则、undo / redo、草稿 / HTML元数据、不可变打印与重置，以及36种表格增删行列 / 首中末边界 / 自动或固定宽度组合和实际项目符号 / 编号列表包裹与解除
- `npm run build` / TypeScript通过，`tmp/empty-font-build-ui-final-397.log`，只有既有大bundle提示；Python语法及 `git diff --check` 通过。**7个浏览器规格仅发现未执行**，`tmp/empty-font-browser-discovery.log`
- 原始严格 all-targets Clippy各仍只有旧限定单项：1.99 XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`；各只排除本工具链对应单项后通过。最终日志 `tmp/empty-font-clippy-{stable,msrv}-gate-{raw,scoped}.log`。新增 lint 已修复，早期失败日志保留
- `vendor/`、依赖 / 锁文件和桌面后端未修改；1264项解析库是历史结果，本轮未重跑。未重试 Chromium socket EPERM、云浏览器 localhost、Linux glib / WebKit 或 Windows离线受阻路线。插件 / DOM声明检查并非实际按键、原生IME / 剪贴板或应用打印验收

冻结CLI `tmp/empty-font-final-cli` SHA-256 `ed77eea6ce7d6f26eede7ee33af3655f42e1eeebbc9060ac086bb57c6a6d9430`；工作CLI散列相同。旧 `tmp/table-vertical-final2-cli` 保持 `0c67ef3110ae794d371123313fe01ccfa53bef452f2316dac8a5e11c999da67e`。功能 / 测试 / 依赖身份见 `tmp/empty-font-final-code-identity.json`。

### 独立原件、DOCX与48份新PDF

最终报告 `tmp/empty-font-independent/run-d5wgo6tc/report.json` SHA-256 `fc0ff5933f7cc477d76894db2a725dc825a54a02ea6759906c1de0b51792e240`；同目录 `acceptance-summary.json` 为 `c76d6fa5b47dc5089d3a9932de8fde4a096c723af7ffcc21d8577bceae0b21da`。独立脚本 `scripts/check-empty-paragraph-fonts.py` 为 `2f4d513fee6051a7eda840a3c380a0a8136422b11fcf34c20c4477583fa40e38`，报告旁保留相同脚本快照。

- **21源 /29个已证明空段**，含2份固定revision / SHA / Git blob的未改造 Apache POI原件与原上下文间距受控反例；**16个边界 / 提示用例、54个非法模型、8个篡改拒绝、5个自有模型边界和2个既有源阻断**分别登记
- **68次导出及完整可信重开、89次禁止覆盖**；独立python-docx / XML核对实际段落标记和文字，未加入伪内容，除文档局部段落标记及嵌入模型外没有改变全局包部件。21个移除新属性的旧模型与上轮CLI **整DOCX字节相等**；原件和已有输出散列不变
- **48份新LibreOffice PDF**，每份新进程 / 独立profile；25组受控基线比较最大差 **0.0pt**，没有调整坐标或放宽0.3pt容差。9份源 / 新 / 旧预览保留，两个原件及受控对照的预览实际查看；新PDF不是应用打印或Word/WPS验收
- 原始 `table-alignment.docx` 六处Aptos12pt空段恢复：五处表间距离从旧23.1pt到源27.4pt，六表起点差由 `[0,-4.3,-8.6,-12.9,-17.2,-21.5]pt` 变为全0。此处上一轮341twip最小行高保持不变；**自动列宽、水平位置、橙色表头 / 白字仍规范化**，不声称整页相同，消费者文字实际使用NotoSerif替代
- 原始 `NoHeadFoot.docx` 四处Calibri11pt空段恢复，登记的所有文字纵坐标现与源相同；旧页末基线提前3.79999pt。分页段不作空段目标，消费者以Carlito / Caladea替代。该原件比较是明确登记的纵向观察，不冒充受控全页字体保真
- 原 `empty-neighbor-explicit-reference.docx` 的bare空run保持真实零内容，旧1.35003pt后续基线漂移恢复为0；该文件是此前独立创作对照，**不称为上游生产文件**。字体可用性 / 替代、多脚本、源列表 / 表格继承等限制继续有效

### 历史字节、安全与完整响应复核

最终原始重放 `tmp/contextual-legacy/run-6fm253ps/report.json` SHA-256 `da37cf466b6aa6de85def856bb780eb53c6c2f7b3c614c9ad538e62040f3f919`：**1520条旧响应、302份旧模型整包字节金样、604次可信重开与604次不覆盖**，源 / 旧报告 / 冻结CLI / 保留PDF均未改变。较更早的上下文间距前基线为1492完全相等 /28差异，原报告原样保留为 REVIEW_REQUIRED。

对最近已发布表格轮 `run-leg3cwb3/report.json` 逐条复核：**1500/1520完整响应精确相等，20条有意变化，仅32处独立源核对的空段字体属性和26条诊断，没有旧支持源被新增拒绝**。仅移除这批已核实新增字段 / 提示后，完整JSON、退出码和stderr均与发布版相同。复核文件 `tmp/empty-font-legacy-final-acceptance.json` SHA-256 `964e6c013eee251ecf56a56809a96239bebbc4bb3e4ac92ca05a70c995070882`；复核脚本 `tmp/empty-font-review-legacy.py` SHA-256 `a5615f2732b56791a7a786e12f577a9752402e90e7231cc32b2c24be44d6b507`。初次和最终冻结CLI的1520条响应全部一致；不把有意扩展写成所有历史响应逐字相同。

302份旧导出全部保持整文件字节；保留的325份旧PDF这里只核对身份，**不是本轮重新渲染**。没有修改旧基线、原件或既有报告，也没有将早期受控 / 共享profile结果累计到最终48份。

## 上一轮：简单表格最小行高与垂直对齐（2026-10-13）

基于 `fcab97018e20c3ea1412e49988655cda19cf5ac8`，新增可选逐行 `rowMinHeightTwips` 与逐格 `cellVerticalAlign`，包含默认 / 显式零 / top、混合状态、当前行格 / 选区、分别重置、撤销、草稿、可信重开和不可变打印快照。最小高度受当前页面正文高度约束，内容仍可自然增高；精确高度与源重复表头 / cantSplit 不进入本轮源子集。范围、真实源身份及编辑器历史 8px 上下内边距与 DOCX 零边距的差别见 [TABLE_VERTICAL_LAYOUT.md](TABLE_VERTICAL_LAYOUT.md)。

### 审计与应用边界

没有全局忽略 vAlign：页面审计仅将严格直接的正文表格单元格路径交给新表格审计，节级、错位和包装声明仍保护。源 hRule 只能省略 / atLeast；原始命名空间 / 属性 / 值 / 单一声明、活动表格样式与 basedOn 链、逐格段落文字 / 空段 / 顺序，以及转换 IR 的数值 / 规则 / 分页标志全部核对后附加属性。未知枚举不会由 vendor 的静默缺省变成支持。

样式索引、默认引用与链缓存，64 层继承上限与累计工作预算有检查；未使用模板中的声明不触发新的全局拒绝。新路径只接受省略或完整物理 dxa 零值的上下单元格边距，包括活动基本 / 条件 / 逐格层；非零、缺单位 / 数值、重复容器、包装和模糊声明保守拒绝。没有新属性的旧路径继续其既有规范化提示，旧 V1/V2 编码不变。

### 最终检查

- **Rust 1.99 / 1.88 各435项通过**：原415 +16项新集成 +4项新身份 / 缓存 / 继承边界单元检查，两个版本不重复累计。最终门禁日志 `tmp/table-vertical-core-gate-{1.99,1.88}.log`
- **346项无浏览器前端检查通过**：原320 +26；精确命令 `tmp/vertical-ui-final-command.json`，最终门禁日志 `tmp/table-vertical-ui-gate.log`。覆盖行格 / 正反向 / 多选 / 整表、默认 / 混合、显式0/top、独立重置、原子历史、过期选区、锁定、结构增删与表头切换、页面缩小、预算和非法粘贴；新行格使用默认，保留存活节点的原属性
- `npm run build` / TypeScript、Python语法及 `git diff --check` 通过，最终门禁构建日志 `tmp/table-vertical-build-gate.log`。**3个实际浏览器规格仅发现，未执行**，`tmp/vertical-browser-discovery.log`
- 原始严格 all-targets Clippy 仍各只有已知跨格式单项：1.99 XLS `chunks_exact_to_as_chunks` 与1.88 PPT `uninlined_format_args`；各只排除本工具链对应旧单项后通过。`tmp/table-vertical-clippy-1.99-raw-final.log`、`tmp/table-vertical-clippy-1.88-raw.log` 与各自 `-scoped.log`。本轮新增 collapsible-if 已修复，早期失败日志保留；没有扩大豁免
- `vendor/`、锁文件与桌面后端没有修改，本轮未重新运行解析库1264项或真实桌面；这些计数仍是历史证据。未重试已确认的 Chromium socket EPERM、云浏览器 localhost、Linux glib / WebKit 或 Windows 离线受阻路径

冻结最终 CLI `tmp/table-vertical-final2-cli` SHA-256 `0c67ef3110ae794d371123313fe01ccfa53bef452f2316dac8a5e11c999da67e`；功能 / 依赖271文件身份在 `tmp/table-vertical-final-code-identity.json`。旧冻结 CLI `tmp/contextual-final2-cli` 保持 `a6179a5e683c7c2b0ddbf93d55afad3fac67b6549f442ee093b8ea035f21e80f`。原始 POI 文件与这两个 CLI 在验证前后均未改变。

### 独立源、实际编辑与60份新PDF

最终报告 `tmp/table-vertical-independent/run-mit7pa7a/report.json`，SHA-256 `d2812fccc54a4f6b8a634c8debf92eed7e27e486b488a1c8d45d9971c2c5ac29`；脚本 `scripts/check-table-vertical-layout.py` SHA-256 `30a7493522301ad5b9c97fe5d1ed5c52f9b6ca33ee12b0c87456e7a13e67b9f3`。

- **15个支持源**：1份固定 revision / SHA / Git blob 的未改造 Apache POI `TestTableCellAlign.docx` 与14份独立 python-docx 定向样本。省略 / atLeast、显式零、单项默认、不同对齐、长内容、显式 / 逐格 / 活动样式 / 条件零边距和未用样式对照分开登记
- **53个精确源拒绝、17个非法模型、5个包篡改拒绝**。包括修复 document.xml 哈希后仍与模型重编码不符的高度 / 对齐篡改，以及额外ZIP部件；不把任意进程失败当作预期拒绝
- **45次DOCX导出 / 完整可信模型重开、90次源 / 输出不覆盖检查**，独立 python-docx + 原始 XML 检查实际属性、段落与全部文字。另15次全部垂直属性重置后输出与旧CLI的**整文件字节金样**相同
- **60份新LibreOfficeDev PDF**：15源、15导出、15实际编辑输出、15另行创作的编辑参考；每个文件使用独立新profile转换。**30组源→输出 / 编辑参考→编辑输出比较**的边框相对基线最大差 **0.0pt**，绝对纵向边框与文字基线最大差也为 **0.0pt**；严格容差小于0.3pt，没有扩大阈值或修正输出坐标
- 原件的两行边框中心距实测67.5 /58.5pt；长内容9条文字行完整可见且超过设置的最小行高自然增高。原件导出、实际编辑原件、长内容和条件零边距输出预览均实际查看；这不是字体替代、自动分页或所有源表格的完整保真承诺
- 原件自动网格9350twip仍按旧规则规范化；三条竖边框x位移 `[-0.5, -9.20001, -18.0]pt` 明确单列，**不能声称整页 / 横向相等**。Calibri 在该消费者替代为Carlito11，边框外观等仍有既有规范化限制

前一次共享profile批量转换在 `tmp/table-vertical-independent/run-lc8e6eq6/` 保留为失败证据：一份输出PDF横向边框异常为0，但相同批次两份**DOCX字节完全相同**的输入正常；原失败DOCX用新的单文件profile即恢复正常（同目录 `rerender-one/`）。最终脚本改为每文件独立profile并从冻结产品重跑全60份，没有挑选旧PDF拼成通过，也没有更改产品去迎合消费者异常。中间结构通过和旧36PDF结果不混入最终计数。

### 旧路径与源安全重放

历史重放报告 `tmp/contextual-legacy/run-leg3cwb3/report.json`（SHA-256 `5317d7c8f628ff3ad2a8bf7d1e5c3e69d0c76b5dedccf0e43cacd883c8a11f63`）完成 **1520条旧响应、302份旧模型整包字节金样、604次可信重开和604次禁止覆盖**。源包、之前报告、冻结CLI和保留PDF散列全部不变。相对于更早的上下文间距前基线，1509条响应完全相同 /11条需复核，其中10条就是前轮已审查的间距支持扩展或诊断变化；严格重放报告原样保留为 REVIEW_REQUIRED，没有覆盖成全相同。

与最近已发布的 `tmp/contextual-legacy/run-h1sni1hf/report.json` 完整结果逐条比较，**1519/1520精确相等、没有旧支持源被新增拒绝**。唯一变化是原 `table-alignment.docx` 的6张表共18行现在保留原先丢弃的 `rowMinHeightTwips:341`；除此之外完整返回JSON相同。这项有意支持扩展已由独立XML / python-docx核对：18行确实为341twip、hRule省略，移除仅这18个新属性后完整响应精确相同；接受记录在 `tmp/table-vertical-legacy-acceptance.json`，原严格报告保留，不会把它误归为全部旧响应逐字相同。

302份旧模型导出全部保持整文件字节，保留的325份旧PDF这里只复核身份，**没有本轮重新渲染它们**。补充原件 `table-alignment.docx` 的独立报告 `tmp/table-vertical-independent/table-alignment-supplement/report.json`（SHA-256 `49b23720cb12eb98829b9fe26651c6835773d7a2068a4f1afc653d3fc52c7cc5`）另有3次导出 / 可信重开、6次不覆盖及3份新单文件profile PDF；旧模型用新CLI仍与旧CLI整包字节相同。每张表的源 / 新输出边框间距均为 `[17.6,17.5,17.6]pt`，旧输出为 `[16.9,16.8,16.9]pt`，这次修复每行0.7pt。原有空段字体 / 行框、文字颜色及自动宽度规范化仍在：表间空段距离源27.4pt、输出23.1pt，六表起点差累计为 `[0,-4.3,-8.6,-12.9,-17.2,-21.5]pt`；不能宣称这个组合文件整体纵向保真。本轮主套件60份新PDF与这3份补充、新旧消费者证据分别统计。原件、已有输出、此前报告与冻结CLI均保留。

## 上一轮：快捷键与 Markdown 标题输入保格式（2026-10-13）

基于 `890cc3ec9ccd66c020263207f52af1e922cba8e0`，把应用注册的 Ctrl+Alt+0–3 和段首 `# ` / `## ` / `### ` 输入规则接入已验证的逐段候选事务。保留对齐、段前 / 段后 / 行距、左右 / 首行缩进七字段、内容 / marks、选区和显式待输入格式；同级标题快捷键切回正文，混合级别设为请求级别。列表首段标题请求原子拒绝且不解除列表，标记不提前删除。范围见 [BLOCK_TYPE_CONVERSION.md](BLOCK_TYPE_CONVERSION.md)。

### 真正插件链发现与修复

最初直接处理器通过后，完整扩展插件链揭示了额外问题：最终正文转标题时默认 trailingNode 自动补尾段，这个文档变化会清除 Tiptap 的输入规则撤回元数据。现在按真实根事务身份合并本次转换附加的步骤，恢复输入规则元数据和待输入格式，并隔开后续输入历史；Backspace / undoInputRule 能同时撤回类型 / 标记转换和本次新增尾段，原来已有尾段保留。普通输入、选区变更和不同根事务结束该窗口，不复活过期规则，也没有禁用默认尾段插件。

37 项新检查使用实际完整扩展管理器的 **86 个插件**，包括 **49 个 keymap / 8 个 input-rule**；通过普通事件对象调用 keymap、实际 handleTextInput 与 compositionend 钩子 / 计时回调。覆盖正反向 / next-text 排除 / 全选 / 节点 / 单元格、marks / 明确空 marks、原子列表与非法候选、前后独立历史、普通粘贴、代码标记排除、只读 / 锁定 / composing、同级删除、两种尾段插件顺序、预先存在尾段、后续输入失效等。现有缩进段落 Tab 保护保持：触发文字自身被拒绝时，不删除哈希或绕过制表位检查。

这些是无 DOM 的真实注册回调 / 插件调度，不是物理按键、系统剪贴板、中文 IME、真实浏览器或原生 WebView 验收。先前 8 项格式丢失复现保留为**未包装依赖的对照**，已不代表本应用当前入口；原始程序命令仍未全部改造。

### 最终代码与检查

- **320 项无浏览器前端检查通过**：原 283 +37 新检查；`tmp/block-interactions-ui-final.log`，完整命令 `tmp/block-interactions-ui-final-command.json`
- `npm run build` / TypeScript 通过，`tmp/block-interactions-build-final.log`；仍有既有大 bundle 提示。Python / Node 语法与 `git diff --check` 通过
- **9 个新增浏览器规格仅发现，未执行**：`tmp/block-interactions-browser-discovery.log`，不重试既已确认的 Chromium socket EPERM、云浏览器 / localhost、Linux glib / WebKit 或 Windows 离线阻碍
- **Rust 1.99 / 1.88 各 415 项本轮重新通过**，两个既有 target 缓存各 0.06 秒编译检查，没有重建或新 target；`tmp/block-interactions-core-1.99.log`、`tmp/block-interactions-core-1.88.log`
- **219 个 Rust / 后端 / vendor / 依赖文件与已发布基线完全相同**，`tmp/block-interactions-core-identity.json`。冻结 CLI `tmp/contextual-final2-cli` 和工作 CLI 的 SHA-256 仍同为 `a6179a5e683c7c2b0ddbf93d55afad3fac67b6549f442ee093b8ea035f21e80f`；未修改导出器、模型 schema、node_modules 或锁文件。本轮未重跑 vendor / 桌面 / Clippy，旧结果不算新验证

### 独立 DOCX 与新消费者证据

最终报告 `tmp/word-block-interactions-final/run-epy2g2fp/report.json`，SHA-256 `d26df18935e7aba1d1902b0b6376069de46d4b4f0b8919df2f455ed82a5367b6`。新增独立脚本 `scripts/check-word-block-interactions.py` 与真实插件驱动 `scripts/convert-word-block-interactions.mjs` 复用前轮固定且未改动的 **6 份源包：5 份独立创作 python-docx 对照 +1 份 Apache POI SampleDoc 原件**。没有把重复使用的源算作新原件，也未提交原二进制。

- **28 个成功导出 / 完整模型可信重开**：15 个快捷键结果、10 个输入规则文档、3 个同级重复切换结果。输入规则文档共 **57 次真实逐段处理**，每次检查 undoInputRule 恢复字面标记 / 空格及撤回这次恢复后的精确模型
- **6 次精确列表拒绝**：3 个整选区快捷键请求原子不变、3 个列表首段输入规则拒绝。输入驱动按段操作，前面的独立成功转换不是整文档批处理；拒绝后不发布输出，原输入 JSON 不变
- python-docx / OOXML 独立核对全部源段落七字段、字符 marks / 文字和标题样式；完整可信重开直接比较全部 JSON。仅独立源模型对照允许已声明的 schema 缺省元数据 / marks 顺序，以及明确登记的单个自动尾段；不静默删尾段来声称相等
- **28 次 Node helper +56 次 Rust 原件 / 输出不覆盖检查**，报告登记 **152 条主命令**，28 次 helper 禁止覆盖重试由对应记录单列；输入、源包、输出、参考、冻结 CLI、固定旧报告和相关最终源码散列不变
- **8 份本轮新 LibreOffice PDF /4 个独立目标参考对子**：文字页码、x / 行末 x / y 坐标最大差 **0.0 pt**；8 份均为单页，4 对完整页像素完全一致。联系图实际查看：同目录 `visual-evidence/shortcut-input-contact-sheet.png`，详情 `visual-report.json`。受控 Latin 字体 / 字号 / marks 匹配以隔离段落数值，不承诺原正文和标题缺省外观相同、任意字体、自动分页、Word/WPS 或应用打印保真

SampleDoc 已知主题 tint 导入颜色规范化仍单列，转换保持导入 RGB；未改 Rust 来迎合源 XML fallback。早期 `tmp/word-block-interactions-initial/run-e683462d/report.json` 在产品仍修改时正确因身份变化失败，保留为失败证据、不计最终通过；最终完整独立目录从冻结代码重跑。

后续在可用的真实浏览器 / 原生环境补验这些实际交互，并继续有证据的 DOC/DOCX/PDF 布局子集与真实源覆盖；完整文字复制、动态命名样式、复杂源布局、Excel / PPT 仍未完成。

## 上一轮：工具栏逐段保格式的正文 / 标题转换（2026-10-13）

基于 `5c03a97bac1155d5ff2db1ff83049dd13d7ce9a1`，修复工具栏跨段正文 / 标题切换将各段显式格式清空的问题。`ui/word-block-type.ts` 独立构造候选事务，逐段保留兼容属性、内容 / marks、精确选区与待输入格式，只改类型 / 级别；全部校验成功才提交，一次撤销，后续输入另分组。列表首段转标题整次拒绝，不解散列表；无操作不新增历史。范围见 [BLOCK_TYPE_CONVERSION.md](BLOCK_TYPE_CONVERSION.md)。

### 最终源码与检查

- **283 项无浏览器前端检查通过**：先前 242 +32 个转换模型测试 +1 个上下文固定值应用正例 +8 个剩余上游入口特征测试。`tmp/block-type-ui-final2.log`、完整命令 `tmp/block-type-ui-final-command.json`。最后 8 项明确断言尚未修复的依赖行为，不代表所有类型切换入口已修复
- 正反向 / 下一段文本起点 / 空段 / 整段 / 全选 / 多单元格选区、每段不同的对齐 / 间距 / 缩进、默认 / 显式零、Unicode / 硬换行 / 链接 / 字符格式 / code 与显式字体共存、stored marks、无操作、原子失败和前后独立历史均覆盖。超结构 / 深度 / 2 MiB 正文 / 32 MiB 草稿预算及共享事务不污染检查通过
- `npm run build`、TypeScript、Python / Node 语法和 `git diff --check` 通过，最终构建日志 `tmp/block-type-build-final.log`；仍有既有大 bundle 提示
- **3 个新增真实浏览器规格仅发现，未执行**：`tmp/block-type-browser-final-discovery.log`。覆盖工具栏草稿 / 撤销、尚未送达的 DOM 选区和混合列表失败。原 Chromium EPERM、云浏览器访问工作区、Linux glib / WebKit 与 Windows 离线限制未重试或绕过；不把无 DOM 事务或 LibreOffice 渲染当作实际浏览器 / WebView / 原生 PDF 验收
- **Rust 1.99 / 1.88 各 415 项本轮重新通过**：沿用既有各自 target 缓存，编译阶段分别 0.10 / 0.12 秒，无重建或新 target；`tmp/block-type-core-1.99.log`、`tmp/block-type-core-1.88.log`
- **219 个 Rust 核心 / 桌面后端 / vendor / 依赖文件与已发布基线逐字节一致**，`tmp/block-type-core-identity.json`。没有更改 Rust 模型、导出器、vendor 或 node_modules；本轮未重跑 vendor、桌面后端及 Clippy，前轮 1,264 个解析库测试和原始严格 Clippy 的既有限定诊断仍是历史记录，不冒充新跑结果

冻结 CLI `tmp/contextual-final2-cli` 与 `target/debug/rust-wps` SHA-256 均为 `a6179a5e683c7c2b0ddbf93d55afad3fac67b6549f442ee093b8ea035f21e80f`。最终 helper 为 `70a3d25449eea1c92b3867d332d88a4905f035f46d5f4395d478520a15e15b5f`；转换验证 driver / harness 和完整源码身份另冻结于 `tmp/block-type-final-identity.json`。非功能文档也一起纳入最终审查树。

### 独立真实文件与新渲染

最终报告 `tmp/word-block-type/run-xmx2wu5y/report.json`，SHA-256 `8acc4da17a13a411d68bcc34b8ab5e244968a5304d9d0e6a76c0f5dbe5e44196`。脚本 `scripts/check-word-block-type.py` 与数值 / 文本预期 `tests/fixtures/word-block-type-expected.json` 分开，**6 份源包：5 份新独立 python-docx 对照 +1 份未改造、固定 revision / SHA / Git blob 的 Apache POI SampleDoc 原件**。没有提交原始二进制，没有将自制探针称为用户生产文件。

- **24 次成功转换 / DOCX 导出 / 完整模型可信重开**，含 0–3 全类型、标题→正文链、多段 / 表格 / 空段、默认样式和显式零 / 241 行距单位。真实 Node driver 调用与工具栏相同的事务，核对修改段落数量、全选保持、一次撤销及完整 undo / redo
- **3 次列表首段标题请求原子拒绝**：输出文件不存在、输入 JSON 未变、精确列表诊断匹配；不将任意错误计为通过
- python-docx / OOXML 独立核对源期望、实际输出每段数值 / 标题样式、全部文字和字符 marks。源模型比较仅允许已声明的 Tiptap 缺省元数据与 marks 顺序；可信模型重开直接核对完整 JSON，不做这种归一化
- **48 次 Rust 原件 / 输出不覆盖检查 +9 次 Node driver 输出不覆盖检查**；所有源 / 输出散列不变。报告登记 129 条 CLI / Node 命令，9 次 Node 不覆盖重试另由对应记录标记
- **16 份本轮新 LibreOffice PDF，8 组独立目标参考→转换输出比较**：共 48 条文字行的页码、文字、x / 行末 x / y 坐标一致，最大差 **0.0 pt**。首行 / 悬挂缩进的固定物理锚点另有数值断言；各对照显式匹配字体 / 字号 / 粗斜体以隔离段落数值，不声称转换标题时原默认外观不变
- 16 份已生成 PDF 的 8 对截图裁切像素完全相同，联系图实际查看；`tmp/word-block-type/run-xmx2wu5y/visual-evidence/heading-body-contact-sheet.png` 与同目录 `visual-report.json`。这是当前参考对子集的证据，不是完整 Word/WPS、字体替代、复杂表格或自动分页保真

独立检查同时保留既有源导入差异：SampleDoc 含主题 / tint 颜色，XML fallback `548DD4` 在导入阶段解析为 `558ED5`；本轮 Rust 导入器未变，类型转换与导出精确保留的是导入后的 RGB，不把 fallback 值误当作固定色参考。先前中间脚本 / harness 运行目录保留，以上只使用最终源码身份匹配的 `run-xmx2wu5y`。

### 剩余可达入口与下一步

本轮是**工具栏**修复。实际注册的上游快捷键回调 / 输入规则处理器另有 8 项无 DOM 复现：Windows Ctrl+Alt+1 / 2 / 3 对两段选区仍清空七个显式段落字段；Ctrl+Alt+0 或对两段 H2 再按 Ctrl+Alt+2 切回正文也清空；段首键入 `# ` / `## ` / `### ` 的输入规则连单段格式也清空。准确选区、参数和说明见 `tests/ui/word-block-type-upstream.spec.mjs` 与功能文档。这里没有实际浏览器按键执行声明。

下一独立增量应将这些可达入口统一到同一保格式 / 原子拒绝 / 历史规则，再补真实浏览器与原生验收。完整 DOC/DOCX/PDF 阶段、源命名样式动态语义、Excel / PPT 原缺口仍未完成。

## 上一轮：同样式上下文间距的有界固定值转换（2026-10-13）

基于 `8b11d7472b3f087598db22a19e7d6c7bace2e655`，新增源样式身份、有效继承和完整原始段落顺序证明，将可证明的 `contextualSpacing` 初始间距转换为现有固定 `spaceBefore` / `spaceAfter`。审计/编辑前提示明确说明命名样式与重排的动态关系不保留；沿用普通间距修改/默认/重置、撤销、草稿和可信重开，不增加虚假的上下文开关或模型字段。范围及整文档内容限制见 [CONTEXTUAL_SPACING.md](CONTEXTUAL_SPACING.md)。

### 语义证据和明确拒绝

未改造 python-docx `List Number` / `List Bullet` 模板在冻结旧 CLI 上均因上下文间距拒绝，Normal 对照通过；新 CLI 接受这两种原模板，未删除其任何上下文声明。独立 ISO 引文与 LibreOfficeDev 实测发现：只有一侧启用且两个相邻间距均为正时，标准的“折叠后相减”与消费者的“先取消单侧再折叠”结果不同。该情况仍拒绝；同样式双侧启用、单侧带明确零、关闭及不同有效样式的可证明边界才转换。

原始叶样式身份与继承格式分开保存于临时证明，默认引用、显式 false/零、部分覆盖、真实空段、标题/列表/表格顺序均核对。相邻列表/正文可以比较，整表和每个单元格边界不能串联。使用中的显示名称与缺名 ID 碰撞、无已声明默认样式时混合隐式/显式身份，消费者都可能合并，因此保守拒绝；大小写/空白处理更保守，不据此承诺任意样式别名兼容。编号或表格样式中的上下文声明（包括 false）不猜测；编号标题也必须核对活动编号层。相关活动上下文触发的是整文档序列证明，文档中任何图片或手动分页/分栏均可能阻止转换，即使该段自己不启用上下文。原没有相关声明的路径保留。

先完整比较转换后的文字、段落种类、列表归属、原始间距和容器 token，再一起写入解析得到的固定值；不把中途修改后的间距用于下一边界。样式/编号解析缓存并有总工作预算，使用与解析器相同的有界快照及实际 OPC 角色，不声称任意 OPC 一致性或完整 Word 排版。

### 最终源码和编辑验证

- Rust **1.99 / 1.88 核心各415项通过**：原388 +24项源/模型集成 +3项原子应用/缓存压力/工作预算测试。日志 `tmp/contextual-final2-core-1.99.log`、`tmp/contextual-final2-core-1.88.log`；两工具链相同集合不重复累计
- **242项无浏览器前端检查通过**：原223 +19新固定格式/历史/种类转换检查；`tmp/contextual-ui.log`，完整命令 `tmp/contextual-ui-command.json`。真实 Tiptap/ProseMirror 事务覆盖重排、单段转标题、分段、列表、单元格选区、普通间距控件修改与重置、显式零、null省略、无效候选原子性、撤销/重做和打印 DOM 声明。没有把无 DOM 事务当作实际浏览器排版
- `npm run build`、最终 CLI build、根包格式、Python/JS语法、diff检查通过；前端仍有既有大bundle提示。`tmp/contextual-build.log`、`tmp/contextual-final2-focused.log`、`tmp/contextual-final2-fmt-root.log`。127个前端文件/配置的散列快照 `tmp/contextual-ui-identity.json` 与最终源码相同
- **2个新增浏览器规格仅发现，未执行**：`tmp/contextual-browser-discovery.log`，覆盖预先警告、取消/接受/编辑与不支持源保留只读。Chromium EPERM、云浏览器/工作区访问、Linux glib/WebKit 与 Windows 离线等限制未重试或绕过，实际浏览器/WebView/原生 PDF 仍未验收
- 原始严格 all-targets Clippy 两版本仍各仅既有单项：1.99 XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`；各只排除对应一项后通过，日志 `tmp/contextual-final2-clippy-{1.99,1.88}-{raw,scoped}.log`。没有声称 raw 严格全绿，没有新增 lint 豁免
- 解析库 **1,264项重新通过**，`tmp/contextual-final2-vendor-tests.log`；vendor、锁文件、导出器和模型 schema 未变，两份旧补丁 reverse-check 通过。共享 XML helper 只扩大 crate 内可见性，没有更换解析语义

最终 CLI：`tmp/contextual-final2-cli`，SHA-256 `a6179a5e683c7c2b0ddbf93d55afad3fac67b6549f442ee093b8ea035f21e80f`，清理后重建的 `target/debug/rust-wps` 与之完全相同。初版413项结果及中间CLI未冒充最终415项。

MSRV重新编译两次因磁盘 ENOSPC 中断，原始日志保留为 `*enospc*`，不计测试通过。停止 Cargo 后仅清理可重建的 inactive incremental 和 rust-wps 包构建工件，保留全部源码、输入原件、冻结CLI、报告与在跑回归；之后两工具链完整重新通过。旧间距脚本一次 LibreOffice 运行在此期间失败，保留 `tmp/contextual-old-spacing-final2/run-j40l2us9/report.json`；释放空间后新的独立目录完整通过，不删失败案例或放宽断言。

### 独立源、原件和新渲染

最终独立报告 `tmp/word-contextual-spacing/final2-y9n0nsdy/report.json`，SHA-256 `a4949aba08e3591534e5b3f82253412244d056e4e305396e589aa2e875129bb9`，**326条实际CLI命令，30支持源 / 7明确拒绝**。脚本 `scripts/check-word-contextual-spacing.py` 与独立预期 `tests/fixtures/word-contextual-spacing-expected.json` 分开；真正未改造模板、受控合成包、另行创作的数值参考及固定原件明确分类。完整模型可信重开、实际固定值编辑后可信重开、独立 python-docx/XML 数值与文字、另存不覆盖、源/输出/参考/脚本/manifest/CLI散列检查均通过。外部重新导入单列，未代替可信模型相等。审计及import的准确规范化警告均有断言：28个相关源出现，2个不活动对照不出现。

**164份本轮新LibreOffice PDF**：37源 +37独立数值参考 +30导出 +30实际编辑导出 +30独立编辑参考。受支持的**60组应用保留比较**（30源→输出、30编辑参考→编辑输出）以及另30组源→数值参考校准，所有标记的页面和绝对 y 基线一致，最大实测差 **0.0 pt**；坐标与相邻距离断言阈值为小于0.3pt，未放宽以吞掉已发现的0.5–1.35pt差。7组拒绝校准单独记录，其中4组非对称数值、2组样式名称/缺名身份碰撞、1组无已声明默认样式的混合身份；不将它们计为支持版式正例。不把有限 Latin 标记基线扩展为整个文件/任意字体/自动分页的保真承诺。

最终运行在磁盘故障后写入独立 `/tmp` 目录；恢复空间后复制到上述仓库证据目录，456个原始/复制文件逐一SHA相同，路径映射及原报告内绝对路径保留在 `tmp/word-contextual-spacing/final2-y9n0nsdy-copy-provenance.json`。三张联系图从14份已生成PDF制作并实际查看，覆盖原始列表、非对称拒绝、表格边界、空段和外侧锚点；未重新生成源DOCX或PDF。检查清单 `tmp/word-contextual-spacing/final2-y9n0nsdy-visual/visual-review.json`。

本轮真实发现的旧规范化差异不被掩盖：裸 python-docx 表格因既有作者边框增加0.5–1pt纵向差；源DejaVu12空段没有可保存的段落基线字体，作者缺省行框造成1.35pt差。冻结旧CLI在分别创作的数值参考上也复现，证据 `tmp/contextual-independent/table-baseline/report.json`、`tmp/contextual-independent/empty-baseline/report.json`。最终上下文表格控件明确作者声明0.5pt六边边框，空段控件采用一致Arial11字体以隔离该能力；没有静默改动原始stock列表或放宽测量容差。原差异及失败报告保留，任意空段基线字体仍是已知缺口。

**3份未改造、固定revision/大小/SHA/Git blob的上游活动上下文原件**：LibreOffice `indentation.docx`、POI `61787.docx`、`60316.docx`，仍因独立页面/缩进或附属内容问题拒绝。来源与许可范围在 `scripts/contextual-spacing-corpus.json`；没有找到完整正向原件，不将拒绝样本称为保真支持，不提交原二进制，也不称为用户生产文件。两种原样python-docx列表是未修改模板正例，与这些上游原件分开。

### 历史兼容和保护

最终 `tmp/contextual-legacy/run-h1sni1hf/acceptance-report.json` 为PASS，完整原始差异报告保留为同目录 `report.json`。使用上述最终2CLI，重放先前受验收间距、缩进、表格宽度/位置/边距、图片尺寸/段落、页面、隐藏文字及附属内容等报告的 **1,520条去重read/audit/import契约**：

- **1,510条完全相等**；没有用“返回任意错误”替代原保护
- **8条有意支持扩展**：4个旧单段上下文样本（直接、已用样式、文档默认、表格单元格）各audit/import。audit只将已解决的间距阻断换为精确规范化警告；完整新模型按独立固定的页面、文字、字体、容器与0/0/240预期核对
- **2条仅诊断增加**：原单图直接/样式上下文拒绝的audit新增具体“drawing无法通过上下文证明”原因；原阻断集合和import精确错误保持，未放宽图片保护
- **302份按整文件SHA去重的历史DOCX字节金样**重新编码完全相同，含无新属性V1/V2及此前间距/缩进/表格/页面/图片模型；**604次完整可信重开、604次不覆盖检查**通过。各旧输入/参考报告、冻结CLI散列不变。旧路径与专项存在交集，不把它们简单相加为独立文件总数
- **325份旧PDF仅登记并核对散列**；这一重放组新渲染数为0，不称为325次新视觉验收。当前视觉证据使用上述164份新上下文PDF及下一条14份新旧间距PDF

原段落间距独立脚本在最终CLI重新通过 **23支持 / 19拒绝 / 14新LibreOffice PDF**，`tmp/contextual-old-spacing-final2-retry/run-qoba7ozi/report.json`；此前19/23改为23/19正是上述4个单段受支持扩展，未删除这些测试。固定POI `SampleDoc.docx`普通基线、完整可信重开、源散列和禁止覆盖继续通过，不把这份普通基线称为真实上下文正例。最终410个产品/解析库/测试/脚本/配置散列与冻结快照 `tmp/contextual-product-final-identity.json` 相同；文档单独完成并纳入最终审查树。

### 下一步

本轮新增19个无浏览器用例包含一个明确标注的既有缺陷回归：多段选区转为标题会把各段间距重置为缺省，单段转换则保持；撤销恢复原值。最小复现及成因在 `tests/ui/word-contextual-spacing-model.spec.mjs`：选两个不同间距段落后调用 `setHeading`，Tiptap只在同一父节点选区复制属性。下一独立增量应在应用层段落种类工具栏实现逐段格式保留和一次撤销，不编辑node_modules，也不把当前重置行为说成理想语义。完整DOC/DOCX/PDF复制仍未完成，Excel/PPT优先级不变。

## 上一轮：简单表格统一水平单元格边距（2026-10-13）

基于 `a4a2bf26f1b8c279342a5999b7d53d27dd4186ce` 新增 `table.attrs.tableCellMarginsTwips` 完整物理左右整数 twip 对。支持当前正文顶层简单表格、显式零、一次撤销、草稿、DOCX 与可信重开和不可变打印声明。所有列须保留正正文宽度；段落行框按实际边距复查，缩窄页面/列宽或增列失败时整次拒绝。范围、旧字节与继承/规范化边界见 [TABLE_CELL_MARGINS.md](TABLE_CELL_MARGINS.md)。

### 测量先于扩展

现代 compatibilityMode15 **没有解禁**，也没有新增 border-reference 属性或全局改模式。前置独立 python-docx/XML→LibreOfficeDev 探针与 0.25/0.5/1/2 pt 边框控制发现：简单加左边距仍有 0.25–0.5 pt 系统差，零边距受半边框下限影响；本应用实际旧包控制亦复现。没有把消费者特有的半边框公式写进产品。证据 `tmp/table-cell-margins-investigation/semantic-summary.json` 和各子目录。现代**无缩进**源若只保留边距数值，会明确提示兼容模式相关位置规范化，绝不归入位置相等结果。隐式默认自定义边距同样不能凭一个消费者或编辑器缺省值猜测。

### 最终源码与前端

- Rust **1.99/1.88 核心各388项通过**：原364 +13模型/编码/草稿/安全 +11源证明测试。日志 `tmp/cell-margins-core-final.log`、`tmp/cell-margins-core-1.88.log`；两工具链同一集合不重复累计，1.88沿用独立 `tmp/msrv-target`
- **223项无浏览器检查通过**，包含26项新边距检查及原197项集合；`tmp/cell-margins-ui-final2.log`，完整命令 `tmp/cell-margins-ui-command.json`。覆盖整对类型/零/缺省、当前表格选区、marks、撤销、无效候选原子性、宽度/页面/增列复查、失效控件、HTML自有元数据、挂载表格视图重置与打印克隆。早先两次全量尝试各有5个跨格式 Python 测试环境错误：缺 USERPROFILE、随后缺原 Linux 测试别名；恢复指向已安装 Python 的路径别名后全223通过，未改测试断言或产品来掩盖失败。相关单元格布局检查没有启动浏览器
- **6个新增真实浏览器规格仅发现，未执行**；`tmp/cell-margins-browser-discovery.log`。既有 Chromium socket EPERM、云浏览器访问受阻、Linux glib/WebKit 和 Windows 不在线限制未重试或绕过；实际浏览器/原生打印仍未验收
- `npm run build`、CLI build、`cargo fmt --check`、Python/JS语法与 `git diff --check` 通过；前端仍有既有大 chunk 提示。日志 `tmp/cell-margins-build.log`、`tmp/cell-margins-cli-initial.log`、`tmp/cell-margins-fmt.log`
- 原始严格 Clippy 两版本仍各仅既有跨格式单项：1.99 XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`。只排除对应单项后 all-targets 通过；`tmp/cell-margins-clippy-scoped.log`、`tmp/cell-margins-clippy-1.88-scoped.log`。原失败日志带 `raw`，没有声称原始严格全绿
- 解析库 **1,264项本轮重新通过**，`tmp/cell-margins-vendor.log`；vendor、锁文件不变，两份既有补丁 reverse-check 通过

最终CLI SHA256：`944a3a9d17630d7113c2d38153c79cbf4dcf80e48ad4ada32184d48816521684`，固定副本 `tmp/cell-margins-initial-cli/rust-wps`；“initial”目录名是冻结时建立，不表示最终用了不同产品代码。

### 独立源、真实文件与新PDF

最终报告 `tmp/word-table-cell-margins/run-0baqvkr8/report.json`，418条实际CLI命令；此前 `run-4uookvjo` 为较窄通过记录，未用新结果覆盖旧文件。定向源由独立 python-docx/OOXML 生成，预期另存；source/output XML 与 python-docx 精确核对网格、单元格宽、完整文字和边距。**27支持源、23源拒绝、22非法模型、3包/模型篡改**通过，另验证显式108/108自有模型耐久、恢复省略后的整包字节、原件哈希和禁止覆盖、完整可信模型与外部重导入。

**108份本轮新LibreOffice PDF**：26份受测源 +26份独立编辑参考 +52份受测导出 +3份相邻表格仅视觉对照 +1份现代拒绝校准。受测源→输出、真正边距编辑→独立参考共有 **51组位置保持比较，最大差0.00pt**；另1组现代无缩进输出独立报告 **0.50pt规范化**，不计为保真相等。计数按实际文件清单，不能把视觉-only或拒绝校准混入支持组。

新增分隔的双表对照避免相邻表格被消费者合并：旧缺省表、新非对称边距+720twip缩进表与显式字体/间距/段落缩进锚点逐表测量。源→输出及编辑参考→输出的边框、所有文字x/y坐标差 **0.00pt**。真实编辑使第一表文字右移6.6pt，第二表边框左移3pt、首列文字仍保持；换行增加使后续内容下移28pt，与独立参考一致，不能把自然重排说成“所有段落都不动”。详细散列与坐标在同目录 `separated-mixed-summary.json`。四张最终联系图实际查看；相邻旧对照保留为视觉-only，不当作逐表坐标证明。

**4份未改造的固定上游文件**单列：`Bug66988.docx` 物理55/55边距仍因百分比表宽、负缩进和页码元数据等独立问题拒绝；七表 `table-indent.docx` 仍有逻辑自定义边距、现代模式、不同单位/负缩进、页面适配及pgNumType阻断；六表 `table-alignment.docx` 保留既有自动宽度/继承位置明确规范化正对照；`SampleDoc.docx` 为普通基线正对照。没有找到能在本子集完整转换的未改造自定义边距原件，因此不冒称真实自定义边距正对照。来源/版本/size/SHA256/Git blob与许可限制见 `scripts/table-cell-margins-corpus.json` 及复用清单；未提交原二进制，未把它们称为用户生产文件，也没有扩大页码元数据处理。

### 旧路径与明确复用证据

- 原表格位置独立脚本本轮重新通过 **35支持/39拒绝/29非法模型/2原件**，`tmp/cell-margins-table-position/run-bk8o39re/report.json`；本次未重新渲染该旧套件。3个旧拒绝改为有证据的支持：统一0/0、216/216的直接缩进与固定样式继承零缩进。非统一单格覆盖、现代、逻辑、未知单位等拒绝保留
- **319条去重旧表格相关 audit/import 响应：313完全相等、6条预期支持扩展**，对应上述3源各audit/import；新audit仅去掉该已解决的表格几何阻断，新完整模型精确确认边距和缩进。196种输入SHA身份不变，`tmp/cell-margins-legacy-checks/table-source-equivalence.json`。不是用“任意错误”替代原保护，不把扩展响应称为逐字相等
- **109份按整文件SHA去重的历史DOCX字节金样通过**，覆盖原117个历史工件路径与上一轮54个输出路径的交集；不能把171个路径加成171份独立文件。当前CLI重新编码整文件字节相同，218次完整可信重开、218次不覆盖，全部旧输入不变；`tmp/cell-margins-legacy-checks/absent-margin-goldens/report.json`
- **119份旧PDF明确复用**：固定宽度6、表格位置51、页面40、段落缩进22。对应39种自有DOCX输入由最终CLI重新生成且整文件字节相同，原始/参考输入及PDF散列不变；`tmp/cell-margins-legacy-checks/retained-table-pdfs/report.json`。这一组新渲染数为0，不能把它称为119次新渲染；其39份输出与109份金样可能重叠，不相加宣称独立文件总数

本轮证据只覆盖声明的有限几何、独立消费者和安全路径。完整DOC/DOCX/PDF复制及后续Excel/PPT仍未完成；下一步现代缩进需要更完整边框/定位语义及多消费者证据，不能通过全局模式切换或经验偏移跳过保护。

## 上一轮：有限段落左右、首行与悬挂缩进（2026-10-13）

基于 `f47da946026679a6cfa8f28c39ec553863179eff` 新增可选整数 twip 的 `indentLeftTwips` / `indentRightTwips` / `firstLineIndentTwips`。正文、标题及简单顶层表格直接单元格段落支持默认/显式零、混合选区、逐项重置、一次撤销、草稿、DOCX 与不可变打印声明。首行负值表示悬挂；左右及首行行框必须适合实际页面/单元格宽度，缩小页面或改列宽时先检查完整候选。范围与未支持语义见 [PARAGRAPH_INDENTATION.md](PARAGRAPH_INDENTATION.md)。这不是完整 Word 行布局、制表位、列表几何或图片缩进支持。

### 最终源码与前端

- Rust **1.99 / 1.88 核心各 364 项通过**（原315 + 17模型/导出/草稿/保护 + 32源审计测试组），日志 `tmp/indent-core-final3.log`、`tmp/indent-core-1.88-final3.log`。两版本同一集合不重复累计，1.88使用独立 `tmp/msrv-target`
- **197 项无浏览器前端检查通过**（原165 + 新32），`tmp/indent-ui-verified.log`；实际命令保存于 `tmp/indent-ui-command.json`，Linux使用既有 `USERPROFILE=$PWD/tmp/linux-home` Python路径映射。真实ProseMirror事务覆盖当前/多段/单元格、默认/混合/零、隔离共享chain事务、存储marks、一次撤销/草稿、过期选区/锁、页面/列宽/结构失败原子性、制表符和不支持容器拒绝、DOM解析/序列化及打印样式；假DOM不是浏览器布局
- 前端构建、CLI构建、Rust格式、Python/JS语法和diff空白检查通过。`tmp/indent-ui-build-verified.log`、`tmp/indent-build-final3.log`、`tmp/indent-fmt-final3.log`；仍有既有较大bundle提示。新增 **9个真实浏览器规格仅发现/语法检查，未执行**，`tmp/indent-ui-discovery-verified.log`
- 最终两工具链原始严格Clippy仍分别仅已有跨格式单项：1.99的XLS `chunks_exact_to_as_chunks`、1.88的PPT `uninlined_format_args`。各只排除对应既有单项后all-targets通过；`tmp/indent-clippy-raw-final3.log`、`tmp/indent-clippy-1.88-raw-final3.log`和对应非raw日志。不声称原始严格检查全绿，没有扩大豁免
- 解析库 **1,264项通过**（新增1个继承单元测试），`tmp/indent-vendor-final2.log`。vendor只在原已修改的 `formatting.rs` / `convert_docx.rs` 中增加分量继承、首行/悬挂槽覆盖和零保持；未全目录格式化，锁文件未改。精确重建的原始128文件全部匹配既有crate清单，两份补丁反向检查通过，`tmp/indent-upstream/verification.json`
- 后续修正仅涉及源缩进审计、对应Rust测试和导入提示，UI/vendor未再改动；`tmp/indent-ui-vendor-identity-final.json`核对249个UI/vendor/锁文件与之前冻结快照相同，保留上述已执行验证而非重复声称新执行。实际Windows/Tauri、浏览器、WebView2/系统打印限制没有重试或绕过

审查与独立检查修复了：先验证默认宽度造成的错误拒绝、只检查非零而漏掉显式零的单元格边距、被包装的样式/编号身份、解析器非法编号ID回退0造成的审计绕过、未知样式的消费者回退不确定性，以及合法 `w:lvl/w:pStyle` 被误当作段落属性而拒绝普通列表。错误中间运行均保留，没有将失败样本删除或放宽为任意拒绝。样式解析已缓存/索引，编号有总工作预算；1,500段/2,000未用样式的正向压力和活动编号工作上限拒绝均覆盖。

现有单图段落的明确缩进规范化保留：完整媒体/几何/段落身份验证后才移除精确相邻空占位段落，并增加具体警告；真实空段的缩进仍核对。普通编号层级仍明确规范化，不能冒充新普通段落属性。已有 `contextualSpacing` 保护未放宽：完全原样的python-docx列表样式仍可能被既有间距审计拒绝；新列表正对照为明确标记的定向合成文档，保留原生编号定义但去掉了其不支持的活动上下文间距，不称为未改造的完整默认模板。

### 独立缩进文件与PDF

最终CLI SHA-256：`8d5ad179c053831bea0c68f7daacc183ea169eaa129cfcfa20ad1d530589d82f`。最终报告 `tmp/word-paragraph-indentation/run-zfejj0t1/report.json`，日志 `tmp/paragraph-indentation-independent-final3b.log`：

- **25支持 / 47明确拒绝 / 37非法模型 / 6包或模型篡改 / 3实际模型编辑 / 12旧缺省整文件字节案例 / 4上游文件 / 22 LibreOffice PDF**。源由python-docx与标记的OOXML边界变体生成，预期独立保存于 `tests/fixtures/word-paragraph-indentation-expected.json`，不从应用输出反推
- 所有受测源→输出和实际编辑→独立作者参考的横坐标及换行比较最大差 **0.0pt**。左右、首行/悬挂、标题、固定单元格、居中/右对齐组合、六个跨层覆盖/零重置案例均覆盖；右缩进2,880twip使受控文字从6行变8行。相反方向覆盖/零重置的实际首行相对位置为−18/+36/−18/+36/0/0pt
- **未经修改的可见缩进真实正对照**：LibreOffice `test_indentation.docx`，revision `6e5f11edfc5f080cf99e1807916df5cedec785b7`，SHA `e999c8a177efaefa7121e8adcabdee99cadd41722164e77038579d852cce92c7`。直接左缩进567覆盖样式1701；原件/输出五行起点均约100.45pt，差0.0pt。POI六张表格原件另有六个缩进空段，单列为空段正对照，不冒充可见文字缩进证据；另两份未修改上游原件为具体拒绝诊断
- 上游来源、固定Git blob/文件哈希、正文与声明清单、仓库许可及单个二进制原作者/再分发权利未单独确认的限制见 `scripts/paragraph-indentation-corpus.json`。原件不纳入仓库，不称为用户生产文件。ECMA规范来源与非负源单位依据同清单
- 所有72个定向源的文件哈希、不覆盖、失败无新输出、完整可信模型重开与反篡改通过。外部重新转换保留缩进/文字/结构，但完整模型仅2/25完全相同，其余23份差异仅为共42处既有 `textColor=000000` 标记增加，无其他差异（`external-reimport-differences.json`）；没有将外部重导入当作可信模型相等
- 脚本、预期清单、语料清单、辅助脚本、CLI及源文件的前后哈希均一致。12份无新缩进V1/V2模型与本轮前CLI `32c4fa62…` 整文件字节相同，包含显式页面、固定/自动表格、图片和间距；它们不与旧117份基线相加为独立文件总数

这些PDF只证明LibreOfficeDev下受控Latin字体和有限行位置/换行，不能替代简文CSS、真实浏览器/原生打印或Word/WPS完整视觉验收。正文、标题、表格、对齐、继承及真实原件共六张联系图已人工查看，清单为最终报告目录的 `visual-qa/manifest.json`。

### 既有路径复核

最终复核 **PASS** 报告 `tmp/indent-final-legacy-equivalence-yb35s9hq/acceptance-report.json` 使用上述最终CLI；同目录 `report.json` 保留发现下述两条诊断差异时的严格相等失败。此前 `tmp/indent-final-legacy-opry4biu/report.json` 因普通列表审计误拒绝而失败，仍保留为诊断，不把它的聚合结果计为通过。

- 重新执行 **117份历史整文件字节基线、468次完整可信模型重开、234次不覆盖**；全部源不变。这些基线与各专项有所重叠，不能相加为独立文件总数
- 重新执行段落间距 **19支持/23精确保护拒绝**，新渲染 **14份PDF**；普通列表和表格单元格正对照通过。另两份普通Letter自动表格核对正文宽8640、列宽 `[4320,4320]` 与具体规范化提示
- 重新执行 **3份POI原件**、2次DOC转换完整可信模型、精确全文段落和1个手动分页、替换后13个未改部件字节、5次不覆盖。没有独立DOC解析器/视觉排版验收；PowerShell未执行，使用Linux等价CLI流程
- 七组旧渲染专项重放 **1,998条CLI命令，1,996条精确相等、2条明确解释的诊断差异；269份重建DOCX整文件字节相同**。逐条重放原记录CLI命令，使用原输入的精确副本，比较完整JSON/模型、退出码、精确保护错误与重新生成的完整DOCX字节。固定列宽 **11固定+2规范化/26拒绝/16非法模型**；表格位置 **32支持/42拒绝/29非法模型**；隐藏文字 **2拒绝+1正对照**；附属内容 **6拒绝+1正对照**；图片尺寸 **13支持/51拒绝/27非法模型**；单图段落 **21支持/36拒绝/88非法模型/2边界/13历史字节**；页面 **18支持+3规范化/49拒绝/73非法模型/4篡改/5历史字节**
- **181份既有LibreOffice PDF明确复用**：宽度6、位置51、隐藏4、附属8、图片尺寸26、单图46、页面40。它们由之前的不可变CLI `71135677…` 渲染；只有最终CLI重建的输入/输出DOCX整文件字节一致后才复用消费者证据，不能称为181次新渲染。与本轮新22份缩进PDF及新14份间距PDF分开记录
- 唯一解释后的诊断差异是图片尺寸 `reject-list-image.docx` 的审计/导入两条命令：修复合法编号层级 `pStyle` 后去掉先前错误附加的缩进风险，精确图片几何阻断、拒绝结果、无新输出与其余响应内容保持不变。原件SHA `d348579ca631cec1ea92954a8847137c859b864212f17dd376857cb2ccd10ada`；这两条不是逐字相等，也不是放宽图片保护
- 最终前后 **469个产品/解析库/脚本/配置/原件文件哈希不变**；文档编辑独立于产品冻结快照。逐条计数与差异记录以最终报告为准

后续优先考虑有界的水平单元格边距与现代表格缩进参照：目前这些声明限制可以证明文字位置的表格范围，需要真实原件与消费者坐标后再扩大支持，不直接删除保护。完整DOC/DOCX/PDF复制及Excel/PPT后续阶段仍未完成。

## 上一轮：单节页面尺寸、正文边距与共享打印几何（2026-10-13）

基于 `60c1dd08c196889205ce3ce312c98fea073d0295` 新增可选根属性 `pageGeometry`。受支持的 A4/Letter/A5 横纵向和四边正文边距以整数 twip 保存；±2 twip 名义尺寸容差只用于识别，不修改源整数。完整模型、缺省历史语义、源页面证明/拒绝边界与自动表格规范化见 [PAGE_GEOMETRY.md](PAGE_GEOMETRY.md)。没有扩展为多节、页眉页脚、完整 Word 分页或任意纸张。

### 最终源码与前端

- Rust **1.99 / 1.88 核心各 315 项通过**（原 287 + 17 个模型/导出/草稿/保护测试 + 11 个源页面测试组），最终日志 `tmp/page-core-final3.log`、`tmp/page-core-1.88-final3.log`。两版本是同一套集合，不重复累计；1.88 使用独立 `tmp/msrv-target`
- **165 项无浏览器前端检查通过**（原 141 + 新 24），日志 `tmp/page-geometry-ui-regression-final.log`，完整命令 `tmp/page-geometry-ui-command.json`。覆盖精确整数、选区/存储 marks、预设/方向/边距、过期控件/锁、一次撤销/重做、草稿、DOM 声明、自然图片框、普通自动表格派生宽度、固定/定位拒绝、重置后的装饰清除和不可变打印快照。使用真实 ProseMirror/CommandManager 的失败 `chain/can` 检查，假 DOM 仍不是浏览器布局
- `npm run build`、CLI 构建、`cargo fmt --check`、Python/JS 语法与 `git diff --check` 通过；构建仍有既有大 chunk 提示。新增 **11 个浏览器规格仅发现/语法检查，未执行**；`tmp/page-geometry-build-final.log`、`tmp/page-geometry-browser-discovery.log`
- 原始严格 Clippy 仍各仅既有跨格式单项：1.99 的 XLS `chunks_exact_to_as_chunks`、1.88 的 PPT `uninlined_format_args`。只排除各自单项后 all-targets 通过，`tmp/page-clippy-final3.log`、`tmp/page-clippy-1.88-final3.log`；原始失败日志 `tmp/page-clippy-raw-final.log`、`tmp/page-clippy-1.88-raw-final.log`。不声称原始严格检查全绿，没有修改 Excel/PPT 来避开它们
- 原生 PDF 的纯几何/验证/发布函数与测试从最终源文件逐字提取，**15 项在两种 Rust 版本均通过**；`tmp/native-page-geometry-tests-final.log`、`tmp/native-page-geometry-tests-final-1.88.log`，该独立 harness 的严格 Clippy 通过。提取身份核对 `tmp/native-page-geometry-source-identity.json`；不包含 Tauri command/WebView2 COM 的执行或原生渲染
- 新显式 Word PDF 路径验证所有页的 MediaBox，以及继承/引用的 Rotate/CropBox、UserUnit、循环/深度、错误发布原子性；没有套用演示文稿的固定页数，也没有收紧旧无页面/演示发布路径。实际 Windows 构建、WebView2/系统打印仍未验收
- vendor、Cargo.lock 和 package-lock.json 未改。没有重试已确认的 Chromium socket EPERM、云浏览器访问本地预览失败、Linux Tauri glib/WebKit 缺失或 Windows 不可用限制

早期完整回归发现颜色/字号/DOC 转存的“外部重导入”预期没有新页面属性；更新预期时保留完整根几何，没有删除新证据来假装模型相同。可信自有重开保持原模型语义。多项保护风险共存时现在报告全部阻断原因，避免新的页面风险按字母顺序掩盖既有间距/表格/隐藏内容风险。

审查修复：页面候选事务不能先修改 Tiptap 共享事务；原生 MediaBox 正确仍可能被 Rotate/CropBox/UserUnit 改变可见页面；自动表格派生宽度必须用于首次数值固定列宽设置，否则窄页面中的合法编辑会被旧 9000 twip 初始化阻止。相应失败原子性和反例已纳入测试。

### 独立页面文件与 PDF

最终 CLI SHA-256：`32c4fa62845fccba00d7e29f11a11484d67f546b5ab21d3dd09597148e289e64`。页面报告 `tmp/word-page-geometry/run-zti35phg/report.json`：

- **18 个支持源 + 3 个整项缺失的规范化源 + 49 个明确拒绝源**；另有 **73 个非法模型、4 个包/模型篡改、5 个历史整包/逐部件/重置字节基线**。定向源由 python-docx 生成，期望另列 `tests/fixtures/word-page-geometry-expected.json`，不由应用输出反推
- **40 个 LibreOffice PDF**，其中 9 份受控源与输出、9 次真正页面编辑与独立作者参考的文字/图片中心/表格锚点差均为 **0.0 pt**。覆盖六种纸张/方向、非对称边距、原始舍入整数和实际编辑；原始 OOXML/python-docx 保持精确 twip，LibreOffice 对临近标准尺寸会吸附纸张，PDF MediaBox 因此允许 0.15 pt 测量容差
- 普通未定位自动表格另列为**明确宽度规范化**：A5 的 6231 twip 总宽对应 `[3116,3115]`，默认 python-docx Letter 的 8640 正文宽对应 `[4320,4320]`；更宽页面恢复 9000。输出与独立规范化参考的位置差为 0.0 pt，不能把它称作原自动列宽保真。固定或带任意位置属性的表格仍不自动缩放；源或编辑超出正文区时明确拒绝
- **2 份未改造、固定哈希的真实上游正对照**单列：Apache POI `SampleDoc.docx` 的 11907×16840/四边 1440，以及六张自动表格的 `table-alignment.docx` 的 Letter 页面。尺寸保留与自动表格宽度/继承位置规范化分别核对；不是用户生产文件，原作者与再分发许可范围未全部审查。来源/版本/哈希及库存见 `scripts/page-geometry-corpus.json`
- 源文件哈希不变、禁止覆盖、失败无新输出、完整可信模型重开、包部件集合/哈希/重编码反篡改通过。源/输出/页面编辑的联系表单独实际查看；可见变更与预期一致，没有把 PNG 检查当作浏览器或 Word/WPS 验收

### 既有路径回归

同一最终 CLI 复测图片尺寸 **13 支持 / 51 拒绝 / 27 非法模型 / 1 原件 / 26 PDF**，`tmp/word-image-dimensions/run-r6_j6kxt/report.json`；单图段落 **21 支持 / 36 拒绝 / 88 非法模型 / 2 边界模型 / 13 历史整包字节 / 1 原件 / 46 PDF**，`tmp/word-image-paragraphs/run-we2dcump/report.json`。两个既有脚本完全未改；图像位置/基线证据与最终视觉检查继续通过。

最终六组旧路径与历史字节汇总 **PASS**：`tmp/page-geometry-final3-legacy-wtjyqaze/report.json`，217 个产品源码、脚本、CLI 和原件的前后哈希完全一致，验证错误为零；文档编辑不在源码冻结快照内。

- **117 份此前接受的无页面属性模型**在旧 CLI `b68e823c…` 与本轮最终 CLI 下输出整文件字节完全一致；468 次完整可信模型重读、234 次禁止覆盖检查通过。`absent-page-goldens/report.json`。与页面/图片专项中的基线组有重叠，不把它们相加为独立文件总数
- 段落间距 **19 支持 / 23 精确间距保护拒绝 / 14 PDF**，脚本未改；两份普通 python-docx Letter 自动表格额外由独立读取器确认 8640 总宽、`[4320,4320]`、原页面边距和明确规范化警告，`default-letter-auto/report.json`
- 固定列宽 **11 固定 + 2 规范化 / 26 拒绝 / 16 非法模型 / 6 PDF**；26 包含原 24 个具体保护及新增 2 个显式页面超宽拒绝。位置 **32 支持 / 42 拒绝 / 29 非法模型 / 51 PDF**；42 包含原 39 个具体保护及新增 3 个显式页面定位超宽拒绝，另验证 3 次页面感知编辑失败无输出
- 原始 POI 语料 3 份、2 次 DOC 转 DOCX/完整可信模型相等、独立完整段落/手动分页、替换后的其余 13 个部件不变及 5 次禁止覆盖通过；这是 Linux 等价 CLI 步骤，没有声称执行 PowerShell 或独立 DOC 视觉验收
- 隐藏文字定向 2 个精确拒绝与正对照、原始 Tika、4 个 PDF；附属内容 6 个精确拒绝与正对照、8 个 PDF 均通过。真实 Tika 或七表 POI 原件可能同时有多个阻断原因，保留组合诊断，不将其误称为单独页面或隐藏文字分支证明

所有旧套件中的 83 个 PDF 仍只是相应受控 LibreOffice 探针。一次中间汇总因最终 UI 补丁/模块格式变动而正确报告源码哈希变化；最终在所有产品文件冻结后从新目录完整重跑通过，没有复用旧输出冒充最终验证。

本轮期间普通 python-docx Letter 自动表格暴露了旧固定 9000 twip 作者宽度造成的新拒绝。最终没有去掉这些源的页面来隐藏回归，而是仅在显式页面、没有任何固定宽度/位置属性的自动表格路径使用 `min(9000, 正文宽度)`，保留原件为正对照并明确提示规范化。少量既有超宽固定/定位资源上限样本则保留原始显式页面作为新拒绝探针，另有明确缺页规范化变体继续测试旧资源上限；既有具体保护原因未被任意拒绝替代。

下一步继续 DOC/DOCX/PDF：可研究有界现代兼容模式的表格缩进参照与水平单元格边距，减少真实 `table-indent.docx` 的布局拒绝；不能只删除保护检查。更广真实文件、Word/WPS 对照、实际浏览器/原生渲染、中文字体替代、多节和附属内容仍未完成，Excel/PPT 优先级不变。

## 上一轮：单图段落间距、对齐与有限基线字体（2026-10-13）

基于 `7a50b54d10493961dd750e88b7ffc0091e8f99d7` 新增可选 `imageParagraph`，保留受支持单图段落有效间距/对齐和可证明的 Latin 段落样式/图片 run 字体。实际图片选择、数值控件、原子撤销、草稿、另存 DOCX 与可信重开贯通；显示尺寸与段落格式独立重置。完整支持/规范化/拒绝边界见 [IMAGE_PARAGRAPHS.md](IMAGE_PARAGRAPHS.md)。没有扩大为混合行内文本、多图同段、浮动或完整页面布局。

### 最终源码和前端检查

- Rust 1.99 / 1.88 核心各 **287 项通过**（原 256 + 18 个源解析/审计回归 + 13 个模型/导出/草稿/保护回归），日志 `tmp/image-paragraph-core-final2.log`、`tmp/image-paragraph-core-1.88-final2.log`。最终 1.88 使用独立 `tmp/msrv-target`，同一集合不双计
- **141 项无浏览器前端检查通过**（原 123 + 新 18），命令 `tmp/image-paragraph-ui-command.json`，日志 `tmp/image-paragraph-ui-final2.log`。真实 ProseMirror 事务、NodeSelection、单次撤销/重做、过期选区/保存锁、两个独立重置、预算、严格原始 JSON/字体对、DOMParser/DOMSerializer 自有包装、恶意/畸形包装拒绝和不可变打印声明均覆盖；假 DOM 不是实际浏览器
- `npm run build`（`tmp/image-paragraph-build-final2.log`）、rustfmt、Python/JS 语法和 diff 空白检查通过，仍有既有大 bundle 提示。新增 **9 项浏览器规格仅发现/语法检查，未执行**，`tmp/image-paragraph-browser-discovery.log`。没有重试或绕过已确认的 Chromium socket 权限、官方云预览连接/URL 限制、Linux 桌面依赖和 Windows 不可用
- 两工具链原始严格 Clippy 最终仍各仅已知跨格式单项：1.99 的 XLS `chunks_exact_to_as_chunks`、1.88 的 PPT `uninlined_format_args`；日志 `tmp/image-paragraph-clippy-strict2.log`、`tmp/image-paragraph-clippy-1.88-strict2.log`。各只排除原单项后 all-targets 通过，`tmp/image-paragraph-clippy-final2.log`、`tmp/image-paragraph-clippy-1.88-final2.log`。新代码的两项 collapsible_if 已修复，并非扩大豁免；不称完整严格 Clippy 通过
- 早期并行混用同一 target 曾造成测试可执行文件暂不可执行/不同编译器 rlib 冲突，产物保留；改为顺序执行、MSRV 独立 target 后完整重跑通过。第一次旧功能聚合因其全源码哈希捕获这次 lint 修正，正确标为失败；所有最终消费者另用冻结最终 CLI 重跑，不拿旧构建或失败聚合充数
- vendor、Cargo.lock、package-lock.json 未修改，没有声称本轮运行 vendor 或桌面套件

### 已测根因与独立文件/PDF

最终 CLI SHA-256 `b68e823cf193b49d2753c839581f775b4acc15ec6f3386b207fdb2f17ab5bb37`，新套件报告 `tmp/word-image-paragraphs/run-c6ewijsn/report.json`，旧图片套件报告 `tmp/word-image-dimensions/run-69_axbal/report.json`：

- 新独立套件 **21 个正向自制源包、36 个明确拒绝源包、88 个非法模型、2 个合法上下界模型、13 个历史整个 DOCX 字节对照、1 个未经改造上游正对照、46 份 LibreOffice PDF**。21 个正向中 4 个是明确缩进/kern 规范化探针，不计为受支持的绝对坐标保持。源包由 python-docx/Pillow 定向生成，固定预期由独立 XML 样式/关系和 python-docx 核对，绝不说成真实生产文件
- 39 个前置控制变量探针证明：50 pt 图片后的 12.3 pt 差异来自段后 6 pt + 1.5 倍行距额外 6.3 pt；恢复 0/0/240 消除。7.5 pt 图片还需有效段落样式字体名和字号；仅给 drawing run 或段落标记直接字体无效。专用样式修复行框，没有减固定常数、加空格或改变媒体来迎合测量，证据 `tmp/image-paragraph-gap-probe/summary.json`
- 新最终 14 份受控文档、15 个图片框的源 → 未修改导出，图片 x/y/宽高与后续文字基线差均为 **0.0 pt**，断言容差 0.30 pt。覆盖大图、小图、重复图、不同 paragraph/run 字体、样式继承、部分覆盖/零和四种对齐。真实编辑后与独立另造源参考一致：字体/字号编辑使后续基线移动 −13.95 pt，行距编辑移动 −13 pt；不是只检查未修改模型
- 原始 `mwilliamson/mammoth.js/tiny-picture.docx` 按既有 revision/文件 SHA/媒体 SHA/Git blob 不变读取，有效 0/200/276、左对齐、Calibri 11 pt；源和输出的 7.5×7.5 pt 图片框及其位置相同。仅证明该原件的图片探针，不扩大为任意字体替代、Word/WPS 全布局或所有生产文档
- 所有支持源完整可信模型相等、PNG/JPEG 媒体字节/类型及双 extent 保留；文件哈希不变、源/已有输出 no-clobber 和失败模型不发布。缺省基线字体源保留省略并警告；外部重导入导出的默认字体会物化已知值，与 authored-only 的完整模型相等分开记录
- 旧图片尺寸套件完整复测 **13 支持 / 51 拒绝 / 27 非法模型 / 1 原始正对照 / 26 PDF**，显示框/媒体/修改/重置与源文件保护保持通过
- 历史 V1/V2 部件字节和旧 13 份仅尺寸导出的整个 DOCX SHA-256 全部一致；字体样式被修改甚至同步伪造模型哈希也拒绝可信重开。删除段落元数据恢复旧模型路径，不隐式移除尺寸；删除尺寸保留段落设置，layout-only 路线不冒充精确浏览器尺寸
- 最终图片/字体编辑/上游对照 PNG 已实际查看，记录 `tmp/word-image-paragraphs/run-c6ewijsn/visual-inspection.json`。LibreOfficeDev 是独立消费者；本轮没有简文实际 CSS 布局、原生打印、Windows WebView2 或 Word/WPS 视觉等价结论

### 旧功能最终回归

冻结同一最终 CLI 的聚合报告 `tmp/image-paragraphs-final-safety-_myn4ru9/report.json`，源码/脚本/工具/原始文件前后哈希不变：

- 段落间距：19 支持 / 23 拒绝 / 14 PDF
- 固定列宽：11 固定 + 2 自动规范化 / 24 拒绝 / 16 非法模型 / 6 PDF
- 表格位置：32 支持 / 39 拒绝 / 29 非法模型 / 51 PDF，含两个固定原始表格语料
- 三份 POI 原始 DOC/DOCX：两次 DOC 转换完整可信模型、精确全文段落与 1 个手动分页、替换后 13 个未改部件字节、5 项 no-clobber
- 隐藏文字：2 拒绝 + 1 正对照 / 4 PDF；附属故事：6 拒绝 + 1 正对照 / 8 PDF。固定 Tika 原件的既有评论先拒绝仍单列，不能冒充隔离隐藏分支证明

本轮关闭受控单图段落间距及小图 Latin 行框的已测缺口，整体复制目标继续。下一项有意义的边界是源单节页面尺寸/边距：居中/右对齐依赖内容区宽度，目前统一 A4/72 pt 会改变非默认源页面的物理位置；应先用真实原件和独立坐标证据确定有界模型，而不是扩大浮动/混排承诺。优先顺序仍为 DOC/DOCX/PDF → Excel → PPT。

## 上一轮：正文行内图片显示尺寸（2026-10-13）

基于 `1adfef6845041dbe4b2720a02926cdb232274e31` 新增成对整数 EMU 图片尺寸、精确 pt/分数输入、保持/恢复比例、重置、原子撤销、草稿及 DOCX 可信重开。仅支持正文顶层单独行内 PNG/JPEG；完整边界见 [IMAGE_DIMENSIONS.md](IMAGE_DIMENSIONS.md)。尺寸和媒体字节保留不等于段落/全页布局保真。

### 最终源码检查

- Rust 1.99 / 1.88 各 **256 项核心测试通过**（原 225 + 18 项审计 + 12 项模型/导出 + 1 项转换身份防线）。命令 `cargo test --locked` 与 `cargo +1.88.0 test --locked --target-dir tmp/msrv-target`；日志 `tmp/image-core-final.log`、`tmp/image-core-1.88-final.log`
- **123 项无浏览器前端检查通过**（原 97 + 新 26），完整命令 `tmp/word-image-ui-command.json`，结果 `tmp/word-image-ui-final.log`。真实 ProseMirror 选择/事务/历史、预算拒绝、raw JSON/null/半对属性、HTML 元数据、默认字节、比例、锁定、迟到解码与不可变打印快照均覆盖；假的 DOM 控制器不是实际浏览器焦点/排版
- 最终 `npm run build` 通过，日志 `tmp/word-image-build-final.log`，仅既有大 chunk 提示。新增 **11 项浏览器验收规格只完成语法检查和发现**，`tmp/word-image-browser-list.log`，未执行。未重试已确认的浏览器 socket/原生依赖限制，更没有绕过被拒绝的浏览器路线
- 两工具链原始严格 Clippy 各仍只有已知跨格式单项：1.99 的 XLS `chunks_exact_to_as_chunks`，1.88 的 PPT `uninlined_format_args`。原始日志 `tmp/image-clippy-strict.log`、`tmp/image-clippy-1.88-strict.log`；只排除各自单项后 all-targets 通过，日志 `tmp/image-clippy-final.log`、`tmp/image-clippy-1.88-final.log`。本轮新审计曾有同类 MSRV 格式 lint，已修复并重跑原始严格检查，未借豁免隐藏新警告
- 命名 Rust 文件 rustfmt、Python/JS 语法与 diff 空白检查通过。vendor、全部锁文件无修改，没有声称重跑 vendor/桌面套件

### 独立原始文件、导出与物理尺寸

最终 CLI SHA-256 `b70459d92733f7ce4b7b51d17bbf471e3d2a316ef05fd4ff0d1f77193871e3e7`。`scripts/check-word-image-dimensions.py --render --corpus-dir tmp/image-corpus` 最终通过，证据 `tmp/word-image-dimensions/run-a0fe72vj/report.json`：

- **13 个支持的自制源包、51 个明确拒绝源包、27 个非法模型、1 个未改造上游原始正对照、26 份 LibreOffice PDF**；脚本/CLI/源文件运行前后哈希稳定，已有输出和源路径禁止覆盖，失败不发布文件
- 独立 OOXML 和 python-docx 核对双 extent、正确单位、MIME 和媒体原始字节，覆盖 PNG/JPEG、拉伸、非整数 CSS 像素、边界、同图不同尺寸及真实相邻空段。每个支持模型完成未修改/实际改尺寸/重置导出与 `read-docx-model` **完整模型相等**；外部重新转换另记，不冒充可信重开
- 原始 `mammoth.js/tiny-picture.docx` 固定 revision `791a189230977143cde64dbf9f1d24dce96ac8a1`、文件/媒体 SHA-256 和 Git blob。保留原始 95250×95250 EMU PNG；独立原始/输出 PDF 均为 **7.5×7.5 pt**。来源与 BSD-2-Clause/图片权利限制见 `scripts/image-dimensions-corpus.json`；不提交二进制、不把自制样本说成原件
- 受控 PDF 验证 100×50 pt、60×120 pt、同图不同框等物理尺寸，实际修改后 120×80 pt 等结果正确。首次独立比较发现无 `distL` 时 LibreOffice 左边缘偏移约 9 pt；显式尺寸分支加入四边零距离后左边缘恢复 72 pt，旧分支仍保持历史字节
- **保留已知纵向差异**：图片段落属性仍规范化，受控首图上边缘可相同，但每图后文字多约 **12.3 pt** 间距；重复图的第二图下移约 12.3 pt、尾文字下移约 24.6 pt。证据 `paragraph-normalization-evidence.json`。没有将其算作完整位置/排版保持。4 张最终 PDF 栅格预览已查看，记录 `png-previews/inspection.json`
- JPEG APP1（包括 EXIF/XMP/方向 1）、PNG eXIf/APNG、裁剪/旋转/相对与矛盾尺寸、外链/关系/MIME 歧义、混合/表格/列表放置、方向/文字缩放/上下注标继承均明确拒绝。像素头上限 20000/边、4000 万总像素；显式尺寸草稿/事务执行 100 张、8 MiB 单图、20 MiB 合计限制

无新属性时 V1/V2 含 PNG/JPEG 的全部部件哈希与修改前 CLI 固定基线一致，`tests/fixtures/word-image-legacy-parts.json`；旧文字/样式历史字节测试也继续通过。新尺寸分支写入 `wp:extent` 和 `a:ext` 的同一整数对，媒体字节不重编码；模型/外部 XML/同步伪造哈希篡改仍不能直接可信编辑。

### 旧功能回归

同一最终 CLI 重跑并通过：

- 段落间距：19 支持 / 23 拒绝 / 14 PDF，`tmp/word-paragraph-spacing/run-8mwmi12q/report.json`
- 固定列宽：11 固定 + 2 自动规范化 / 24 源拒绝 / 16 非法模型 / 6 PDF，`tmp/word-table-widths/run-21heh2ms/report.json`
- 表格位置：32 支持 / 39 拒绝 / 29 非法模型 / 51 PDF / 2 固定原始语料，`tmp/word-table-position/run-hlqjs48m/report.json`
- 三份 POI 原始 DOC/DOCX：两次 DOC 转换完整可信模型、替换后 13 个未改部件字节、全部段落与 1 个手动分页、5 项 no-clobber；附属故事 6 拒绝 + 1 正对照 / 8 PDF；隐藏文字 2 拒绝 + 1 正对照 / 4 PDF 与固定 Tika 原件。总报告 `tmp/image-dimensions-safety-y4h32tu5/report.json`。第一次临时语料脚本错误要求 ZIP 条目顺序，改为精确部件名集合且保留全部字节比较后重跑通过，失败产物保留；未改产品。Tika 导入仍先拒绝已有评论内容，审计确实识别隐藏风险，不能把它说成隔离证明隐藏分支

所有 LibreOfficeDev 结果仅证明该消费者下的有限物理尺寸、正文/边界探针，不是简文浏览器 CSS、原生打印、Windows WebView2 或 Word/WPS 全布局等价。目标继续进行。下一项优先修复已测出的图片段落间距/对齐模型缺口，同时保持本轮严格尺寸和原件保护；浮动/复杂对象与其他阶段缺口仍未关闭。

## 上一轮：表格对齐与有限左缩进（2026-10-13）

基于 `ce0cc4c5b1d6c5e2cfc8f262c5d4bc98939d4bf0` 新增可选 `tableAlign` / `tableIndentTwips`，保留默认与显式零的区别；覆盖工具栏默认/混合、当前及跨表选区、撤销/草稿、固定/自动表格、DOCX 另存与可信重开。范围和非直观的文字边缘缩进语义见 [TABLE_POSITION.md](TABLE_POSITION.md)。没有添加合并、浮动、双向表格、拖拽列宽或完整 Word 排版。

### 最终源码与前端

- Rust 1.99 / 1.88 全部核心各 **225 项通过**（原 194 + 新 31）；日志 `tmp/position-core-final.log`、`tmp/position-core-1.88-final.log`。同一集合不重复累计
- **97 项无浏览器前端检查通过**（原 75 + 新 22），`tmp/position-ui-final2.log`；完整命令 `tmp/position-ui-command.json`。覆盖实际 ProseMirror/CommandManager 事务、NodeSelection/CellSelection/多表混合、候选资源预算、撤销、HTML/打印快照声明及 TableView 挂载/更新/重置。DOM shim 不是浏览器排版
- 最终 `npm run build`、`cargo fmt --check`、CLI 构建、Python/JS 语法和 `git diff --check` 通过；构建仍有既有大 chunk 提示，`tmp/position-build-final.log`
- 两端原始严格 Clippy 仍各有一个既有跨格式 lint：1.99 的 XLS `chunks_exact_to_as_chunks`、1.88 的 PPT `uninlined_format_args`。仅排除对应单项后 all-targets 通过，`tmp/position-clippy-final.log`、`tmp/position-clippy-1.88-final.log`；不是完整严格 Clippy 通过。新代码/测试 lint 已修复，没有扩大豁免
- 新增 **6 项浏览器规格仅语法/发现检查**，`tmp/position-browser-list.log`；未重试已确认的 Chromium socket EPERM、云浏览器连接预览失败、Linux 桌面依赖缺失和 Windows 不可用。浏览器焦点、真实 CSS 布局、Tauri/原生 PDF 仍未验收
- vendor 与锁文件未修改；没有声称重跑 vendor 套件。一次前端聚合因 USERPROFILE 指向错误导致 5 个既有 Python 路径测试失败，使用已有 `tmp/linux-home` 路径适配后 97 项重跑通过，未改 Excel/PPT 产品代码

审查和独立消费者发现并修复：默认缩进和显式零边框原点不同、现代兼容模式改变缩进参照、显式选中样式链的零缩进与隐式默认样式不同、隐藏在包装节点中的 margin 声明、模糊 settings 关系，以及 TableView 恢复默认后的样式残留和 NodeSelection 类型丢失。受支持设置保留；无法证明的固定几何拒绝；旧自动规范化路线仅在没有直接缩进时保留明确警告。

### 独立文件、PDF 与真实原件

最终报告 `tmp/word-table-position/run-v9xg2k26/report.json`，最终 CLI SHA-256 `b1c44b3e53f39dcb4dd3f5a268200915b1e0459706eaee2420bf70eb604ef137`：

- **32 个支持源文件、39 个明确拒绝源文件、29 个非法模型拒绝**；其中新增 12 个 margin 包装/命名空间/重复/未知属性保护样本。定向文件由 python-docx 自制，期望清单独立预设，不能称为真实生产文件
- **51 个 LibreOffice PDF**：16 个受支持源、32 个未改动/实际位置编辑的输出，以及 3 个拒绝的现代模式源校准。受控页面与边框/文字位置断言通过；默认/显式零/36 pt/居中/右对齐五张 PNG 实际查看，边框与文字完整，`png-previews/inspection.json`
- 同为 A4、四边 72 pt、表宽 300 pt 的探针，旧基线默认左边框约 71.8 pt，显式零 66.6 pt，36 pt 缩进 102.6 pt；现代模式显式零 72.3 pt。这个差异是拒绝未经转换的现代缩进的实证理由，不把复制整数当作位置保真
- 32 个支持源均完成导入 → 未修改/编辑 → 导出 → 独立 XML/python-docx → **完整可信模型相等**；`import-docx` 外部重转换另列，不冒充可信重开。清除新增位置属性还原基线模型和确定性 DOCX 字节；源、脚本、辅助脚本和 CLI 哈希在运行期间不变，源路径/已有输出覆盖均拒绝
- 原始 Apache POI `table-alignment.docx` 六张自动表格保留 absent/left/start/center/right/end 对应的规范对齐及完整段落/单元格文字；明确提示源列宽及无法保真的继承几何规范化。它是**对齐属性正对照，不是绝对宽度/位置保真**
- 未改造的 `table-indent.docx` 七张实际固定表格仍安全拒绝：modern15、自定义 55-twip 边距、不支持单位和负缩进未被错误宣称为支持。原件来源/revision/字节/SHA-256/Git blob 见 `scripts/table-position-corpus.json`；无二进制提交，再分发权与原始作者仍未全面审查

### 旧路径回归

最终同一 CLI 复测固定列宽 **11 支持 + 2 自动规范化 + 24 拒绝 + 16 非法模型 + 6 PDF**，`tmp/word-table-widths/run-q5obrcvy/report.json`；旧“任意正缩进拒绝”样本换为仍不支持的负缩进，新受支持正缩进由独立位置套件覆盖。原始七表拒绝不变。

段落间距 **19 支持 + 23 拒绝 + 14 PDF**、历史 V1/V2 逐部件字节、附属故事保护均通过；独立证据 `tmp/word-paragraph-spacing/run-d6_q0bmb/report.json`、`tmp/side-stories/run-v2hg4wbq/report.json`。隐藏文字定向包、固定 Tika 原件及 LibreOffice 可见性探针通过，`tmp/table-position-safety-3Fd7YzVC/hidden-text/run-wr05b1pq/report.json`。三份固定 POI 原件的 Linux 等价 CLI 检查、两份 DOC 转存/可信模型完全相等、一次 DOCX 文字替换/其余 13 个部件不变，以及独立完整段落/手动分页核对通过，`tmp/table-position-safety-3Fd7YzVC/corpus/report.json` 与 `independent.json`。没有声称执行 PowerShell 或独立 DOC 视觉保真；第三方替换文件没有可信内嵌模型，正确拒绝 authored-only 重开。

目标继续按 DOC/DOCX/PDF 优先推进。下一步应从仍拒绝的真实原件继续研究现代兼容模式下的位置参照和可保留的水平单元格边距，或补齐源页面尺寸/边距的有界模型；不能通过移除拒绝检查把未知版式称为支持。Excel/PPT 优先级不变，真实浏览器/原生打印与完整 Office 复制仍未完成。

## 上一轮：简单矩形表格固定列宽（2026-10-13）

基于 `59e338c020c3c6a7a961df8727f7d88a6846c6a9` 增加非嵌套、未合并矩形表格的有限固定列宽。规范属性为表级整数 `columnWidthsTwips`，各列正数、总和不超过 31680 twip；当前列数值设置、整表恢复自动、结构操作、撤销和草稿贯通。导出 explicit fixed + 一致的 grid/tcW/总宽，可信模型重开不走外部重转换。范围与语料来源见 [TABLE_WIDTHS.md](TABLE_WIDTHS.md)。

### 最终源码检查

- Rust 1.99.0 / 1.88.0 全部核心各 **194 项通过**（原 174 + 6 模型/导出 + 14 导入/审计），日志 `tmp/table-core-final.log`、`tmp/table-core-1.88-final.log`。同一套测试不重复累计
- **75 项无浏览器前端检查通过**（原 52 + 新 23），`tmp/table-ui-final.log`；命令清单保存在 `tmp/table-ui-command.json`。覆盖整数/总和/结构边界、稀疏数组、schema、当前列/单元格选择、首中末列增删、行和整表删除、撤销/重做、非法修改与资源预算、真实 Tiptap CommandManager 的 direct/chain/can，以及 TableView 的 DOM shim 挂载/更新/重置。DOM shim 不是浏览器排版
- `npm run build`、`cargo fmt --check`、CLI 构建、Python/JS 语法和 `git diff --check` 通过；既有大 chunk 提示仍存在。构建日志 `tmp/table-build-final.log`
- 两端核心 Clippy 均只有原有跨格式 warning 在原始严格命令下阻止通过：1.99 的 XLS `chunks_exact_to_as_chunks`、1.88 的 PPT `uninlined_format_args`。分别仅排除该 lint 后全部 targets 通过，`tmp/table-clippy-final.log`、`tmp/table-clippy-1.88-final.log`；不是完整严格 Clippy 通过
- vendor 和锁文件未修改，没有用旧 vendor 计数冒充本轮重跑。新增 6 项完整浏览器规格只做语法/发现，**没有执行浏览器或原生验收**；未重试已确认的 Chromium socket 权限、云浏览器访问预览、Linux 桌面依赖和 Windows 环境限制

审查与针对性测试实际发现并修复：恢复自动后 DOM `<col>` 残留旧宽度、Tiptap 失败命令仍可提交共享事务、粘贴绕过模型资源预算，以及隐藏在包装节点中的重复几何声明。固定单元格未用 overflow 裁掉窄列正文。一次聚合检查 70 项通过、5 项旧 Windows 路径测试因未设置 USERPROFILE 失败；使用既有本地路径适配并顺序构建后，75 项完整重跑通过，未改 Excel/PPT 产品代码。MSRV 新增布尔表达式 lint 已修复后复测。

### 独立文件和 PDF

最终运行：

```text
python scripts/check-word-table-widths.py --cli target/debug/rust-wps --render --corpus tmp/corpus/SampleDoc.docx --table-corpus tmp/table-corpus/table-indent.docx
```

证据 `tmp/word-table-widths/run-bt8cz_b1/report.json`：

- **11 个固定支持样本 + 2 个自动规范化样本 + 24 个明确拒绝样本**；另有 **16 个非法模型拒绝**。定向包由 python-docx 自制，固定预期清单与独立 XML/python-docx 读数核对，不称上游真实生产文件
- 支持样本均完成导入 → 未修改/实际宽度编辑 → 导出 → 独立 grid/tcW/总宽/fixed 核对 → `read-docx-model` **完整模型完全相等**。外部 `import-docx` 几何重转换另列；不将其当可信重开
- **6 个 LibreOfficeDev PDF**：源及未修改输出的列宽分别 90/360 pt 与 210/240 pt，受控左列文本分别 12 行与 4 行；编辑后 109.9/340.1 pt 与 229.9/220.1 pt，分别 6 行与 3 行。边框位置与文字换行断言通过。两张窄列输出 PNG 另经查看；这不是简文打印、Windows WebView2、Word/WPS 版式等价或完整字体/分页验收
- 脚本/CLI 运行前后哈希不变；源哈希不变，源路径/已有输出禁止覆盖，失败模型不发布文件。最终 CLI SHA-256 `a00fe5dc5c637ab3ddaee0e8c4bc9b07486cab7088779cab9853fc0a4608718a`
- 原始固定 Apache POI `SampleDoc.docx` 是**无表格的旧兼容正对照**，正文和可信模型重开通过。新增未改造的 `table-indent.docx` 包含 **7 张 3×4 实际固定表格**，grid/tcW 全为 `[2409,2410,2409,2410]`，其中 6 张有不同的不支持缩进声明；独立核对后明确拒绝编辑并保留源文件。此项是实际语料的安全拒绝证据，不是固定表格可编辑往返证据。来源/revision/字节/SHA-256/Git blob/license 限制见 `scripts/table-width-corpus.json`，没有提交二进制

### 旧行为和边界

无新属性 V1/V2 的逐部件历史字节继续通过；原段落间距 14 项核心包含该固定基线。源表格几何由同一有界快照中实际 OPC main/styles 审计，不从网格单独推断 fixed；转换后核对表格顺序、数量和完整行/列形状。未使用模板样式不误拒绝；活动/默认链上无法证明的布局、未解析引用或冲突命名空间保守拒绝。自动表格不带新数组并明确提示源宽度规范化，表格对齐/边框/单元格格式仍规范化。

本轮重跑段落间距独立脚本的 19 支持/23 拒绝/14 PDF、三份固定原始 POI DOC/DOCX 的等价 Linux CLI + 独立完整段落/手动分页校验，以及附属故事和隐藏文字保护，证据分别为 `tmp/word-paragraph-spacing/run-jn03ix2j/report.json`、`tmp/corpus/tables-h3yhuanr/report.json`、`tmp/side-stories/run-alc8qecp/report.json`、`tmp/hidden-text/run-eu0w4e7_/report.json`。目标继续进行；下一项可针对实际语料扩展表格对齐/缩进，但本轮没有实现它们。仍优先 DOC/DOCX/PDF，真实 UI/原生打印与完整 Office 布局缺口未关闭。


## 上一轮：段前/段后与倍数行距（2026-10-13）

基于 `c435d02d2ce4ba8fbaa3d2590b4d0dcbb2d8a4fa` 增加正文/标题/列表/单元格段落的间距编辑、默认/混合状态、撤销、草稿、DOCX 读写与打印内联样式。修复分量样式继承、列表段落属性和自闭合空段落丢失；外部 DOCX 实体化有效缺省 0/0/240，可信自有模型及旧 DOC 规范化路线不变。范围与保守拒绝见 [PARAGRAPH_SPACING.md](PARAGRAPH_SPACING.md)。

### 最终源码实际结果

- Rust 1.99.0 / 1.88.0 全部核心各 **174 项通过**，含 14 项新间距集成测试；日志 `tmp/spacing-core-final.log`、`tmp/spacing-core-1.88-final.log`。部分旧颜色/字号/导入预期显式补上有效间距，不删除差异来掩盖回归
- vendor 两种 Rust 工具链各 **1,263 项通过**，包含样式分量、列表/嵌套/空项、真实空段与单元格继承回归。1.99 严格 Clippy 通过；1.88 有 21 项既有 `uninlined_format_args`，仅排除该 lint 后通过
- 核心最后一次原始严格 Clippy 两端仍因各 1 项既有跨格式 lint 失败：1.99 的 XLS `chunks_exact_to_as_chunks`，1.88 的 PPT `uninlined_format_args`。分别限定排除该单项后全部 targets 通过；`tmp/spacing-clippy-final.log`、`tmp/spacing-clippy-1.88-final.log`。不是完整严格 Clippy 通过，也没有为此改 Excel/PPT
- `npm run build` 最终通过，保留既有大 chunk 提示；**52 项无浏览器前端/模型测试通过**（原 39 + 新 13），`tmp/spacing-ui-final.log`。新项覆盖全部 2,281 种整数行距、精确单位/HTML 元数据、默认/null、混合/选区/表格 CellSelection、历史分组及修改前资源上限
- 新 6 项浏览器规格已语法检查/发现，尚未执行。没有重复已确认失败的 Chromium socket 启动、缺 glib/GTK/WebKit 的 Tauri 构建或不可用的 Windows 原生检查；前端函数/DOM 样式声明通过不等于实际焦点、CSS 间距或原生 PDF 通过
- CLI 构建、命名核心文件 rustfmt 检查、Python/JS 语法、源文件 diff whitespace 检查通过。没有对 vendor 全目录格式化；两份补丁反向检查通过，128 原始文件哈希核对、119 未改文件一致，锁文件无变化。生成的 unified patch 有标准单空格空白上下文行，普通 `git diff --check` 对该补丁本身报 trailing whitespace；排除此生成补丁后的源文件检查通过，没有破坏补丁来消除提示

### 独立文件与渲染

最终 `python scripts/check-word-paragraph-spacing.py --cli target/debug/rust-wps --render --corpus tmp/corpus/SampleDoc.docx` 通过，证据 `tmp/word-paragraph-spacing/run-928gb_yl/report.json`：

- **19 份支持的定向合成包、23 份拒绝包**，python-docx 1.2.0 与独立 OOXML reader 核对固定预期清单。未经改造的默认模板正对照通过；未使用 contextual 样式/编号不会被误判成使用中。另含继承、before-only/after-only、显式零、边界、标题/列表/单元格、活动编号层级与默认缺省
- 空段样本有 7 段，其中 5 个源 XML 自闭合空段；导入/导出/外部重导入保留数量、位置及继承的 before120/after180/line360。首次测试发现空段丢失，修复后才计通过
- 所有 19 份支持样本及固定 POI 输出经真正的 `read-docx-model` 可信加载器核对**完整模型相等**。`import-docx` 外部重转换单独记录，可物化默认黑色；不把它冒称可信重开
- **14 份 LibreOffice PDF**（7 对源/输出）通过；受控 Latin 三种行距的基线分别 14/21/28 pt，零段间距基线差 14 pt，段前 12 pt、段后 12 pt、两者同时设置均为 26 pt。证明相邻取最大值，不是相加
- 渲染最初捕获输出缺 `word/settings.xml` 时相邻间距相加为 38 pt。隔离实验 `tmp/word-paragraph-spacing/compat-probe-p1ik8s_s/report.json` 证明只加空 settings 即恢复 26 pt；模式 14/15 也相同，故最终只加最小空部件。仅新间距属性路径启用，部件关系/类型/哈希/删除与伪造校验有核心回归
- 源文件和已有输出哈希不变、禁止覆盖、CLI/脚本运行前后哈希稳定。LibreOfficeDev 26.8 开发版及字体替代的限制仍有效；这些 PDF 不是简文打印、Word/WPS 布局等价或 Windows WebView2 证据

V1/V2 无新属性的逐部件字节与保留的 b2fbcae 导出器完全一致；该导出器与当前基线 c435d02 的 `src/document.rs` / 锁文件无差异。证据 `tmp/word-paragraph-spacing/legacy-5laveczc/report.json`；固定哈希清单 `tests/fixtures/word-spacing-legacy-parts.json` 纳入两 profile 回归，旧历史包基线继续通过。

三份固定 Apache POI 原始 DOC/DOCX 用最新 CLI 重跑源 SHA-256、两份 DOC 转存/可信模型重开、DOCX 替换与独立完整段落/1 个手动分页检查通过；`tmp/corpus/spacing-tb_sx9k3/report.json`。这是 Linux 等价 CLI 步骤，不声称执行 PowerShell 或证明独立 DOC 渲染一致。专项间距脚本另外验证原始 POI DOCX 的受支持间距，不能把上游样本和自制样本混为一类。

前两轮保护使用最终 CLI 复测：附属故事 6 份定向包及简单正文对照通过，`tmp/side-stories/run-bw8z43_x/report.json`；隐藏文字定向包与固定 Apache Tika 原件通过，`tmp/hidden-text/run-9t0r4yqd/report.json`。未把任意拒绝当作目标保护分支成功。

目标继续进行。真实 UI、原生打印、完整间距/分节/字体布局与更广真实文件仍待验收；下一步优先文字的简单固定列宽，不扩展 Excel/PPT 本轮声明。

## 上一轮：DOCX 附属内容关系审计（2026-10-13）

基于 `b2fbcae593c49ca6a2fc87aa1a09c65ce68a5dc6` 修复非标准部件名称绕过页眉页脚审计、导入成功后丢失附属文字的问题。按实际 OPC 标准关系及主正文引用拒绝，转换层检查全部六种 IR 页眉页脚字段。范围、过度拒绝与复现见 [SIDE_STORIES.md](SIDE_STORIES.md)。没有增加页眉页脚编辑能力，没有更改 vendor、锁文件、UI 或 Excel/PPT 功能。

### 本轮实际执行

- 独立基线复现 `python scripts/check-side-stories.py --cli tmp/rust-wps-story-baseline-b2fbcae --expect-baseline-loss --render` 通过，证据 `tmp/side-stories/run-3f7kgav1/report.json`：6 份定向 python-docx 包的默认/首页/偶数页页眉和页脚，在 source PDF 指定页可见；旧 CLI 导入、导出及导出 PDF 全部丢失探针
- 对旧 CLI 使用默认拒绝预期会失败（缺少附属内容风险），退出 1；`tmp/side-stories/run-up34ozf4/report.json`。同一检查在修复后通过
- 本轮 CLI 的 `python scripts/check-side-stories.py --cli target/debug/rust-wps --render` 通过，证据 `tmp/side-stories/run-hy0bupx9/report.json`：6 份均因明确附属内容风险拒绝；独立读取/渲染确认原件保留。三页简单正文正对照完成导入、导出与外部重新导入；文字/结构/手动分页一致，重导入可新增已声明默认黑色，不假称模型逐字节一致。源哈希和禁止覆盖检查通过
- Rust 1.99.0 / 1.88.0 全部核心各 **160 项通过**，包含 6 项新增集成测试与 1 项 IR 防线单元测试；同一集合不重复累计。日志 `tmp/side-story-core.log`、`tmp/side-story-core-1.88.log`
- `cargo fmt --check`、CLI 构建、Python 语法和 `git diff --check` 通过
- 核心 Clippy 在分别排除既有 `chunks_exact_to_as_chunks`（1.99）和 `uninlined_format_args`（1.88）的命令行限定检查下通过。1.99 本轮再次遇到同一 XLS 既有 lint；不称完整严格 Clippy 通过。日志 `tmp/side-story-clippy-waiver.log`、`tmp/side-story-clippy-1.88-waiver.log`
- 三份固定 Apache POI DOC/DOCX 语料重跑哈希核验、CLI 提取、两份 DOC 转存、DOCX 替换/重开和独立 python-docx 段落/1 个手动分页比较通过；`tmp/corpus/stories-7d_f8dmg/report.json`。Linux Python 执行等价 CLI 步骤，没有声称执行 PowerShell 脚本或证明独立 DOC 视觉等价
- 上一轮 hidden 脚本用本轮 CLI 重跑三份定向包与固定 Apache Tika 样本，python-docx/LibreOffice→PDF 检查通过；`tmp/hidden-text/run-bmtn24ng/report.json`。这保护隐藏文字分支不回退，不把多重风险文档的任意拒绝当作新分支证明

本轮没有修改 vendor、前端或桌面，因此没有把上一轮 1,255 项 vendor、39 项无浏览器前端检查或构建冒充为本轮重跑。Chromium socket 权限、Linux glib/GTK/WebKit 缺失和无 Windows 原生环境的阻碍未变化，没有重复这些已确认受阻检查；完整浏览器、Tauri 后端和 Windows WebView2 仍未验证。本轮独立 PDF 检查使用 LibreOffice，不是简文原生 PDF 导出，也不是 Word/WPS 排版等价验收。

## 上一轮：DOCX 隐藏文字导入保护（2026-10-13）

从 `f6aa8f6c50c364b753324c9cb5fa254605bd3542` 增加保守隐藏文字审计/导入拒绝和中文说明。8 项新增 Rust 测试覆盖隐藏 run、默认值/样式、模糊 toggle、前缀差异、关系定位的样式、实际主部件诱饵、Content Types 大小写/UTF-16，以及 false-only 的草稿、导出、重开和原件/已有输出保护。未增加隐藏文字编辑，也未修改 vendor、Excel 或 PPT 功能。

### 已实际执行

| 检查 | 命令/方法 | 结果 |
| --- | --- | --- |
| 旧版回归失败 | 在独立基线工作树加入新测试，仅运行 `hidden_direct_runs_are_refused_before_the_ir_can_drop_text` | 预期失败，退出 101，旧审计缺隐藏风险；`tmp/baseline-red.log` |
| 旧版实际丢失 | `python3 scripts/check-hidden-text.py --cli tmp/baseline-rust-wps --expect-baseline-loss --render` | 直接/继承隐藏 run 在旧版导入及导出后丢失，独立 python-docx 确认；`tmp/hidden-text/run-o8w_g5uz/report.json` |
| 新增核心测试 | `cargo test --locked --test word_hidden_text` | 8 通过 |
| 全部核心 | `cargo test --locked`；`cargo +1.88.0 test --locked --target-dir tmp/msrv-target` | Rust 1.99.0 / 1.88.0 各 153 通过；同一测试集，不重复累计 |
| vendor | `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --target-dir target --quiet` | 1,255 通过；同路径 `cargo clippy --lib ... -- -D warnings` 通过 |
| 核心格式 | `cargo fmt --check`（1.99） | 通过；未重排 vendor |
| 新版独立文件 | `python3 scripts/check-hidden-text.py --cli target/debug/rust-wps --render --corpus tmp/hidden-corpus/testWORD_features.docx` | 通过；`tmp/hidden-text/run-2g_ibxa3/report.json` |
| 前端构建 | `npm run build` | TypeScript 与 Vite 通过；既有大 chunk 提示仍存在 |
| 无浏览器前端/模型 | Linux 配置下显式选择无需 page/browser fixture 的 Playwright 用例 | 39/39 通过，含真实 Rust 导出和独立 openpyxl；`tmp/ui-no-browser-linux.log` |
| Python/JS/补丁检查 | `python3 -m py_compile scripts/check-hidden-text.py`、`node --check tests/ui/word-hidden-text.spec.mjs`、`git diff --check` | 通过 |

三个原有 Apache POI DOC/DOCX 样本按清单下载并核对 SHA-256；在 Linux 用 Python 执行与 `check-corpus.ps1` 相同的 CLI 检查、两份 DOC 转存、DOCX 替换/重开和源哈希校验，再运行 `python3 scripts/check-corpus-independent.py tmp/corpus/linux-uzvehcrb` 通过。输出段落数组和 1 个手动分页一致；这不是执行 PowerShell 脚本，也不是独立 DOC 二进制解析/视觉等价证明。

独立 hidden 脚本区分三份 python-docx 定向生成包与固定 Apache Tika 上游测试文档。Tika 样本 8,169 字节，SHA-256 和出处见 `scripts/hidden-text-corpus.json`；另含批注/修订，因此明确断言隐藏风险，不能用任意拒绝代替新分支验证。python-docx 能读到两个隐藏 run，LibreOffice→PDF 不显示它们。输入和已有输出哈希保护检查通过。生成/Tika PDF 的 PNG 已查看；LibreOffice 26.8 开发版有字体缓存不可写提示，但返回 0，文字断言通过。字体、复杂排版及 Word/WPS 一致性不在本项证明范围。

### 失败、受阻与未执行

- 完整严格核心 Clippy **未通过**：Rust 1.99 在既有 `src/spreadsheet_xls.rs:129` 报 `chunks_exact_to_as_chunks`，基线同命令复现相同失败；Rust 1.88 在既有 `src/presentation_import.rs:97` 报 `uninlined_format_args`。未为本项改其他格式。分别用 `-D warnings -A clippy::chunks_exact_to_as_chunks` / `-D warnings -A clippy::uninlined_format_args` 的命令行限定排除后通过；这是带明确排除的检查，不是原始严格检查通过。
- 浏览器套件完整发现 203 项/57 个文件。新增隐藏文字 UI 尝试启动两次（含执行权限升级）都在 Chromium 154 创建 socket 时因 `Operation not permitted` 退出，测试体未执行。39 项无浏览器检查通过，其余 **164 项浏览器相关检查未验证**，不宣称全量 UI 通过。
- Linux 配置仅在忽略的 `tmp/playwright.linux.config.mjs` 中改用系统 Chromium；临时 CLI `.exe`/Python 路径适配用于原有 Windows 硬编码测试。没有把这些适配提交为产品修改，也没有据此声称 Edge/WebView2 覆盖。日志见 `tmp/hidden-text-ui-linux.log`、`tmp/ui-list-linux.log`。
- `cargo test --locked --manifest-path src-tauri/Cargo.toml` 实际尝试，因 `glib-2.0.pc` 缺失而构建失败（退出 101）；环境也没有 GTK/WebKit 开发包。桌面后端测试体、桌面 Clippy、打包和 Windows 原生回归均未执行完成，不能使用下方历史通过结果替代。
- 首次 `npm ci` 因默认 home 缓存不可写失败；改用工作区 `--cache` 后成功安装 72 包，锁文件未改。Rust 工具链也仅安装到工作区。没有安装额外系统库来假装 Windows 原生验收。

完整边界、复现命令和后续必补检查见 [HIDDEN_TEXT.md](HIDDEN_TEXT.md)。目标继续进行。

## 历史：Word 显式字体名与主题字体导入

字体名称作为 `fontFamily{name}` 文字标记保存在草稿与自有 DOCX 模型中；写出的每个显式 run 只有一个 `w:rFonts`，其中 ascii、hAnsi、eastAsia、cs 均为所选字面名称。外部 DOCX 按实际文字脚本拆分并保留不同字体片段，解析直接格式、字符/段落/标题样式及有限主题语言；不能解析的受引用主题字体由 `DOCX_FONT_THEME_UNRESOLVED` 提示。旧 V1 未设置显式字体的编码部件仍按原样输出。

本轮 `cargo test --locked --quiet` 全部通过，新增 9 项字体集成用例覆盖有效/无效名称、草稿和模型篡改、代码文字、四个 OOXML 字体槽、主题语言/缺项警告、真实标题样式及结构上限。`cargo clippy --locked --all-targets -- -D warnings`、前端 `npm run build`、普通桌面后端 10 项测试、vendor 1,255 项测试及严格 Clippy 均通过。vendor 原始 128 文件哈希核对后有 9 文件差异、119 文件一致；两份补丁反向检查通过。

`npx playwright test` 运行 202 项，199 项通过、3 项旧断言失败，耗时 5.7 分钟。失败的共同原因是外部重导入现带有实际字体名，而旧断言仍按无字体名比较；新功能、其他 199 项行为均通过。更新三处断言以明确期望解析出的 Arial / Microsoft YaHei 后，相关三个文件 `npx playwright test tests/ui/word-replace-all.spec.mjs tests/ui/word-scripts.spec.mjs tests/ui/word-strike.spec.mjs --reporter=line` 共 9 项通过。新增 6 项字体 UI 用例覆盖 CSS 字体名粘贴、局部/混合/光标/代码文字、保存锁、撤销、结构上限，以及真实 CLI 导出和独立 python-docx 读取。上述两次运行共同覆盖 202 项，未声称修改断言后又执行一次全量。

三份固定 DOC/DOCX 语料的 `scripts/check-corpus.ps1` 和独立 python-docx 核对通过，目录 `tmp/corpus/7ed5c405-f2be-4cfe-b933-16f499428a53/`；配对非空正文、段落结构及 1 个手动分页一致，源件不变。该核对不包含 DOC 独立二进制解析或视觉排版比较。

真实 WebView2：`npx tauri build --no-bundle --debug --features native-smoke` 后运行 `node scripts/native-word-font-family-smoke.mjs` 通过，证据 `tmp/native-word-font-family/run-PsZcVu/`。两段实际键盘输入分别为普通/代码文字，均设置 Courier New、13.5 pt、RGB `1A6B3C`；撤销/重做、V2 磁盘草稿与 worker 重开保留标记。独立 python-docx 在两段 run 中各读到单一 `rFonts` 的四槽 Courier New 和 `szCs=27`；原生 PDF 的 BaseFont 为已安装的 CourierNewPSMT 子集，pypdf 实测两段约 13.494 pt 和目标 RGB（1/255 容差内）。打印未改草稿，源 DOCX 哈希不变；编辑器截图已检查。此证据仅覆盖两段水平 Latin 字形，不覆盖缺字替代、CJK 字形、完整塑形和 Word/WPS 分页等价。

已补充 [dot 接手说明](DOT_HANDOFF.md)。目标继续进行，字体替代/主题动态关联和 DOC/PDF、Excel、PPT 路线图缺口均未关闭。

## 历史：Word 版本化默认样式

新建文档及外部 DOC/DOCX 编辑副本使用 `standard-v2`。正文声明 Arial / Microsoft YaHei、11 pt、黑色和 1.5 倍行距；三级标题为 18/15/13 pt、粗体，段后 6 pt、标题段前 12 pt，A4 四边 25.4 mm。旧模型省略 profile，继续使用原 V1 编码与显示规则；旧缺省样式的历史差异没有通过静默迁移消除。

根节点 `doc.attrs.styleProfile` 随编辑、撤销、草稿、DOCX 和打印克隆保留。草稿仍使用原文件名，内部版本为 1 或 2；嵌入模型部件名、版本和 profile 必须一致。未知/null profile、双模型、删部件后伪造哈希及额外包部件明确拒绝。旧 fixture 再编码逐部件字节相同。

本轮核心 `cargo test --locked --quiet` 共 136 项、普通桌面后端共 10 项通过。核心与桌面严格 Clippy 全 targets（桌面含 all features）、核心 fmt 通过。8 项新增核心用例验证版本协议、默认样式、常规/复杂文字属性、草稿及包一致性；5 项新增 UI 用例验证真实 CLI/独立 python-docx 输出、重开、草稿队列、打印快照与实际 PDF 字号/边距、非法版本不替换当前文档和外部导入。

`npx playwright test` 全量运行 196 项，194 项通过，2 项失败，耗时 6.2 分钟。失败均是上下标/删除线旧用例对外部重导入模型的精确期望没有包含 V2 已声明的黑色；实际差异只有每个文字节点新增 `textColor=000000`。自有包直接重开仍保留“无显式颜色”的原模型，外部重新导入则解析已声明的默认颜色，两条路径有意不同。此次全量包含最后的 code 字体 Arial 回退补充，前端资源在测试期间没有重建。

核对失败产物后，仅更新这两处精确期望，并新增独立 python-docx 默认黑色断言；原上下标/删除线与草稿无显式颜色断言保留，产品代码未因此修改。随后 `npx playwright test tests/ui/word-scripts.spec.mjs tests/ui/word-strike.spec.mjs tests/ui/word-style-profile.spec.mjs` 共 10 项通过，25.4 秒，覆盖两项原失败及全部 5 项 V2 专项。其余 186 项沿用本轮全量的成功结果，没有把定向复测描述为又一次全量运行。

固定真实 DOC/DOCX 语料及独立 `scripts/check-corpus-independent.py` 通过，目录 `tmp/corpus/d1aac369-abed-43a9-9644-32013ea2e7a2/`。独立 python-docx 核对配对段落数组、替换正文与 1 个手动分页，源文件不变。当前轮没有修改 vendor，1,255 项解析库结果沿用下方历史记录，未重复计为本轮运行。

原生检查先以 `npx tauri build --no-bundle --debug --features native-smoke` 构建：

- `node scripts/native-word-style-smoke.mjs` 通过，目录 `tmp/native-word-style/run-3w1Rep/`。真实键盘/标题菜单输入、V2 磁盘草稿、CLI DOCX 导出及 worker 重开通过；独立 python-docx 检查默认字体、sz/szCs、三级标题和四边 25.4 mm。原生 PDF 为 1 页 A4，pypdf 实测正文/标题约 10.990/17.991/14.993/12.991 pt，黑色 Arial / Arial Bold，另有 Microsoft YaHei 中文字体资源，正文横坐标约 72 pt。保留原始 PDF 矩阵和字体证据，草稿及源 DOCX 哈希不变，编辑器截图已检查。首次脚本错误地假设末尾标题后没有空段；核对 Tiptap 默认 TrailingNode 后，改为严格允许最多一个末尾空段，四个非空探针及物理属性检查不变。
- `node scripts/native-word-font-size-smoke.mjs` 通过，目录 `tmp/native-word-size/run-mngOV5/`。显式 13.5 pt / `szCs=27` / RGB `1A6B3C` 经草稿和独立 DOCX 读取保留；PDF 实测约 13.4936 pt、填充 `[0.102,0.4196,0.2353]`，撤销/重做及打印不改草稿通过。
- `node scripts/native-doc-pages-smoke.mjs` 通过，目录 `tmp/native-doc-pages/run-rw3uX6/`。真实 DOC worker/导入保留 1 个手动分页，PDF 为两页 A4，各页正文标记分离，原件及隔离输入哈希不变。

补齐 code 的 Arial 字体回退链后，重新构建 native-smoke 并再次运行默认样式探针通过，最终证据目录为 `tmp/native-word-style/run-qR7But/`；字号、黑色、Arial、72 pt 横坐标、V2 worker 重开和源文件不变均继续通过。上述显式字号/颜色与 DOC 分页两项使用补充前的嵌入前端执行；差异仅为 CSS code 回退链，不涉及其受测文字。

这些检查证明受测默认样式声明、水平 Latin 字体/字号、黑色和左边距的一致性；DOCX 通过 CLI 导出实际磁盘草稿，不覆盖系统另存对话框。浏览器与 Word 使用不同排版引擎，字体回退、行高计算、中文/复杂文字塑形、自动分页及复杂版式尚不保证相同。

原生验证结束后，`npx tauri build --no-bundle --debug` 成功，已恢复不含 native-smoke 的普通桌面程序。前端保留既有的大 chunk 构建提示，本轮不据此声称启动性能或完整 Office 兼容达标；Goal 继续进行。

## 历史：Word RGB 颜色、主题解析与包完整性

RGB 颜色贯通选区/后续输入、色盘/HEX/黑色色块、草稿、撤销、清除和 DOCX 导入导出。显式颜色优先于链接默认蓝色；表格、代码和原有字号共存，拆分超预算时在修改前拒绝。主题色按 Word HSL 规则近似转为固定 RGB，标题颜色继承、缺主题时的最终备用色及严格颜色词法有回归；自动/未解析颜色明确提示，自定义主题映射与受引用的复杂主题颜色方案拒绝编辑导入。

本轮实际执行：

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 跨格式界面全量 | `npx playwright test` | 191 项通过，5.3 分钟 |
| 包完整性修复后的文字界面复测 | `npx playwright test tests/ui/authoring.spec.mjs tests/ui/word-font-size.spec.mjs tests/ui/word-text-color.spec.mjs` | 18 项通过，43.0 秒 |
| 最终 Rust 核心 | `cargo test --locked --quiet` | 128 项通过 |
| 本地解析库 | `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet` | 1,255 项通过 |
| 最终普通桌面后端 | `cargo test --manifest-path src-tauri/Cargo.toml --locked --quiet` | 9 项通过 |
| 静态检查 | 核心/桌面严格 Clippy 全 targets，解析库严格 Clippy lib；核心 fmt | 全部通过 |

191 项全量执行后，审查发现旧自有包加载器只逐项检查实际存在的部件，没有反向检查完整集合。新增用例先复现仅删 styles、仅删图片媒体、同时删两者并同步删哈希记录的三种误接受，再补齐 expected/actual 完整部件键集合相等。修复后上述 18 项界面和完整核心/桌面检查通过，编码字节规则未改；历史无字号包的部件字节兼容测试仍通过。解析库 1,255 项执行于此末尾根加载器修复之前，此后 vendor 未改。前端资源在每次界面测试期间保持固定。

新增 9 项颜色 Rust 集成测试覆盖 RGB/非法值/草稿与包篡改、链接优先级、标题完整颜色继承、24 组主题与备用色的有/无主题分支、12 种非默认映射拒绝、默认映射放行及 clrScheme/fmtScheme 变换区别。HSL 预期由独立 Python colorsys 固定计算；微软 shade 示例的一个色阶量化差异明确记录于 EXPLORATION.md，不宣称 Word 像素一致。5 项颜色 UI 测试包含真实 CLI 导出与独立 python-docx 逐 run 读取、色盘/黑色色块、局部/混合/光标、保存锁、粘贴及结构预算。

三份固定 DOC/DOCX 真实语料经 `scripts/check-corpus.ps1` 与独立 `check-corpus-independent.py` 通过，目录 `tmp/corpus/cc0ce5bb-4061-45ef-9358-c679728c075d/`：非空正文、完整段落数组及 1 个显式分页匹配；原件保持不变。此语料回归早于末尾加载器集合检查，后续定向文字回归覆盖正常自有包重开。vendor 原始 128 文件哈希匹配，本地仅记录的 8 文件变化，其余 120 文件不变，两份补丁反向检查通过。

最终原生 WebView2 检查在包完整性修复后执行，先以 `npx tauri build --no-bundle --debug --features native-smoke` 构建：

- `node scripts/native-word-font-size-smoke.mjs` 通过，证据 `tmp/native-word-size/run-zLsBb3/report.json`。实际 IPC 草稿记录半点 27、RGB `1A6B3C`；颜色撤销/重做保留字号和文字。从实际草稿经 Rust CLI 生成 DOCX，独立 python-docx 读取 13.5 pt、`szCs=27`、RGB `1A6B3C`，每 run 只有一个颜色节点。原生 PDF 共 1 页，pypdf 在水平 Latin 探针测得约 13.4936 pt，填充 RGB 为 `[0.102, 0.4196, 0.2353]`，与目标 `[26,107,60]/255` 在 1/255 容差内；打印前后草稿字节不变。原生编辑窗口截图已检查。
- `node scripts/native-doc-pages-smoke.mjs` 通过，证据 `tmp/native-doc-pages/run-gaEH4k/report.json`。真实 DOC worker、编辑器和磁盘草稿均保留 1 个分页，独立 pypdf 核对两页 A4 及每页正文标记，原件及隔离输入哈希保持不变。

原生 DOCX 验证使用 CLI 导出实际草稿，不覆盖系统另存对话框；PDF 探针只证明受测水平文字的物理字号和纯 RGB 填充，不证明完整颜色管理、所有脚本塑形、字体替代或 Word 版式一致。默认字体/字号/颜色的编辑、打印和 DOCX 差异仍待版本化统一，链接下划线颜色仍可能不同。

原生验证结束后，`npx tauri build --no-bundle --debug` 成功，恢复普通桌面可执行文件。前端仍有既有较大 chunk 提示，未把构建通过当作加载性能验收；Goal 继续进行。

## 历史：Word 半点字号、默认样式与标题字号

本轮字号贯通 UI → 草稿 → Rust DOCX → 独立读取，支持 1–1638 pt、0.5 pt 精度。修复全选后立即局部选区的同步、输入转焦提交、代码格式共存和修改前保存预算；粘贴不把字号留在硬换行等非文字节点上。解析库补齐默认段落样式、标题有效字号，以及常规/复杂文字字号单位的精确换算。上游来源、七文件修改和两份独立补丁见 [vendor/README.md](../vendor/README.md)。

最终源码的实际检查：

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| 跨格式全部界面 | `npx playwright test` | 186 项通过，5.8 分钟 |
| Rust 核心 | `cargo test --locked --quiet` | 118 项通过 |
| 本地解析库 | `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet` | 1,248 项通过 |
| 普通桌面后端 | `cargo test --manifest-path src-tauri/Cargo.toml --locked --quiet` | 9 项通过 |
| 静态检查 | 核心/桌面 `cargo clippy --all-targets -- -D warnings`，解析库 `--lib` | 全部通过；核心 `cargo fmt --check` 通过 |

字号的 7 项 UI 用例包括局部/混合选区、半点、后续输入、清除与撤销、锁定/非法值、粘贴软换行、上下标/代码/表格、9,999 节点的拆分拒绝；真实 Rust 输出经 python-docx 检查各 run 的 pt 和 `szCs`。7 项 Rust 专项包含半点边界、JSON 属性拒绝、草稿/包篡改、默认样式/标题/复杂文字继承、带单位字号及旧版自有 DOCX 包部件字节兼容。界面全量运行期间未重建前端资源。

固定三份真实语料复测与独立 python-docx 检查通过，成功目录 `tmp/corpus/37c0961f-441f-41e1-8c16-5164a63d3e7f/`。DOC 转存段落数组和 1 个显式分页与配对 DOCX 一致，替换结果也完全匹配。首次以 Windows PowerShell 5 执行时，UTF-8 无 BOM 的脚本把中文替换参数误读，独立检查成功捕获；脚本改为 UTF-8 BOM 后重跑通过，失败目录 `03038790-3227-4570-b1fe-a2f28502fe7e/` 不计成功。

首次字号选区测试及随后标题字号集成测试曾暴露实际问题，修复后才执行以上全量结果。支持字号不等于完整 Word 排版：缺省编辑/打印与 DOCX 样式尚未统一，字体名称/颜色、分节布局、完整样式、真实输入法及系统剪贴板仍待实现或验收。

原生 WebView2 验证：

- `node scripts/native-word-font-size-smoke.mjs` 通过，证据 `tmp/native-word-size/run-wWdSvj/report.json`。真实 Tauri IPC 写入草稿半点值 27，撤销/重做保留文字；从该草稿经 Rust CLI 导出，python-docx 读取字号 13.5 pt、`szCs=27`。真实 WebView2 PDF 的水平 Latin 探针经 pypdf 文本/页面矩阵计算为约 13.4936 pt（允许 0.15 pt 误差），共 1 页；打印前后草稿字节不变。这条 DOCX 路径使用 CLI，未覆盖系统另存对话框。
- `node scripts/native-doc-pages-smoke.mjs` 通过，证据 `tmp/native-doc-pages/run-jW7Oo9/report.json`。真实 DOC worker/导入、磁盘草稿均保留 1 个分页；PDF 为两页 A4，页一/页二标记分开，原件及隔离输入哈希不变。

原生脚本先使用 `npx tauri build --no-bundle --debug --features native-smoke` 构建。字号探针证明受测文字的物理字号，不证明所有字体替代、复杂文字塑形或完整 Word 视觉版式。

验证后 `npx tauri build --no-bundle --debug` 成功，已恢复不含测试 feature 的普通桌面可执行文件。前端仍有既有较大 chunk 提示，未据此宣称启动/加载性能达标。Goal 保持进行中。

## 历史：DOC 正文手动分页恢复

固定 `office_oxide 0.1.13` 的本地源码补丁保留分页控制信息，区分手动分页和分节终止，不从空行推断。来源、128 个上游文件的原始哈希及四文件差异见 [vendor/README.md](../vendor/README.md)。同时修复分页旁真实空段落丢失，并对只恢复部分结构化正文的文件拒绝编辑转换。

本轮实际结果：

- `cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet`：最终 1,235 项通过。
- `cargo test --locked`：最终 111 项通过，包含两个自制二进制 DOC 的完整导入回归；原文件字节不变，正常样本转存 DOCX 重读保持空段与分页，不完整样本明确拒绝。
- 普通桌面 `cargo test --manifest-path src-tauri/Cargo.toml --locked`：9 项通过；核心与桌面 `cargo clippy --all-targets -- -D warnings`（桌面指定 manifest）均通过。
- `npx playwright test`：179 项通过，5.4 分钟。此全量运行早于末尾正文完整性保护和空段保留两项补充；补充后定向重跑 DOC/DOCX 导入与打印，共 9 项通过（25.9 秒）。
- 固定真实语料 `scripts/check-corpus.ps1` 通过，输出目录 `tmp/corpus/dcbaf5de-5069-4b86-a2c6-c3575608c69f/`。独立 `check-corpus-independent.py` 验证配对 DOC 转存结果与参考 DOCX 的段落文字数组一致，且均有 1 个显式分页；DOCX 替换检查继续通过。
- 真实 WebView2 `node scripts/native-doc-pages-smoke.mjs` 通过，证据 `tmp/native-doc-pages/run-2iEb9t/report.json`。编辑器与磁盘草稿均有 1 个分页，pypdf 独立读取两页 A4，第一页含 `This is page 1`、第二页含 `This is page two`，互不串页；原件与隔离输入哈希不变。首次脚本运行在打开后的等待步骤超时，未计成功；补充 DOMContentLoaded 等待与失败诊断后重跑通过。

复现自制输入：`python scripts/make-doc-pagination-fixtures.py --check`。真实语料独立检查：`python scripts/check-corpus-independent.py tmp/corpus/dcbaf5de-5069-4b86-a2c6-c3575608c69f`。原生验证需要先用 `npx tauri build --no-bundle --debug --features native-smoke` 构建，结束后恢复普通构建。

新原生脚本再次运行也通过，目录 `tmp/native-doc-pages/run-Ot54zS/`。以上证明受支持正文的显式分页恢复及样本的逐页文字，不证明完整 Word 视觉版式。分节、自动分页、复杂表格/列表内分页、更多中文 DOC 与真实输入法仍待实现或验收；Goal 继续进行。

验证结束后，`npx tauri build --no-bundle --debug` 成功，已恢复不含 native-smoke 的普通桌面构建。前端构建仍有既有的较大 JS chunk 提示，未当作加载性能达标的证据。

## 历史：查找与替换

全部替换加入后：19 项 Word 相关回归通过（49.5 秒）；补充草稿/节点预算用例后，4 项批量替换用例复测通过（17.4 秒）。真实 WebView2 批量替换正文和表格、单步撤销、磁盘草稿、Rust DOCX 导出及独立 python-docx 读取通过，产物 `tmp/native-pdf/run-1pvUV9/`；已有单处替换、A4 PDF、拒绝覆盖及损坏输入保护继续通过。未重跑下面 173 项全量基线，未据此扩展 DOC 版式兼容声明。

当前匹配替换加入后：16 项 Word 相关界面测试通过，覆盖替换/删除、首字符样式和链接、撤销分组、草稿、无效输入与已有编辑/退出流程；真实 WebView2 替换、磁盘草稿、Rust 导出和独立 python-docx 验证通过，产物 `tmp/native-pdf/run-sTsvMt/`。本轮未重跑下面的 173 项历史全量基线。

补齐 U+FFFE/U+FFFF 的修改前拒绝后，4 项查找/替换用例复测通过（24.4 秒），包含无效输入不改正文、草稿及历史。原生证据早于这项校验补充。

## 历史：Word 正文查找加入后的完整回归

随后增加了 DOC 分页丢失的明确导入提示：9 项 Rust 导入测试、3 项导入界面测试、核心严格 Clippy 与前端构建通过。下面 173 项记录早于这一提示修改，不作为修改后重新执行的全量结果。

实际执行 `npx playwright test`：173 项通过，4.7 分钟，退出码 0。包含删除线、上/下标、清除字符格式、结构按键选区同步及正文查找；同时覆盖原有 PDF、Excel、PPT 流程。运行期间未重建前端资源。

`cargo test` 全部通过；`cargo test --manifest-path src-tauri/Cargo.toml` 9 项通过；两端 `cargo clippy --all-targets -- -D warnings`（桌面端加对应 manifest 参数）均通过。以上均来自本轮实际命令退出码 0，不复用历史结果。界面仍主要使用模拟 Tauri 桥，不能替代原生输入法或系统对话框验收。

`scripts/check-corpus.ps1` 三份固定样本复验通过，输出目录 `tmp/corpus/66f3a4bb-f051-4ea9-8303-17319d96a517/`。新增独立 python-docx 检查可复现：

```text
python scripts/check-corpus-independent.py tmp/corpus/66f3a4bb-f051-4ea9-8303-17319d96a517
```

独立读取证明配对 DOC 转换的非空段落正文一致，DOCX 替换后的全部段落文字及显式分页一致；同时发现 DOC 转换结构不一致：参考 DOCX 有 1 个显式分页，DOC 转换输出为 0，边界出现空段落。该脚本明确报告差异，不将非空正文一致解释为版式兼容；也不提供独立 DOC 二进制解析或渲染验证。

本轮没有重跑原生冒烟；最近 Word 原生查找、草稿字节不变及 A4 PDF 证据仍为 `tmp/native-pdf/run-p6AYUU/`。目标继续进行，复杂排版与原生输入、PDF 页面操作、Excel 筛选/样式、PPT 基本形状等缺口仍有效。

## 历史：164 项基线与后续定向验证

此处 164 项全量基线在新增文字删除线及上下标按钮之前执行，不作为该新增之后代码的全量证明。新增之后：18 项既有文字/打印/退出相关测试通过，新删除线用例及独立 python-docx 复测通过，原生 WebView 选区/草稿与 A4 PDF 脚本通过，详见 EXPLORATION.md。

本记录在加入 Excel 自动重算、工作簿键盘输入/查找，以及 PPT 画布选择、文本/图片拖拽、中断过滤、比例恢复、方向键微调和右下角尺寸手柄之后执行。它替代先前 140 项界面回归作为当前源码的全量记录；不表示完整 Office 兼容或整个开发目标完成。

| 检查 | 实际命令 | 本次结果 |
| --- | --- | --- |
| 全量界面/前端模块 | `npx playwright test` | 164 项通过，4.4 分钟 |
| Rust 文档核心 | `cargo test` | 全部通过 |
| 普通桌面后端 | `cargo test --manifest-path src-tauri/Cargo.toml` | 9 项通过 |
| 核心静态检查 | `cargo clippy --all-targets -- -D warnings` | 通过 |
| 普通桌面静态检查 | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | 通过 |

界面测试使用最近一次普通 Debug 桌面构建中的同版前端资源，运行期间没有重建 dist。`tmp/ui-tests/.last-run.json` 当前为 passed 且 failedTests 为空；该目录会被后续运行更新。核心结果来自实际命令退出状态和各组测试输出，不引用历史 `tmp/core-regression.log`。

覆盖文字编辑、表格、段落对齐、链接、手动分页、草稿/退出失败保护、DOC/DOCX 基础导入、PDF 本机密码/搜索/渲染/打印快照；XLS/XLSX 稀疏读取、编辑类型、日期系统、数字格式、复制粘贴、有限计算及自动调度、排序、查找、工作表操作和导入；PPT/PPTX 阅读、备注/母版、基础导入、文字替换、图片、页面/文本框复制、层次、隐藏、历史/草稿、播放、换行/对齐、尺寸切换、画布移动与微调、对象尺寸及近似 PDF。

界面按需要调用真实 Rust CLI 和独立 python-pptx/openpyxl 消费者，部分用例真实写入并重读 Office/PDF 文件；不是每个测试都使用真实文件。界面的 Tauri IPC、草稿运输层、原生关闭及文件对话框多数由测试桥模拟，页面由真实 Edge 执行。后端测试另验证 worker 协议/截止/响应限制、Windows Job 内存限制及 PDF 原子发布。

本轮未重跑原生 WebView 冒烟；最近一轮原生文本/图片尺寸、键盘、拖拽、磁盘草稿、PPTX 和六页 PDF 证据位于 `tmp/native-pdf/presentation-YZby9O/`，方法和边界见 [PRESENTATIONS.md](PRESENTATIONS.md)。Word/Excel 原生证据见各格式文档。上述证据不能替代真实中文输入法候选、系统剪贴板/对话框、复杂 Office 视觉比较、发布性能和跨平台验收。

阶段仍未完成：文字/PDF 需更多真实语料与复杂排版/原生交互证据；Excel 缺少筛选、字体边框等样式、完整依赖重算及 XLS 编辑转换；PPT 缺少基本形状、混合对象层次和复杂模板/对象兼容。当前能力矩阵与原阶段要求见 [ROADMAP.md](ROADMAP.md)。


上下标与结构按键选区同步加入后：19 项相关 Rust 测试、24 项界面回归及两个跨段合并补测通过，核心严格 Clippy 通过。没有重跑上述 164 项全量基线，不能将该历史结果当作当前修改后的全量证明。


修复后原生 Word 脚本无诊断延迟连续两次通过（上/下标互斥、撤销、磁盘草稿、快速分段和输入、A4 PDF、拒绝覆盖与失败保护）；运行产物见 EXPLORATION.md。


清除字符格式加入后，18 项相关界面回归及两个清除用例复测通过；上面的全量基线仍早于该修改。


编辑正文查找加入后：20 项相关文字/退出回归通过；补充扫描节点上限后两个查找用例复测通过。此前全量基线不包含该功能。
