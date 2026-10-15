# 有界自动表格首选列宽（发布 38）

发布 38（2026-10-15）：本地验证已完成；验证范围与限制如下。最终 v2 stable / MSRV 1.88.0 各 856 项 / 47 阶段已独立接受，测试名称集合相同；源 / 包、收窄的真实原件消费和编辑证据、两工具链 portable 资格验证已接受。一般 intrinsic AutoFit / 浏览器 / 原生 Office 保真仍未证明。

简单顶层表格新增独立的自动首选列宽，保留受证明的 auto/0 与逐列 grid / tcW 提示，不把首选值解释为固定物理边界。未改造 Bug60337 在固定 LibreOffice 中的源 / 产品完整测量与两分辨率光栅相同，本样本此前列宽差异消除。两轮 AUTO / FIXED 区分控制均不具区分力，原消费者区分门禁未满足；接受范围明确收窄到 OOXML 语义与该真实原件改进，不承诺一般 AutoFit、固有内容适配或浏览器 / Word / WPS 保真。

## 首选宽度与固定宽度是不同模式

新增仅限 table 的 `autoPreferredColumnWidthsTwips`，不复用固定列宽 `columnWidthsTwips`；两者按属性存在互斥。正整数 twip 数组长度须与列数一致，1–50 列，每项及合计不超过 31,680；null、字符串、分数、非法位置或不匹配拒绝。编辑器内部 null 默认值仅在规范保存时省略。无新字段的旧模型保持旧语义和导出字节。

自动首选模式导出 `tblW type=auto,w=0`、首选 tblGrid 与逐格匹配 dxa tcW，省略 fixed tblLayout。固定模式仍输出原固定布局，重置则恢复旧自动规范化。首选列宽只是消费者提示，不是实测单元格宽度；名义合计未超页面正文宽度也不能证明文字或表格物理包含。

## 源证明与既有拒绝优先级

仅支持正文直接、未合并且非嵌套的简单矩形表格；tblLayout 缺省或显式 autofit，tblW 必须明确 auto/0，完整正整数 grid 和每格直接 dxa tcW 必须逐列相同。须有已证明显式页面，首选合计适配正文名义预算；位置沿用旧证明，缺省 / 左对齐及缺省 / 显式零缩进可以。Bug60337 的 `tableIndentTwips: 0` 保留，不为了资格删掉零值；原默认 108 twip 单元格边距语义不改。

新模式不接受显式自定义单元格边距、单元格段落缩进（即使显式零），正表格缩进、居中 / 右对齐或 noWrap / fitText 等依赖。源及作者范围分别校验，不能删掉依赖属性后冒称源已证明。缺 / dxa tblW、缺失或不一致首选等此前合法源继续旧规范化；旧损坏 / 继承 / 重复 / 非直接 / 外命名空间 / 合并等硬拒绝不会被新可选路径吞掉或降级。

同一包中的实际 OPC 身份、主部件 / styles 关系、默认及活动样式依赖、完整表序号、表头双标记、行 / 格 / 段落文字顺序在 raw→IR→最终模型核对。受支持零文字书签 / proofErr 路径也需闭合和身份证明。普通几何不合资格按表保留旧模型；共享样式 / 文字 / 书签证明损坏使新证明组失败。全部旧 fallible 附加和验证先完成，再对完整克隆附加和验证，成功才提交；不在失败时留下半张新模式表。旧公共配额和先到达的样式链门禁不扩展，内部 64 级 / 2,000,000 工作防线不是公共输入放宽承诺。

## 风险文字与混合文档

成功保留时实际风险为：

`Automatic table width preferences preserved; intrinsic layout and pagination remain consumer-dependent`

未保留的旧自动表继续保留适用的旧风险：

`Autofit table widths normalized; fixed column widths were not inferred`

以及适用时：

`Unpositioned automatic table widths normalized to the explicit page content area; source widths are not preserved`

只有全部相关自动表已保留才清除对应旧规范化风险；混合文档不能因一张成功而删掉其余表的警告，旧颜色 / 字体 / 边框等风险不抹除。代码中的 `Automatic table width preferences not preserved: source or converted identity is outside the supported subset` 是内部 audit 失败标记；当前公共附加路径不把它插入风险集合，不能将该字符串描述为已输出的新警告。

## 编辑、原子转换与页面预算

无新增首选数值控件。文字 / 样式 / 颜色 / 无底色 / 行分页与受支持纵向属性编辑保留首选。增删行、切换表头保留列向量；插列复制相邻首选值，删列移除对应值，全列删除删除整表。所有修改先验证列数、总量与页面名义预算，不合法时整次拒绝，不写历史 / 草稿。

现有固定列宽操作从首选向量取其他列初值，应用所选固定值并原子清除首选；宽度重置同时清除两种模式，不能因原 fixed=null 而漏掉首选。撤销 / 重做恢复整个模式与无关属性。正缩进 / 居中右对齐 / 自定义边距 / 单元格段落缩进须先重置或转固定，否则拒绝。页面缩到首选合计以下、或移除显式页面同样整次拒绝，不静默缩放 / 丢弃首选。

草稿及 owned HTML 元数据保留合法首选，非法 / 错位置 / 冲突的 owned 粘贴元数据在 schema 丢弃前拒绝；任意外来 CSS 不算源证明。渲染采用 table-layout:auto 与首选 colgroup 提示，不走 fixed colwidth 物理承诺。模式转换清掉旧 colgroup、width / min-width / layout 与旧 page-auto 装饰，print clone 同样检查；这些模块检查不代表真实浏览器排版或物理打印。

## 实际接受结果与未完成门禁

- 独立源 / 包：56 个源端点、14 个产品模型、302 个无新增字段旧作者 DOCX 整包 golden、1,018 个命令通过；完整导入风险 / 模型、外部导入、可信重开、原始 XML / 部件及不覆盖均按预先冻结完整预期核对
- 编辑链路：20 状态 / 63 CLI 调用、实际完整模型与独立 XML 通过；最终 752 个不同前端检查 = 原 724 + 新 28，TypeScript / build 通过。这是 detached 编辑器 / DOM 运输与实际 CLI，不是真实浏览器、系统剪贴板、原生按键或打印验收
- 最终 v2 stable / MSRV 1.88.0 各 856 项 = 原 845 + 新 11，325 library + 531 integration，47 阶段含 46 harness 和显式 doctest，failed / ignored / filtered 均为 0，已独立接受。这是串行无筛选 Cargo target 覆盖，不是一次字面 cargo test 命令；原 driver aggregate 的阻断记录保留，独立原始覆盖 aggregate 另给出接受。两工具链完整测试名称集合相等，各自实际执行，不合计成 1,712 个不同测试
- 两工具链生产后端提取各 35 项通过，但属于 v1 generation；最终 v2 不冒称重跑后端。最终 v2 两工具链构建、fmt / diff 及 scoped Clippy 已通过：原严格 Clippy 均返回 101，仅允许对应单一既有 lint 后返回 0，不称无 allowance 严格全通过
- 最终 v2 桥接：112 调用覆盖 56 源端点与 14 产品，14 DOCX 整包与已接受 v1 精确相同；完整 trusted / external 响应及不覆盖通过。此前 v1 的 752 前端、20 编辑状态、消费者及后端结果仍按 v1 记录，不描述为 v2 重新执行
- 公开小包按不可变的 5 文件草稿和 3 个共享 fixture / license 文件隔离布局运行：最终 v2 stable / MSRV 均 positive 返回 0，旧 release37 全响应不匹配、损坏 fixture、损坏 expected、陈旧 XML 哈希四项负例返回 1；输入与 CLI 散列不变。只是在同一云主机运行，不是 fresh-host 验收
- 主消费者：5 对源 / 产品、10 新 PDF / 20 整页 PNG，全部捕获字段及 72 / 144 dpi 光栅精确一致；107 字符长 token 全部存在，受测格界内未观察到裁剪。不是通用物理包含保证
- 加强区分控制：4 新 PDF / 8 PNG，源 / 产品同样精确一致；AUTO 与 FIXED 也零差异。原控制及加强控制均 INCONCLUSIVE，原区分门禁未达成，不标记完整原消费者计划 PASS。接受口径缩小为语义与特定真实样本改善

## 原始失败、不可区分结果与收窄接受

首次 candidate-nm02op8_ 在第 59 个命令因外部导入完整预期失败停止，原始失败保留。实际外部导入会增加既有黑色 textColor / 空段默认字体；先用冻结 release37 CLI 证明原有规范化，再通过 28 个 old-only export/import 调用预绑定全部 14 个完整外部响应。9 个完整响应不变，5 个仅有预声明宽度模式 / 风险差异；原 56 源端点预期没有更改。独立审核接受 v2 全响应 oracle 后，candidate-pdh189jg 才运行并通过，不是忽略未知差异后报告首次通过。

主消费者使用真正未改造 Apache POI Bug60337，实际产品不是诊断 C11。5 对源 / 产品比较含页面、全部 text / words / rawdict / texttrace、完整字符坐标 / bbox、drawings、字体、内容流、Poppler 正文 / bbox body 与完整 72 / 144 dpi 像素。该原件此前 1870→1800 twip、最多 14 pt 文字 / 17.5 pt 右边框差异在此次固定消费者测量中消除；不将这一结果推广到任意文档或消费者。整个 PDF 元数据 / trailer / 原始字节另记，不声称整个 PDF 字节一致。

主实验不等宽 / 长 token 控制中的 AUTO 与 FIXED 同样精确一致，因此无区分力。只追加一次预先冻结的加强控制：400/600/800/1000/1200 twip 列向量、明确短语，在相同内容的 AUTO / FIXED 中仍全部相同。4 PDF / 8 PNG 的源 / 产品通过，但区分判定仍为 `INCONCLUSIVE_NO_GEOMETRY_DISCRIMINATION_STOP_NO_VARIANT`；没有换阈值或继续搜索变体。加强短语 46 个非空白字符完整且在第二格实测边界内，最小 0.25 pt 中心线间距等于半个边框笔宽；软换行空白不冒称逐字符 PDF 保持。

因此原消费者计划的 AutoFit-vs-fixed 区分门禁没有通过。接受口径已明确缩小到正确 OOXML 首选语义、受测源 / 产品精确匹配与 genuine 原件改善；不证明 intrinsic fit、一般 AutoFit / 裁剪安全、动态分页、浏览器 / 原生 Word / WPS 保真。所有原始失败和两次 INCONCLUSIVE 结果留存，不把完整原计划写成 PASS。

## 证据定位

- 源 / 包：`tmp/release38-independent/candidate-pdh189jg/report.json`，SHA-256 `9084485c897f0d76b6132e6340c300f94e707dfbb85edb7c1daa89c7306b4882`
- 主消费者：`tmp/release38-independent/consumer-dgs54uz2/report.json`，SHA-256 `381b8681e8e8c17e2bb9abbca1e0e354d50e084d6f91c14aaf8c6812640c4f1e`
- 加强控制：`tmp/release38-independent/discriminator-consumer-lo7kjqjm/report.json`，SHA-256 `045c91c94aa4700edbc159ae42ef6747dc571dad6ea909e74d043695c5994d49`
- 编辑链路：`tmp/release38-editor/integration-xki_fykz/report.json`，SHA-256 `cf21f0d2c093aacd1e24da649c18a17231705730114b491d49f8ed11415b6db4`
- 前端：`tmp/release38-editor/frontend-final-_3_x3nui/report.json`，SHA-256 `b60a999bae71a37425a0b668a3bb3dba47ea707d3f65d36b417f68ae4eeaf0eb`

这些历史审计位置不是公开 portable 运行依赖。本轮未重放全部 1,894 历史源端点 / 187 行分页源，也未做 MSRV 的完整独立源消费者重放；不能以本轮 302 作者 ZIP golden 替代这些未运行门禁。本地验证已完成；各代际已完成检查和限制见下文。

## 最终源码代际、静态门禁与待发布状态

v1 stable CLI 为 `fc5378ee32242657020fee03993d0aae77be7bd1d3c43ed4b1d42093cf60e630`。随后独立审核了仅语法层差异；最终 v2 stable / MSRV CLI 分别为 `75cbc86fa3724f76db6447f83af20adaf216ae3e28020ff319449a5ba7029edd` / `55cdc4ab56d14e43fbe535e19d0d7ea0ce9a469f82e4ee6a26c7b9def43aaa9d`，不是与 v1 同字节二进制。112 调用行为桥及 14 产品整包精确相同支持已有有限证据的连接；这不是重新运行 v1 消费者、编辑器、全部历史 golden 或生产后端测试。

最终 stable 接受证据：`tmp/release38-regression-plan/executable-v1/stable-batch-v3-amortized/independent-stable-aggregate-v1.json`，SHA-256 `6053866a9f4d6c776c7ecea1f01d2fdd34c1da9f59b4675d1f914966f83aae2d`。实际 856 = 845 + 11，完整 47 阶段，664 源身份桥核对；原受阻 aggregate 和历史失败不删除。MSRV 全套接受证据：`tmp/release38-regression-plan/executable-v1/msrv-batch-amortized-final-v1/independent-1.88.0-aggregate-v1.json`，SHA-256 `f7337db36b149c47d2b890a19dd5a8d2b4b5d0e30dc59806f01edce882b23e16`。实际同为 856 = 845 + 11，325 library、47 阶段、零 failed / ignored / filtered，测试名称集合与 stable 完全相同；执行 owner 返回 0，终止收据 SHA-256 `99c6668fa5de1997b6dc287540fcc7f6a9b5faac44aa5220b82a7200b6d2a743`。

最终 v2 桥：`tmp/release38-independent/v2-bridge/attempt-2vt22ds7/report.json`，SHA-256 `4ea23a7ab75bb6fba5a69c059a4c193d755f2f917a71b113b4b929752d51ac5c`。原始 v1 1,018 调用、302 ZIP golden、24 旧作者比较与前端 / 消费者等证据不改代际标签。

最终静态检查：`tmp/release38-backend/preflight-bbwkja4g/report.json`，SHA-256 `540bc818233c379bbe71192c9279c23c8134a8dfb49c1d6d99aa1419ccfbf23e`。stable 严格原始错误限于 `clippy::chunks_exact_to_as_chunks` / `src/spreadsheet_xls.rs`，MSRV 限于 `clippy::uninlined_format_args` / `src/presentation_import.rs`；各仅允许对应已知单项后通过，原始失败保留。

v1 生产后端各 35 项：`tmp/release38-backend/backend-lbpzrg9_/report.json`，SHA-256 `79aa3978125ef3998d562a0376d99557bc13e0bff3ed1c7c2034a1704438ad7f`。原生探测缺少 glib-2.0、gtk+-3.0、webkit2gtk-4.1、javascriptcoregtk-4.1、libsoup-3.0、gdk-pixbuf-2.0、librsvg-2.0 七项依赖；未验收原生构建 / GUI / IPC，不以提取的生产 worker 测试冒充桌面运行。

公开包资格报告：`tmp/release38-public-bundle/runtime-v1/stable-6h3_yrgg/qualification.json` SHA-256 `36d4f896a06ee8aa4ff745bf85a2c8d9319071c8c425380d368b0fe3e0269548`；`msrv-bv8p9wfq/qualification.json` SHA-256 `d41175382479fb252154785b3179cb6e27c5885337ab380442684db273560349`。完整独立预期来自冻结源 binding，不来自候选输出；raw proof 校验 genuine auto/0、5 个 1870 grid 和 15 个对应 tcW。共享未改造原件及 Apache LICENSE / NOTICE，无旧私有二进制运行依赖；资格 harness 的旧 release37 负例不成为公开 runner 依赖。测试过的公开五文件保持原样，内部旧 draft / not-yet-qualified 标签作为冻结时状态保留，本节补充后续资格结果，不倒写原始测试输入。

两工具链全套已独立接受，本地验证完成；这一结论仅限上述验证范围。原始与加强两次 intrinsic 区分控制仍 INCONCLUSIVE，原消费者区分门禁未通过；没有因核心 / portable 通过而扩大该接受范围。
