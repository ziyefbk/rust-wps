# DOCX 隐藏文字导入保护

## 当前状态

2026-10-13 基于 `main` 的 `f6aa8f6c50c364b753324c9cb5fa254605bd3542` 实现。旧版实际丢失及新增失败测试均复现；修复后 Rust 1.99/1.88 各 153 项核心、1,255 项 vendor、前端构建、39 项无浏览器前端检查和独立文件检查通过。完整严格 Clippy 有既有跨格式告警，浏览器/桌面原生检查受环境限制；详见 [REGRESSION.md](REGRESSION.md)，不宣称全部测试通过。

## 为什么拒绝编辑副本

Microsoft 的 [Vanish 说明](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.vanish?view=openxml-3.0.1) 定义 `w:vanish` 为可继承的隐藏文字属性。文字仍在 DOCX 中，普通显示可以隐藏它，应用也可以选择显示隐藏文字。

当前 `office_oxide` 的 `convert_docx.rs` 在有效 `hidden=true` 时不输出 run；应用此前审计未检查此属性。旧版 CLI 的实际导入/导出和 python-docx 独立读取已证实受测隐藏原文丢失。新逻辑在转换前增加 `Hidden text cannot be preserved in an editable copy` 风险并拒绝编辑导入，不把缺失内容当作可接受的版式变化。基线测试在旧版本预期失败；同一测试在修复后通过。

原文件只读，不改源包、草稿、导出机制或 vendor。只读提取仍可能省略隐藏文字；拒绝说明会明确这一点。此修复不提供隐藏文字的显示、编辑、恢复或 Office 完整兼容。

## 保守支持边界

- 扫描 `word/*.xml` 中的 `vanish` 声明，以及通过实际 OPC 关系解析得到的样式部件。样式可在其他目录，或使用非 `.xml` 扩展名
- 使用与转换器相同的 OPC 主部件定位；实际主部件不是 `/word/document.xml` 时拒绝，不能通过放置同名空白诱饵绕过审计
- 缺省值、`true`、`1`、`on`、无效/模糊值均拦截；直接属性、字符/段落/表格样式、文档默认值都在检查范围内
- 唯一一个字面 `w:val` 为 `false`、`0` 或 `off` 时，该声明本身不触发风险。文档内其他启用声明仍触发拒绝，不能把单个 false 覆盖当作全文件无隐藏文字证明
- 当前 vendor 按字面 `w:val` 读取属性。其他前缀的 `x:val="false"` 或未加前缀的 `val="false"` 不能可靠传递 false，因此保守拒绝；不能只按属性 local name 放行。冲突/重复属性拒绝或解析失败
- 不计算声明是否实际使用：未引用的隐藏样式、已被所有 run 显式取消的隐藏默认值仍会阻止编辑。这是有意的过度拒绝，后续有效样式审计或无损隐藏文字模型可以收窄
- `semiHidden`/`unhideWhenUsed` 是样式列表可见性，不作为隐藏 run 拦截。[`webHidden`](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.webhidden?view=openxml-3.0.1) 是网页显示专用属性，vendor 当前不将其当作 `hidden` 丢弃文字；本轮不新增网页视图语义支持，也不将其误报为本项已支持的隐藏文字
- 这不是完整的 DOCX 内容安全审计。页眉页脚、域、修订、浮动对象及关系目标等既有兼容范围仍有限；源文件完整保存不等于编辑副本完整保真

## 测试与独立样本

`tests/word_hidden_text.rs` 的 8 项测试已通过：IR 丢失机制、14 种 enabled/ambiguous/前缀组合和重复属性、默认值/四类样式情况在三种目标位置、false-only 草稿/导出/重开/禁止覆盖、非文字隐藏标记、主部件诱饵、无 Content Types 包拒绝，以及大小写变体/UTF-16 主部件检查。核心完整回归包含这 8 项。

`tests/ui/word-hidden-text.spec.mjs` 编写了真实 CLI 拒绝 + 模拟 IPC 的界面测试，预期验证：两次失败均保留只读正文和标题、不写旧草稿，随后仍能恢复原草稿，源字节不变。本轮 Chromium 因 socket 权限在启动时退出，测试体未执行；它也不是原生 WebView2 测试。

`scripts/check-hidden-text.py` 使用独立 python-docx 生成三份真实 ZIP/OOXML 包，分别为直接隐藏、字符样式继承隐藏、显式取消隐藏。它们是**定向生成样本，不是真实世界语料**。python-docx 独立读取源 run 与隐藏属性，LibreOffice 渲染为 PDF 后用 pypdf 核对探针显示/隐藏；CLI 可用时继续验证审计、导入、导出、原件与已有输出保护。

额外固定上游样本来自 [Apache Tika 引入提交](https://github.com/apache/tika/commit/6f6e6d389007c682cde51a95714e725895dd0c6e)：[testWORD_features.docx](https://github.com/apache/tika/blob/6f6e6d389007c682cde51a95714e725895dd0c6e/tika-parsers/tika-parsers-standard/tika-parsers-standard-modules/tika-parser-microsoft-module/src/test/resources/test-documents/testWORD_features.docx)。固定字节数、SHA-256、隐藏 run 和版权限制见 `scripts/hidden-text-corpus.json`。仅在本地下载使用，不提交二进制；仓库许可证为 Apache-2.0 及第三方声明，未独立核实该单份样本的再分发权。

该上游测试文档由 LibreOffice 生成，有普通正文、两个隐藏 run、批注与修订。它不是用户生产文档，也不能单独证明新拒绝分支，因为原版可能已经因其他不支持内容拒绝。测试须明确检查隐藏风险项。任何渲染结论只限受测文字，不代表复杂版式或 Word/WPS 等价。

## 复现与待补检查

从上述固定链接下载到 `tmp/hidden-corpus/testWORD_features.docx`，脚本会核验清单中的哈希与长度，不修改它。仅独立读取/渲染（本轮已执行）：

```text
python scripts/check-hidden-text.py --render --corpus tmp/hidden-corpus/testWORD_features.docx
python -m py_compile scripts/check-hidden-text.py
node --check tests/ui/word-hidden-text.spec.mjs
```

本轮已在单独的基线工作树编译 `f6aa8f6c50c364b753324c9cb5fa254605bd3542`，用新脚本的 baseline 模式检查可见文字保留而隐藏文字丢失。再次复现时仍使用独立工作树。不要修改当前源码来伪造基线：

```text
python scripts/check-hidden-text.py --cli <基线CLI路径> --expect-baseline-loss --render
```

当前改动的复现检查（逐项实际状态见 REGRESSION；Windows CLI 加 `.exe`）：

```text
cargo fmt --check
cargo test --locked --test word_hidden_text
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo test --manifest-path vendor/office_oxide/Cargo.toml --lib --quiet
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo build --locked
python scripts/check-hidden-text.py --cli target/debug/rust-wps --render --corpus tmp/hidden-corpus/testWORD_features.docx
npm ci
npm run build
npx playwright test tests/ui/word-hidden-text.spec.mjs
npx playwright test
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-corpus.ps1
python scripts/check-corpus-independent.py <原有真实语料输出目录>
```

UI 套件默认需要 Edge，原有测试还含 Windows CLI/Python 路径；不能把仅 Linux 定向检查称为全量通过。新测试可通过 `RUST_WPS_CLI` 和 `PYTHON` 指定路径。前端构建完成后再运行测试，测试期间不要重建资源。

Windows 原生验收仍需按 DOT_HANDOFF/REGRESSION 的普通和 native-smoke 构建流程执行；本轮云端 Linux 不具备该环境。新 guard 经同一个 Rust `import_docx` 被 worker 调用，但这只是代码路径关系，不是原生回归证据。

本轮 Cargo.lock、package-lock.json、src-tauri/Cargo.lock 与 vendor 保持不变。完整浏览器、桌面和 Windows WebView2 检查仍需在具备所需依赖/权限的环境补齐；本轮没有完成整体验收或关闭阶段 1。
