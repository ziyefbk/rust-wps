# PDF 有界 ActualText 正文提取

> 发布 44（2026-10-16）：基于发布 43 `0f361dc`，本轮有界源语义提取及防覆盖保护已完成本地验证和独立接受。冻结代 5 的 stable / MSRV、提取模块、真实 worker 子进程运输、独立语义控制和浏览器证据均按下述范围接受。原 9 组 Word 功能重放通过，但原双读取器检查仍因两份中文 pypdf 不一致整体 FAIL；所有失败历史和原生 / 一般 PDF 兼容限制保留。

## 问题与接受边界

发布 43 的真实 Chromium 打印产物中，会议纪要与新建中文文档具有可读页面和正确的局部 `/ActualText`，但字形 `ToUnicode` 含康熙部首码点。旧 `pdf-extract 0.12.1` 和生产 PDF.js 提取路径未使用这些替换语义，导致正文、复制内容及部分字面查找错误。会议纪要的 `项目`、`文档`、`月`、`日`、`人`、`林舟`、`行动` 七个字面查询在旧正文中均不命中。

本轮只在源 PDF 已明确提供、能够完整审计的页面标记内容范围内应用原始 `ActualText`，并阻止 PDF.js 字形重提取覆盖已应用的语义正文。不修改 PDF 源文件，不做 NFKC、一般 Unicode 规范化、康熙部首替换或按字体全局映射。真正的部首、相同字形在不同出现位置的不同替换、组合字符和兼容字符均须按源字符串保留。

这不是 OCR、阅读顺序修复、页面渲染改进、所有字体映射修复或完整 PDF 语义实现。发布 43 的 pypdf 严格双读取器 FAIL、原截图和原始输出继续保留；本轮没有修复第三方 pypdf，也不以 Poppler 单读成功改写历史失败。新的有界验收必须比较冻结的作者全文预期、替换次数、字面子串和源字节身份。

## 实现与第三方来源

- 根依赖固定为 `pdf-extract = { version = "=0.12.1", path = "vendor/pdf-extract" }`
- 上游项目：<https://github.com/jrmuizel/pdf-extract>；包声明作者 Jeff Muizelaar；发布源码提交 `47b792ba04c5f3b6e478072b2efb8e1a2a06ec7e`
- crates.io 原归档 SHA-256：`e5c4820f5811e424ce037d08493ac87752ecc54339e0f1e40c3d17da93278861`，复制前与原根 `Cargo.lock` 的校验值核对
- 原始逐文件散列保存在 [`vendor/pdf-extract-upstream-files.json`](../vendor/pdf-extract-upstream-files.json)；原包 `Cargo.lock` 与 Cargo 缓存标记没有复制，注册表缓存没有修改
- 上游 `Cargo.toml` 声明 MIT；已检查的发布归档和上游仓库没有独立 LICENSE / NOTICE 文件。本地添加标准 [`LICENSE-MIT`](../vendor/pdf-extract/LICENSE-MIT) 及明确披露此情况的 [`NOTICE`](../vendor/pdf-extract/NOTICE)，保留原包元数据和源码注释，不虚构上游版权年份或声称该许可文件来自归档
- 新增 opt-in API `extract_text_from_mem_with_actual_text`；原提取 API 不调用新模块。根检查路径在既有同句柄有界快照上调用新 API；正常无 ActualText 时沿用原输出解释器

新代码的主入口为 `vendor/pdf-extract/src/actual_text.rs`，与 `src/pdf_snapshot.rs` 的错误 / 预览策略及 `ui/main.ts` 的会话级重提取保护贯通。既有长度和 SHA-256 源身份、不覆盖保护及旧文档保留策略不变。

## 当前支持的严格子集

1. 页面内容流中的 `/Span … BDC` 到对应 `EMC` 范围，覆盖 `BT` / `ET` 中非空的 `Tj` 或 `TJ` 字形内容。一个替换只输出一次，覆盖的字形正文不重复输出；替换字符串允许有意为空
2. 不带 ActualText 的普通嵌套 `BMC` / `BDC` 范围；它们的 `EMC` 不结束外层替换。不同替换范围不能重叠
3. 内联属性字典或当前页 `Resources/Properties` 中的具名属性；具名属性字典及 ActualText 字符串可使用受限间接引用。页面资源沿有界父链继承
4. 严格 PDFDocEncoding，或带 `FE FF` BOM 的 UTF-16BE；UTF-16 代理项必须成对，原始字符串中的 TAB / LF / CR 保留，字符串转义按 PDF 解析结果处理
5. 同一页 `Contents` 数组按顺序拼接后审计，标记范围可跨同页内容流；不得跨页，标记范围与文本对象范围不得交叉错配
6. 已发现的全部替换与审计均通过后才进入替换输出，且只有整次检查提取成功才提交正文；后续失败不返回先前成功部分。受支持页面范围之外的普通字形仍由现有解释器输出；页面几何分隔沿现有启发式处理，不承诺字节相同的布局空白或完整阅读顺序

`Type3` 字体本身不是一律拒绝项：可审计且无 ActualText 的 Type3 走原字形路径；页面级受支持 Span 可以覆盖可解码 Type3 字形。Type3 `CharProcs` 中的 ActualText 则不在支持范围内。

### Type3 宽度声明的特定词法边界

真实 Chromium 的字形程序使用合法 `d0` / `d1`，但 lopdf 0.42 的内容词法器不能正确处理操作符中的数字：有的程序直接拒绝，有的被错误拆成 `d` 与数字。本轮仅在审计阶段识别并消费 CharProc 开头的宽度声明：`d0` 必须正好带 2 个、`d1` 正好带 6 个词法合法且有限的 PDF 数值操作数；空白 / 注释可分隔 token。该声明计入操作预算，后面的每个字节仍走严格内容解析，不改写 PDF 字节或字形输出。

剩余 Type3 正文若解析出 `d`，目前无法分辨它是合法 dash-setting `d`，还是非开头 `d0` / `d1` 的词法误读，因此该子集保守视为未证明。合法虚线字形不是损坏文档：没有已知 ActualText 时保留精确旧正文，同时给出 `unknown` 和诊断；文档已发现 ActualText 时整次返回 `unsupported`。普通页面 / Form 中的 `d` 行为没有改变。头部验证失败也不能变成“未检测到”：可严格解析到的后续正证据仍在范围判断前收集。

## 明确拒绝或不能证明的范围

- 嵌套、重叠的 ActualText，包括外层非空 / 内层空字符串组合。独立 Poppler 控制对原先“外层优先”假设给出不同结果；本轮收窄为拒绝，保留原先预期与读取器结果，不选择对自己有利的解释
- 非 Span 标签中的替换、只覆盖绘图 / 图片而没有可解码字形的替换、空字形内容范围、范围内未支持的绘图或文本显示操作符
- Form XObject、pattern 流、Type3 CharProc 或结构树中的 ActualText；未使用的已发现资源也保守参与审计。当前实现进一步拒绝“已发现 ActualText 且审计发现任何 XObject `Do`”的整份文档，即使 Do 位于替换范围外
- UTF-8 BOM、UTF-16LE、奇数 UTF-16BE 字节、孤立或反向代理项、未定义 PDFDocEncoding 字节、语言转义序列、非字符串 ActualText
- 具名属性或资源无法解析、引用缺失 / 循环 / 超深、坏的 BMC / BDC / EMC、未闭合或跨页范围、跨越文本对象范围、外部内容流，以及已超过预算的审计
- 已证明的 Form 调用循环在进入旧递归解释器前拒绝；没有证明 ActualText 存在时，这是解析失败 / `unknown`，不能伪报为 `unsupported` 或“未检测到”。完整页面父链在找到最近 Resources 后仍继续检查；资源 / Form 审计失败或不完整且不能安全委托旧递归解释器时，明确拒绝旧回退

这些是当前保守合同，不是对全部合法 PDF 的否定。有效、可审计且无 ActualText 的 Forms / Type3 文档应保留旧正文；对不安全的损坏资源 / 父链 / Form 的提前拒绝，是不保留旧失败行为的安全例外。审计不完整时不能宣称不存在语义替换。

## 四态证据与预览

`Inspection.pdf_actual_text` 是可选、严格枚举字段，序列化值为 `not_detected`、`applied`、`unsupported` 或 `unknown`。非 PDF 检查不写该字段；旧 JSON 缺失字段仍可读取。它描述本次已检查源字节的证据，不认证页面外观、文字顺序或所有字体映射。

| 状态 | 后端含义 | 前端合同 |
| --- | --- | --- |
| `not_detected` | 审计完成且没有发现 ActualText，使用原提取输出 | 保留普通 PDF.js 重提取；只有完整成功时可清除旧 `text_error` |
| `applied` | 已发现的替换全部符合当前子集，输出已使用源语义；结果允许有意为空 | 禁用重提取按钮，自动和直接调用处理器也不得用 PDF.js 字形正文覆盖；显示持久语义提示 |
| `unsupported` | 已知 ActualText，但范围、编码、资源、输出或审计不能安全支持 | 严格 `inspect` 失败；预览保留原文件身份、空正文和 `text_error`，可尝试临时 PDF.js 字形正文但必须保留风险提示 |
| `unknown` | 没有完成语义证明，包括无已知 ActualText 的审计不确定、解析错误或提取 unwind | 若旧提取可完成则可能返回临时正文和诊断；否则仅预览候选。不能据 PDF.js 完成就升级为可靠语义 |

已知不支持时后端不会静默退回字形并报成功。读取、签名和输入大小等硬错误仍直接拒绝，不改成预览成功。普通 unwind 捕获仍不包括 abort、OOM、栈溢出或挂起。

前端对缺失或非预期的传输值按 `unknown` 处理，不能用警告字符串猜测四态。`unknown` / `unsupported` 的 PDF.js 恢复即使完整结束，语义状态和错误仍保留；持久提示明确正文仅为临时字形提取，搜索 / 复制不能保证正确。重试在没有新文字时取消或失败，保留此前正文。异步结果必须同时属于当前源对象、PDF 对象、会话 token 和此次尝试；取消 / 失败的 Open 保留旧源正文与提示，真正切换源后重新计算提示。

## 资源边界

源读取仍为同一打开句柄最多 64 MiB 的验证缓冲区。新增语义审计上限为：

- 引用和标记深度 64；对象访问 200,000 次；页面 1,000 页
- 内容操作 500,000 条；审计累计解码内容 32 MiB
- 单个替换原始与解码后字符串均最多 64 KiB；累计替换和语义路径输出各最多 2 MiB

解析与解压仍调用 lopdf，部分预算在解码后检查；这不是恶意 PDF 的硬内存 / 时间沙箱。无 ActualText 的旧输出路径没有被本次 2 MiB 语义输出限额重写。既有 Windows worker 的时间 / 输出 / 作业配额仍属另一层防护，本轮 Linux 模块控制不等于运行了 Windows Job 或原生 WebView。

## 夹具、预期与计数

[`tests/fixtures/pdf-actualtext/README.md`](../tests/fixtures/pdf-actualtext/README.md) 和 manifest 固定输入散列与来源，不得从修复后提取器重建预期。

- 仓库 manifest 共 107 个输入：65 个初始独立字节作者控制、8 个继承循环控制、31 个 Type3 词法控制，加 3 个真实 Chromium 操作产物（最初失败会议纪要、发布 43 当前会议纪要、新建中文文档）。这不是 107 个独立 Rust 测试函数，也不是真实第三方中文原件
- `tests/pdf_actual_text.rs` 的 6 个测试函数在冻结代 5 stable / MSRV 各实际通过；它们循环检查上述 107 输入的全文 / 字面子串、缺省路径、已知拒绝、unknown、输入散列 / 源字节与序列化，两个有意空替换必须精确为 `""`
- 独立验证共 109 个字节作者输入：68 个初始控制、8 个继承控制、31 个 Type3 控制和 2 个“先成功、后失败”控制。与仓库共享 104 个；独立额外包含累计替换 / 引用深度 / 操作数三个预算控制，以及缺失后续字体 / 反射方向两个后成功失败控制
- 109 个独立输入分别通过严格 CLI 和生产 preview API，共两种检查路径，不能记为 218 个不同场景。32 条生产 `findReadingMatches` 字面断言也复用其中输入与真实工作流结果，不另加成无重叠场景
- 独立真实工作流比较用发布 43 的会议纪要、新建文档、真实英文表格编辑后打印的 3 份 PDF；第三份无 ActualText，是普通路径控制。它与仓库所收录的 3 份 Chromium 输入不是同一集合
- 6 份既有 PDF 基线覆盖 tracemonkey、basicapi、copy_paste_ligatures、mixedfonts、noembed-sjis、rotation；4 份成功正文与旧基线精确相同，2 份已知字体编码失败保留，不能计为字体解码已修复

完整文本只移除布局空白后比较 Unicode、顺序和出现次数；精确子串断言另保护替换中的字面空白及非规范化 Unicode。来源散列、独立字面预期、旧 CLI 和 PDF.js 输出都在候选运行前固定。源 PDF 没有改写；真实部首和规范等价但字面不同的字符保持区分。

## 冻结代 5 验证结果与接受边界

| 项目 | 已核对结果 | 接受边界 |
| --- | --- | --- |
| stable / MSRV 1.88.0 定向 Rust 运行 | 各 366/366 PASS = 342 library + 6 ActualText + 8 inspection + 10 editing | 两工具链各自执行，不合计为 732 不同测试；107 fixture 输入包含在 6 个 ActualText 测试函数内 |
| 全根测试 / preview example 类型检查 | 两工具链 PASS；含 45 个 integration test targets 及根 lib / bin 测试和 preview example；CLI 均已构建，preview 二进制仅 stable 已构建 | 没有链接并执行全部 integration harness；不是完整 integration runtime suite |
| fmt / diff | PASS | 检查范围以精确命令为准 |
| scoped Clippy | 两工具链严格运行均保留历史 FAIL；stable 为 `clippy::chunks_exact_to_as_chunks` / `src/spreadsheet_xls.rs`，MSRV 为 `clippy::uninlined_format_args` / `src/presentation_import.rs`；各自精确 allowance 复跑 PASS | 不称无 allowance 严格通过；新增 unused-assignment 已实际修复 |
| 独立严格 CLI | stable / MSRV 各 109/109 PASS，除 elapsed_ms 外的 inspection 完全相同 | 同一 109 作者输入在两工具链执行，不重复算为不同场景 |
| 生产 preview API | stable 同一 109 输入 109/109 PASS；身份、错误、空正文合同保持 | 直接生产 API adapter，不是 Tauri IPC；没有 MSRV preview 二进制运行 |
| 独立生产字面查找 helper | 两工具链正文各 32/32 PASS | 同一断言分别执行，无 NFKC、无兼容查找开关；与上述输入重叠 |
| 3 份真实 Chromium 工作流 PDF | 两工具链各 3/3 PASS：两份中文全文 / 字面计数修复，表格原正文精确不变且 `not_detected` | 不改变 PDF 或第三方 pypdf；原双读取器 FAIL 保留 |
| 6 份既有 PDF | 两工具链各 6/6 预期结果保持：4 成功正文不变 + 2 已知编码拒绝 | 两次预期拒绝不是新提取成功 |
| 最终前端 | 850 个不同测试名 PASS = 838 headless（原 826 + 新 12）+ 8 Save Chromium + 4 PDF 密码 Chromium；TypeScript / build PASS | 精确名称并集无重复；初步 89 项是子集，不追加 |
| 真实语义浏览器 | 冻结代 5 的 12 个场景组 PASS，源与候选散列不变，无未捕获页面错误，已独立复核 | 与 850 测试名分开记；真实 Chromium + 实际 UI 源码 / PDF.js，原生运输为适配器 |
| 提取后端与 native PDF-export 模块 | stable / MSRV 各 35 production worker/session + 15 native PDF-export 函数 / 测试片段 PASS | 采用生产源码和实际库链接；不等于 Tauri GUI / IPC、Windows Job 或 WebView 执行 |
| stable 实际 worker 运输 | 22/22 PASS，含 16 次真实 Open 子进程往返；语义状态、正文、源身份、同长度源改变拒绝、fresh Open 及旧会话发布保护通过 | 生产 worker::run 启动子进程；不是 Tauri IPC / GUI，不能与重叠夹具相加 |
| 原 9 组 Word 工作流的本轮重放 | 9/9 功能流程 PASS，产品 PDF 正文失败为 0；原检查器整体仍 `passed=false` / exit 1，2 个中文 pypdf 严格比较 FAIL | 原脚本 / 预期未修改；表格严格通过，不能称整个原 checker PASS |
| 原生 Windows / Tauri IPC / WebView / 系统对话框 / IME / 系统剪贴板 | 本轮未运行 | 日常可用里程碑仍开放 |

独立后成功失败控制证明：先有可用替换，再遇到缺失字体时预览为 `unknown` / 空正文；反射方向失败为 `unsupported` / 空正文，没有先前成功文本泄漏。两个有意空替换均精确输出空字符串；每个 preview 身份与原文件 SHA-256 / 长度一致。

实际 worker 验证以冻结二进制 `5f0939c5a99818d19fc070ad6d19830b7da652fe0ef31d2537465d322c4671d1` 调用生产 `worker::run`，再启动同一程序的 `--document-worker`。16 次 Open 覆盖 applied / 有意空字符串 / unsupported / unknown，正文、长度和 SHA-256 与生产 preview 一致。只改动一份自有 677 字节临时副本，以同长度不同内容证明 verified read 和真实 rotate worker 在 staging 前拒绝旧身份、不输出源字节或残留目标；fresh Open 与旧 token 发布保护也通过。后者使用真实 SessionState / rotate_current 的独立 adapter，不是并发 GUI 试验。

12 组语义浏览器覆盖两份中文文档的精确查找 / 重复提取防覆盖、取消 / 失败 Open、真实候选空替换、替换相邻真部首及普通真部首、不带 ActualText 表格和普通正文、真实 unsupported / 加密 unknown 预览恢复及取消重试、旧传输缺字段，以及切回 applied 源后的提示隔离。只有缺字段 envelope 是模拟旧传输控制；unsupported / encrypted 报告来自冻结的实际生产 preview API。

### 失败历史原样保留

- 初代：引用 lopdf 私有编码表导致 E0603，类型检查失败，尚未链接或运行
- generation-2：全根测试类型检查和 342 library 已通过，但 ActualText 测试暴露合法 Chromium Type3 `d1` 词法问题，不能称该代通过
- generation-3：正证据审计与 `deep-scopes` 状态预期不一致；按原独立合同恢复已知 ActualText 的 `unsupported` 分类，未改源字节或预期文字
- generation-4：新增 `text_error` 无效初始化触发严格 Clippy，失败保留；generation-5 实际删除该初始化，没有以 allowance 掩盖
- 初期浏览器 shell 启动失败、一次重试以及此前成功的候选代均保留；只有最终冻结代 5 的指定散列用于上述正式复跑
- 实际 worker 首轮的两个失败仅为 harness 错用了中文诊断子串；源改变当时已被正确拒绝。原 `check_worker_g5_v1.py`、`WORKER_G5_HARNESS_V1.json` 和 `WORKER_G5_HARNESS_CORRECTION.json` 保留；只校正诊断断言后完整 22 例复跑通过，不改产品或弱化拒绝合同
- 发布 43 的严格双读取器 FAIL、pypdf 部首输出及旧 PDF.js / CLI 全文结果继续保留；本轮没有修复 pypdf

两工具链各 416 项运行 = 366 根测试 + 35 worker/session + 15 PDF-export 模块检查；测试名集合完全一致，共执行 832 次，不是 832 个不同测试。全 45 个 integration targets 只有 pdf_actual_text / inspection / editing 三个实际运行，其余 42 个仅类型检查；上游 pdf-extract 完整 Cargo dev-dependency 套件未运行。库 / integration 测试函数、fixture 循环子例、独立 CLI 案例和浏览器工作流分别计数。

## 冻结代与关键收据

冻结代 5 源清单 SHA-256 为 `fe84ff3975e677e6684f84fb9f464a682edd5e239b5b05195f757109b80cf2ce`；`generation-5/plan.json` 为 `237e78331decc1dcf2832c7f554818813c22b91f27f23bfce3538c1c9ab7f82f`。当前冻结二进制和关键收据：

- stable CLI：`3d7542e583580849e7b5e4608da206480b7ed9e369cfdf823e1bceaf12ba690d`
- stable preview API adapter：`218a7d858d5ec27723dba715364e7a3b8629743040ec9432e71e10269571c646`
- MSRV CLI：`2a1f6dca223ff6b789e4358a156c12b019b837a1106666b562718e2e3273d29d`；MSRV preview example 已类型检查，没有独立 preview 二进制 / 运行声明
- Rust `generation-5/stable-family/completion.json`：`eb835940fb2e00f8e7579cef75630420cfb9f2c88b3ea473af208b7c1489fa80`
- Rust `generation-5/stable-family/linkage.json`：`7ab270658232b9d695742e8174c53cb1152da8dd07238505ce26a23c792b2a26`
- Rust `generation-5/msrv-family/completion.json`：`95366c02df5f44acac0130b01bcd5714a864aa1756f807ac21f6ecaddbce527d`
- Rust `generation-5/msrv-extracted/completion.json`：`f2914936a79a458056131b12aef5f409bb6eb75de69c81ab920e6dc0ce9a5d66`
- Rust `generation-5/stable-extracted/completion.json`：`33b31465102f6dc0001ab1d006d699506205ea7df02ff632dc19d5b743d001ea`
- 独立 `stable-g5-assessment.json`：`39fcc96617f76fd596321006b56b93d16ac7d70ef7f2e56de048377b939c2006`
- 独立 `stable-g5-preview-results.json`：`70776c047007b4436de01af3d53ce930782ba9cb3eb56c7e28cabb6b85948474`
- 独立 `stable-g5-literal-search.json`：`76884f38f98966c5cb0bd84a1c2a790f0dbf2367016b2a4ad98cfee920cc6bd3`
- 独立 `stable-g5-real-corpus.json`：`b74db175a174567e2a91b7ba358f9ca966d077fd10beea7ab9055e80915f0d9d`
- 前端 `tmp/release44-frontend/attempt-7wj65bhx/report.json`：`e7fb9643a39abf89b187023aec93ac99be4d1a2b0e06e678f42de8134e9fdd80`
- 850 名称并集 `tmp/release44-frontend/named-test-union.json`：`cbf2dc81596d71b8471d34858d05bb5df4f369e169b7ab93592d2f83410b8eca`
- 12 组浏览器 `tmp/browser-pdf-actualtext/candidate-v5/report.json`：`5e5558e4eab6060a99e20a7d75fb1af3b00de588111a508cef276e91bd7835bd`
- stable 独立综合 `FINAL_G5_INDEPENDENT_REVIEW.json`：`bbedcc970958b9d0c584d996d287dd41a09406c8c6c6c408aa7e2a235fc90cb4`
- 浏览器独立复核 `BROWSER_G5_INDEPENDENT_REVIEW.json`：`e843ceea3c9133ca658e7d134118791c4c4252c228d3d60e89bf4b55803782af`
- 两工具链 / worker 独立综合 `FINAL_G5_COMBINED_REVIEW.json`：`c54151d8918ccff27ea881ff410757b8a7857a9645a49cdc9fb69ede24bfc8af`
- MSRV 精确 inspection 对照 `MSRV_G5_COMPARISON.json`：`f2bd99880e60dcd07ca2faac3e16632024affb964f5d49debc5f4ff7d49d7777`
- 实际 worker `WORKER_G5_INDEPENDENT_REVIEW.json`：`37b3773f1f3b69b38ed65e1379d761ac15156f062ec36580b060dd6e34879b75`
- Word 重放独立复核 `WORD_REPLAY_G5_INDEPENDENT_REVIEW.json`：`995557a4c8330b64010e237721899ab67a83f99627118972a054b1e8e132c382`
- Rust 组合 `generation-5/combined.json`：`b5f35f3c25bfd961d87021754e03e94e10f475c60c95438264dd216f74807077`
- 最终只读源 / 收据审计 `generation-5/final-read-only-audit.json`：`929c2a797515977dd5250940a8d2c82fd8c7d61bab35b217f54c48c0438a183b`
- 原 Word 重放 `tmp/browser-word-workflow/run-xUSQs7/report.json`：`39142599aa0fe302fca2078ba4521520275af9d2144539be693b08aa76d23ce8`
- Word 资源守卫 `tmp/browser-word-workflow/final44-candidate5.guard.json`：`329a072daea0aa7dd019d5f449a456beae91976b9f8545cd5fc9923027cab0e9`

Rust 相对路径以 `tmp/pdf-actualtext-regression-20261016` 为根，独立结果以 `tmp/release44-independent` 为根。实际 worker 运输与模块运行分别有独立收据；本轮 Word 重放的原严格失败不得改写为通过。

## 复现命令与证据目录

以下是已存在脚本的准确入口。不自动安装、下载、清理缓存或覆盖旧证据。需要已存在的固定 Rust 工具链 / 依赖、Node/npm 依赖、Chromium、Python 和 Poppler；本轮未证明 fresh-host 资格。审计 runner 固定本机来源和历史收据，在另一台机器重建时须先重新建立对应计划，不能把临时证据目录假称为仓库自带依赖。

### Rust 资格验证

冻结代 5 两工具链和提取模块的实际入口为（依次执行，不并行）：

```sh
python3 tmp/pdf-actualtext-regression-20261016/generation-5/run.py stable family
python3 tmp/pdf-actualtext-regression-20261016/generation-5/run.py msrv family
python3 tmp/pdf-actualtext-regression-20261016/generation-5/run.py msrv extracted
python3 tmp/pdf-actualtext-regression-20261016/generation-5/run.py stable extracted
python3 tmp/pdf-actualtext-regression-20261016/generation-5/supplementary-checks.py
python3 tmp/pdf-actualtext-regression-20261016/generation-5/aggregate-results-v2.py
```

runner 要求保留收据和源冻结一致，输出目录必须不存在；不得在原目录覆盖重跑。每阶段的实际 `argv` / cwd / profile / 空间门槛在执行前写入 `intent.json`，结果和采样轨迹写入 `result.json` / `disk.jsonl`。模块使用生产 worker/session 源码及 native PDF-export 的准确函数 / 测试片段，与本次生产库链接；没有运行 Tauri 命令 / 原生 WebView。补充 rustfmt 检查覆盖 vendored / 新增模块和三个 native 模块；aggregate 只汇总已有精确名称 / 收据，不是重跑测试。Linux-filtered native `cargo metadata --locked --offline` 已解析到 vendored 包；不带平台过滤的 metadata 因离线未缓存 `android_system_properties 0.1.6` 受阻，没有执行原生构建。

冻结代 5 的 Cargo 覆盖包括以下准确参数，实际执行顺序及编译器绝对路径以收据为准；由资源守卫串行执行，不脱离守卫并发运行：

```sh
cargo +stable check --locked --offline -j1 --tests --example pdf_preview --message-format=json
cargo +stable test --locked --offline -j1 --lib --no-run --message-format=json
cargo +stable test --locked --offline -j1 --test pdf_actual_text --no-run --message-format=json
cargo +stable test --locked --offline -j1 --test inspection --no-run --message-format=json
cargo +stable test --locked --offline -j1 --test editing --no-run --message-format=json
cargo +stable build --locked --offline -j1 --bin rust-wps --example pdf_preview --message-format=json
cargo +stable fmt --package rust-wps --check
git diff --check
cargo +stable clippy --locked --offline -j1 --lib --bin rust-wps \
  --test pdf_actual_text --test inspection --test editing --example pdf_preview \
  --message-format=json -- -D warnings
cargo +stable clippy --locked --offline -j1 --lib --bin rust-wps \
  --test pdf_actual_text --test inspection --test editing --example pdf_preview \
  --message-format=json -- -D warnings -A clippy::chunks_exact_to_as_chunks
```

MSRV 将工具链换为 `+1.88.0`，CLI build 不带 `--example pdf_preview`（example 已在首项类型检查），最后 allowance 换为 `clippy::uninlined_format_args`。两工具链严格失败和各自单一历史 allowance 成功必须同时保留。runner 从 Cargo JSON 取得本次二进制，先用 `--list` 核对测试名称，再以 `--test-threads=1` 运行指定 target。`check --tests` 只证明所有根测试 target 类型检查，不能称未列出的 integration targets 已执行。

开发 / 测试两个 profile 均显式设定：`DEBUG=0`、`OPT_LEVEL=0`、`DEBUG_ASSERTIONS=true`、`OVERFLOW_CHECKS=true`、`CODEGEN_UNITS=16`、`INCREMENTAL=false`、`LTO=false`、`PANIC=unwind`、`STRIP=none`、`RPATH=false`、`SPLIT_DEBUGINFO=off`（变量分别以 `CARGO_PROFILE_DEV_` / `CARGO_PROFILE_TEST_` 为前缀）；另设 `CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=1`、`CARGO_CACHE_AUTO_CLEAN_FREQUENCY=never`、`CARGO_NET_OFFLINE=true`。工具链家目录和分离 target 路径以该代 `plan.json` 为准。

冻结代 5 资源合同：硬保留 1 GiB，采样停止线 1,152 MiB；Cargo 编译入场至少 1,344 MiB、单阶段增长不超过 192 MiB；直接编译入场 1,280 MiB、增长不超过 128 MiB；运行入场至少 1,184 MiB、单阶段增长不超过 32 MiB；全代增长不超过 768 MiB。每 0.1 秒检查磁盘，单阶段最多 1,800 秒，单路输出最多 64 MiB，停止后收束整个进程组。原较早代门槛保留在原计划，不能混写成代 5 参数。runner 不自动清理，收据保留 `cleanup_performed=false`；最终 Rust 组合记录累计峰值占用 773,771,264 字节（小于 768 MiB），最低采样剩余 1,329,111,040 字节。未修改注册表缓存或清除原失败 / 旧基线。

### 最终前端 aggregate

实际运行命令：

```sh
python tmp/release44-frontend/run.py \
  --cli tmp/pdf-actualtext-regression-20261016/generation-5/stable-family/candidate-cli \
  --cli-sha256 3d7542e583580849e7b5e4608da206480b7ed9e369cfdf823e1bceaf12ba690d \
  --provenance tmp/pdf-actualtext-regression-20261016/generation-5/stable-family/linkage.json
```

runner 先检查 `source-pins-final.json`，再在隔离副本执行 prepare-pdf、TypeScript、production build 和精确 838 名称 Playwright 清单；保留完整命令参数和原始 JSON。850 名称并集另加入 `tmp/release44-save-browser/final1/report.json` 的 8 项与 `tmp/pdf-semantic-browser-regression/cloud-report.json` 的 4 项，两组为真实 Chromium；原初步 `tmp/pdf-actualtext-focused/report.json` 的 89 项是子集，不追加。

### 最终真实 PDF 浏览器

实际 `/workspace/shared/pdf45-run.sh` 使用冻结 CLI、preview API adapter 和独立语义 manifest：

```sh
node scripts/browser-pdf-actualtext.mjs \
  --cli tmp/pdf-actualtext-regression-20261016/generation-5/stable-family/candidate-cli \
  --cli-sha256 3d7542e583580849e7b5e4608da206480b7ed9e369cfdf823e1bceaf12ba690d \
  --preview-cli tmp/pdf-actualtext-regression-20261016/generation-5/stable-family/candidate-preview \
  --preview-cli-sha256 218a7d858d5ec27723dba715364e7a3b8629743040ec9432e71e10269571c646 \
  --semantic-manifest tmp/release44-independent/fixtures/manifest.json \
  --workflow-source-dir tmp/browser-word-workflow/run-yMJlKi \
  --output tmp/browser-pdf-actualtext/candidate-v5
```

复跑时省略 `--output` 创建新的 `run-*` 目录，不能覆盖 `candidate-v5`；需要可运行的云电脑 Terminal，必要的进程局部 PATH 在原脚本中保留。本次 Chromium `154.0.8037.57` / PDF.js `5.6.205`；实际 UI 由 Vite 源码转换，不是原生生产包。Tauri 运输、文件选择和 token 为测试适配器；已知不支持 / 加密源报告来自实际生产 preview API，只有旧传输缺失字段 envelope 是模拟控制。完整截图、浏览器正文、查询命中和源 / 候选散列保留。

### 原 9 组 Word 工作流重放

本轮使用未修改的原 workflow / 独立 checker / 来源及预期，绑定最终 stable CLI。实际命令由 `/workspace/shared/pdf45-word.sh` 经空间 / 时间守卫执行：

```sh
python3 /workspace/shared/word45-guard.py node scripts/browser-word-workflow.mjs \
  --cli tmp/pdf-actualtext-regression-20261016/generation-5/stable-family/candidate-cli \
  --cli-sha256 3d7542e583580849e7b5e4608da206480b7ed9e369cfdf823e1bceaf12ba690d \
  --source tests/fixtures/word-workflow/independent-chinese-minutes.docx \
  --expected tests/fixtures/word-workflow/independent-chinese-minutes.expected.json
```

新输出位于 `tmp/browser-word-workflow/run-xUSQs7`；9 项功能全部通过，无 `productPdfTextFailures`，所有来源 / UI / checker 散列保持。独立复核确认三份新打印 PDF 的产品 / Poppler 完整 Unicode 均匹配作者预期，并直接查看四页新光栅图：文字和表格可读，未见明显裁剪或缺字框，两页文档手动分页保持；这是受测输出的视觉证据，不是完整页面保真保证。原检查器没有被放宽，仍 exit 1 / `passed=false`，仅会议纪要和新建中文文档两个严格 pypdf 全文不一致，普通表格严格通过。该结果同时说明产品语义修复生效和第三方读取器严格门禁仍失败；不是失败被豁免后的整个 checker PASS。实际 UI / TipTap、冻结 CLI 和真实 Chromium 打印配合测试运输 / JSON 草稿，不能外推生产 Rust 草稿、真实中文输入法、系统剪贴板或原生打印。

资源守卫没有触发：入场 / 运行保留至少 1 GiB，增长最多 128 MiB，240 秒超时和 0.2 秒采样；实际约 23 秒，退出码 1 来自原严格 PDF 检查，不是空间或超时停止。复跑必须创建新 guard 输出路径，保留本轮 FAIL 收据。

### 独立冻结证据

- `tmp/release44-independent/PRE_CANDIDATE_PINS.json`：作者输入、预期及候选前散列；后续独立 supplement 的各自 manifest 保留来源
- `BASELINE_FROZEN.json`、`PDFJS_BEFORE_FROZEN.json`、`POPPLER_LAYOUT_BASELINE.json`：旧 CLI / 旧生产 helper / Poppler 的完整工作流基线
- `CONTRACT_NARROWING.json`、`baseline-authored-results.json`、`baseline-additional-authored-results.json`、`preflight-v1`：重叠替换合同收窄和原失败，不覆盖
- `EXISTING_CORPUS_BASELINE.json`：6 份既有 PDF 的来源身份、旧全文和已知拒绝
- `stable-g5-{assessment,preview-results,literal-search,real-corpus}.json` 及 `msrv-g5-{assessment,literal-search,real-corpus}.json`：最终两工具链证据；`*-authored-results.json` 保留每个 CLI / Poppler 的原始输出
- `WORKER_G5_INDEPENDENT_REVIEW.json`、`WORD_REPLAY_G5_INDEPENDENT_REVIEW.json` 和 `FINAL_G5_COMBINED_REVIEW.json`：真实 worker、原 Word 重放和总独立范围复核

独立最终收据中的实际命令如下；`CLI` / `PREVIEW` 为前述 generation-5/stable-family 内冻结二进制。复跑必须统一改用新的结果 label，不能覆写下列 `stable-g5*` 正式收据：

```sh
CLI=tmp/pdf-actualtext-regression-20261016/generation-5/stable-family/candidate-cli
PREVIEW=tmp/pdf-actualtext-regression-20261016/generation-5/stable-family/candidate-preview
python tmp/release44-independent/check_real_candidate.py "$CLI" stable-g5 3d7542e583580849e7b5e4608da206480b7ed9e369cfdf823e1bceaf12ba690d
python tmp/release44-independent/check_authored_fixtures.py "$CLI" stable-g5
python tmp/release44-independent/check_authored_fixtures.py "$CLI" stable-g5-additional additional
python tmp/release44-independent/check_authored_fixtures.py "$CLI" stable-g5-inheritance inheritance
python tmp/release44-independent/check_authored_fixtures.py "$CLI" stable-g5-type3 type3
python tmp/release44-independent/check_authored_fixtures.py "$CLI" stable-g5-post-success post-success
python tmp/release44-independent/assess_candidate.py stable-g5
python tmp/release44-independent/check_preview_candidate.py "$PREVIEW" stable-g5 218a7d858d5ec27723dba715364e7a3b8629743040ec9432e71e10269571c646
node tmp/release44-independent/check_exact_search.mjs stable-g5
python tmp/release44-independent/check_browser_g5_review.py
python tmp/release44-independent/write_final_review.py
```

上面最后两个命令复核已有结果 / 来源 / 候选 / 图片散列并汇总 stable 收据，不是再次执行浏览器或完整测试。MSRV 对相同作者 / real-corpus / search 命令使用 `msrv-g5` label、MSRV CLI 路径及其明确散列 `2a1f6dca223ff6b789e4358a156c12b019b837a1106666b562718e2e3273d29d`；不重跑 preview 命令。实际 worker 命令为 `python tmp/release44-independent/check_worker_g5.py`，它固定 worker 散列并创建新的 scratch 目录；复跑须另建 scratch / 输出目标，保留首轮失败和本轮正式收据。全部 75 个初始 pins 和 109 个作者 PDF 字节仍不变。浏览器独立视觉复核针对正文截图：三张与已审阅的前代图像逐字节相同，新空正文图另行直接查看，不等于新做页面保真证明；发布 43 原四页 PDF 视觉证据仍保留；原 Word 重放另有上文所述四页新图直接复核。

## 仍开放的日常使用门禁

本轮有界改善已由两工具链、生产 preview / worker 和浏览器独立证据支持：上述源语义子集产生正确正文，已应用正文不被页面字体重提取覆盖，不能证明的情况持续明确提示，旧正常路径和源身份保护保持。原 Word 重放 9 功能组通过但原严格 checker 整体仍 FAIL；本轮只接受明确的源语义修复和保护范围，不改变严格双读取器门禁。真正中文外部语料、一般 PDF 阅读顺序 / 字体映射、OCR、原生 Windows IPC、系统复制 / 打印 / 文件对话框、真实输入法、生产草稿与完整 Office 排版仍为独立门禁。
