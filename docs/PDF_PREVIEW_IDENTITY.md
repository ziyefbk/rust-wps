# PDF 预览源身份与会话选择

本轮基于 `9431f8485796cf963cc49f2a935f1c1a6b64ab7d`，补齐源快照与随后 PDF.js 读取之间的身份断点。原来 Rust 检查 A 后，页面预览会重新打开路径；路径已经换成 B 时，正文与页面可能静默来自不同文件。本轮遇到变化明确要求重新打开，不自动换身份或重新解析。

## 受支持的保证

- 新公共 API `inspect_for_preview_with_identity` 返回原 `Inspection` 与独立的 `pdf_identity`。长度与 SHA-256 来自经过原 64 MiB 有界读取 / `%PDF-` 校验的同一缓冲区，且在提取器普通 unwind 捕获区外计算。提取成功、空正文、返回错误、加密、畸形及普通 panic 预览候选均使用同一身份
- 原 `Inspection` 字段、`inspect` 与 `inspect_for_preview` 接口及序列化结构不变。旧严格提取失败仍失败；身份有效不表示 PDF 有效、可解密或可渲染
- worker 的 `Opened` 信封只增加固定大小的长度 / 32 字节摘要，既不传整份 PDF，也不增加 32 MiB 响应、64 KiB 请求、30 秒解析截止与 Windows 512 MiB Job 配额。PDF 缺失 / null / 非法摘要宽度 / 不一致长度及路径或格式错配，在提交会话前拒绝；非 PDF 不携带该身份
- 会话同时保存原路径、PDF 身份与十进制字符串 token。打开请求在文件对话框 / worker 之前预留递增 ticket；只有最新未使用 ticket 能提交。新请求取消或失败保留已提交文档，却仍阻止更早未完成请求重新出现。相同路径重新打开也获得新 token；计数溢出拒绝，不回绕
- `read_current_pdf` 必须收到前端原样回传的字符串 token。实际生产模块先检查会话、释放锁、只读取一次有界缓冲区、核对其长度 / SHA-256，再检查会话，最后返回同一个 `Vec`。读取期间不持有会话锁，不验证后第二次重开
- 已变化或无法验证的路径提示重新打开。不会更新旧指纹，不会静默改正文。已加载 PDF.js 缓冲区可继续显示与原检查匹配的旧内容；显式重新打开才选择新源
- 前端在读取、加载、密码回调、页面获取、正文重提取及旧文档销毁之后核对所属文档。迟到结果丢弃并清理；旧密码回调不能关闭新提示框，旧提取结果不能写入新正文或状态。密码仍只交给本机 PDF.js，不进入 Rust IPC 或草稿
- 另一处会话写入 `replace_presentation_current` 同样被 ticket 保护，原 PPTX 类型检查保留。若文件已另存但选择结果过期，明确报告已保存路径及未切换当前文档；不把它称为保存失败

## 不在保证范围

这不是文件系统原子快照、历史修改侦测、文件锁或沙箱。读到的缓冲区与原身份一致即通过；A → B → A 恢复完全相同字节可以通过。并发原地写入仍不是原子读取，只会对最终缓冲区做一致性核对。摘要不承担密码认证或文件访问授权。

64 MiB 是源字节上限，不是解码后资源上限。新摘要工作量随已读取字节数线性增长，摘要状态固定；没有新增第二份源缓冲区或把 64 MiB 装入 worker JSON。后续文件读取没有新增独立时间截止；既有解析截止不应被误述为覆盖这次读取。abort、OOM、栈溢出、双 panic、挂起及复杂字体 / OCR / 排版问题仍未解决。

## 可复现检查

核心使用既有缓存工具链与目标目录：

```sh
export CARGO_HOME=$PWD/../toolchains/cargo
export RUSTUP_HOME=$PWD/../toolchains/rustup
export PATH=$CARGO_HOME/bin:$PATH
export CARGO_INCREMENTAL=0
cargo +stable test --locked
CARGO_TARGET_DIR=$PWD/tmp/msrv-target cargo +1.88.0 test --locked
cargo +stable build --locked --lib --bin rust-wps
USERPROFILE=$PWD/tmp/linux-home npm run build
```

不设置 RUSTFLAGS 或 profile 覆盖。不要对 vendor 执行全目录格式化。独立检查器 `scripts/check-pdf-session-identity.mjs` 只写入全新证据目录，使用冻结旧 CLI / 预览程序、当前 CLI、与当前库链接的 `scripts/pdf-session-probe.rs` 及逐文件构建身份记录。最终运行位置与计数见 [REGRESSION.md](REGRESSION.md)。

该 probe 直接包含生产 `document_protocol`、`document_session` 与 `worker` 模块。`open` 真正启动独立子进程并使用既有请求 / 响应协议；`verify` 将生产函数返回的完整 `Vec` 原样写出，供独立哈希与 PDF.js 检查。它不是复制实现，也不是 Tauri 命令、原生 IPC、系统对话框、浏览器或 WebView2 验收。

新增前端测试读取并转译实际 `main.ts` 函数，通过确定性传输 / DOM 替身覆盖 token 精度、变化拒绝、过期读取 / 加载 / 提取 / 页面、同路径重开、密码取消 / 重试、重叠加载、新旧选择、失败后控制恢复。更新的历史浏览器样例携带会话 token，但本轮没有运行真实浏览器规格。

原件仍为 [固定 Mozilla manifest](../scripts/pdf-snapshot-corpus.json) 的六份未改造 PDF。独立检查比较所有字节、Node SHA-256、Poppler / pypdf 和真实 PDF.js；A/B 路径替换只在新证据目录复制件上进行。同长度负例另行标为受控改动，不冒充未改造原件。两份真实 CMap panic 的原有严格失败 / 预览回退 / PDF.js 正文恢复保留；旧历史 `check-pdf-panic-recovery.mjs` 的整文件前端冻结保护没有改写，新后继检查器明确审计此次前端变化。

## 环境边界

实际 Chromium 进程权限限制、云端浏览器对本机服务的连接限制、Linux GLib/WebKit 构建依赖及离线 Windows 环境仍未解除。本轮不重试已拒绝的浏览器路径，不把独立进程或 DOM 替身称为真实桌面验收。完整原生打开 / 取消 / 重试、页面像素及 WebView2 IPC 仍需在可用 Windows 环境补验。
