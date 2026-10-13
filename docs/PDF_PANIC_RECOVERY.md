# PDF 提取器 unwind 与预览回退

## 修复的故障

两份固定、未改造的 Mozilla PDF.js 原件 `mixedfonts.pdf` / `noembed-sjis.pdf` 在 `pdf-extract 0.12.1` 中分别因 `UniJIS-UTF16-H` / `90ms-RKSJ-H` 触发普通 Rust panic。此前 CLI / 桌面解析 worker 退出 101；主进程没有崩溃，已有文档仍保留，但 `open_document` 在切换会话 / UI 前拒绝，既有 PDF.js 回退无法开始。

现在仅将提取器的普通 **unwind** 转为显式的提取失败：

- 严格 `inspect` 仍返回错误；CLI 退出 1，stdout 为空，不输出成功 JSON 或部分正文
- 公共 `inspect_for_preview` 返回预览候选，正文为空，保留源缓冲区的真实字节数和警告，`text_error` 以 `PDF text extractor panicked: ` 开头
- 预览候选允许已有页面 / PDF.js 正文恢复路径继续尝试；不是 Rust 成功解码、文件有效或页面一定可渲染的证明
- 普通成功正文、有效空白 PDF 与提取器原本返回的错误保持原策略 / 原诊断

桌面 `showPdf` 先打开并渲染页面，再尝试正文恢复。本轮实际执行的是公开核心 API、真实 PDF.js 文本 helper，以及 `ui/main.ts` 的恢复 / 字面查找原函数；没有执行真实页面渲染、IPC 或 WebView2 交互。因此只能确认这两份原件不再因该 Rust unwind 阻止返回预览候选，以及其 PDF.js 正文可恢复，不能声称桌面端到端已经验收。

## 捕获范围与安全边界

该 panic 回退轮中，`src/pdf_snapshot.rs` 的生产路径与测试共用私有 `inspect_with_extractor` callable。后续 [预览身份增量](PDF_PREVIEW_IDENTITY.md) 将实际生产 / 测试共用 callable 扩为 `inspect_identity_with_extractor`，旧测试入口委托给它。仍先执行原有 `read_source`（现在同时计算身份），再只对提取调用使用 `catch_unwind`：

- 已打开 reader 的 64 MiB + 1 有界读取、实际大小与 `%PDF-` 签名检查完全在捕获区外；I/O / 签名 / 超限仍直接拒绝，reader 自身的 panic 不被错误标为提取器失败
- 提取器只接收不可变、已验证的同一字节切片，解析状态局部持有。callable 有 `UnwindSafe` 约束，没有 `AssertUnwindSafe` 或共享可变应用状态
- 仅识别 `&str` / `String` payload；逐 Unicode 标量转义控制 / 格式字符，明细最多 512 UTF-8 字节，截断时额外添加一个省略号。非字符串为固定通用提示，不调用未知 payload 的 `Debug` / `Display`
- 没有修改全局 panic hook。默认 hook 仍可能把原 panic 写到 stderr；严格 CLI 错误与预览 probe 的 stderr 原样保存，不能靠 stderr 中有没有 `panicked` 判断退出结果。诊断不是保密机制，默认 hook 不受 512 字节明细上限约束
- 只处理 unwind。`panic=abort`、OOM / abort、栈溢出、二次 panic（包括任意 payload 析构时 panic）、挂起和解析资源膨胀不在保证内；64 MiB 输入限制不等于解压后内存 / CPU 配额
- 未改变桌面 worker 的异常退出、30 秒截止、响应上限或协议检查。这些硬故障仍拒绝，不把任意非零退出或无法识别错误泛化为 PDF.js 回退

在本篇记录的 panic 回退轮结束时，`read_current_pdf` 仍按会话路径重新打开，跨阶段源身份尚未绑定；该缺口现由独立的 [PDF 预览身份增量](PDF_PREVIEW_IDENTITY.md) 处理。文件读取期间原地并发写入也不保证原子快照。没有改写原 PDF、修复 CMap、加入 OCR / 字体映射 / Unicode 规范化 / 阅读顺序或 PDF 编辑。

## 确定性与真实文件检查

原 13 项源快照测试完整保留，另加 10 项检查：借用字符串、拥有字符串、非字符串 panic 的严格 / 预览和序列化策略；安全有界诊断；成功 / 空白 / 原有错误不变；非法签名、超限、有效前缀后 I/O 错误均未调用提取器；reader panic 不被吞掉；同进程捕获之后正常提取无旧状态或部分文字泄漏。

专用 `scripts/check-pdf-panic-recovery.mjs` 使用原 `scripts/pdf-snapshot-corpus.json` 的六份固定原件。之前的 `check-pdf-snapshot.py` / `check-pdf-phrase-search.mjs` 及其历史预期不改写。旧核心 CLI 固定为 `tmp/pdf-snapshot-final-cli`（SHA-256 `aea080512d18946111e35490c223643225df1a2e423d79360f3bc37bfec54c67`）。

- 四份原本成功文件：严格 JSON 仅除去 `elapsed_ms` 后完整相等，原始文字精确相等，候选公开预览结果同样核对
- 两份已知失败：旧 CLI 明确退出 101 / 空 stdout / 对应编码诊断；新严格模式必须退出 1 / 空 stdout / 对应 panic-specific 错误，新公开预览必须为空正文 / 真实字节数 / 同一明确错误。任何其他失败不能算通过
- 真实 PDF.js 5.6.205 Node legacy adapter、真实 `ui/pdf-text.ts` 与 `ui/main.ts` 的 `recoverPdfText` / `renderText` 原函数读取候选预览报告；完整恢复清除 `text_error`，失败 / 停止 / 部分结果保留错误并说明不完整，重复恢复不累积内容。DOM 只使用运输替身，不算浏览器验收
- Poppler 与 pypdf 独立读取。`noembed-sjis.pdf` 的 PDF.js 正文为 `あいうえお`；`mixedfonts.pdf` 的相关文字内容可读，但页脚顺序与 Poppler 不同；pypdf 对相关字体仍有警告 / 误解码。不能以单个消费者为全文正确性标准
- 原件、两个 CLI / probe、应用源码及公共 API transport 前后检查身份，证据必须写入新目录。一个已发表论文加五份上游 fixture，不统称六份用户生产文档；不分发原件二进制，不重新生成 / 重存 / 修改原件

## 本环境复现

沿用既有工具链 / target，不设置 RUSTFLAGS 或 profile 覆盖：

```sh
export CARGO_HOME="$PWD/../toolchains/cargo"
export RUSTUP_HOME="$PWD/../toolchains/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_INCREMENTAL=0
cargo +stable build --locked --lib --bin rust-wps
cp -n target/debug/rust-wps tmp/pdf-panic-final-cli
rustc +stable --edition 2024 scripts/pdf-preview-probe.rs \
  --extern rust_wps=target/debug/librust_wps.rlib \
  --extern serde_json=target/debug/deps/libserde_json-c159884722caddc3.rlib \
  -L dependency=target/debug/deps -o tmp/pdf-panic-preview
node scripts/check-pdf-panic-recovery.mjs --output tmp/pdf-panic-recovery-final \
  --preview-provenance tmp/pdf-panic-preview-provenance.json
```

上面的 serde_json 文件名是本环境当前 stable 构建的精确缓存身份，不能在另一环境猜测沿用；须使用同一候选构建的匹配依赖。provenance 文件是该次实际构建命令、compiler与所有链接输入 / 输出散列的记录，checker会复核记录对应的当前文件；另一环境需按其实际成功构建另作记录。输出目录必须尚不存在，复跑须换新目录。`scripts/pdf-preview-probe.rs` 仅负责调用公共 `rust_wps::inspect_for_preview` 并序列化，没有复制提取逻辑，也不是新增产品 CLI 命令。完整门禁、最终散列、失败保留和未运行的浏览器 / 原生范围见 [REGRESSION.md](REGRESSION.md)。
