# DOCX 段落最小行高（发布 40）

发布 40（2026-10-15）：本地验证已完成；有界 DOCX 段落最小行高的源 / 模型 / 编辑验证已通过；stable / MSRV 全套各 878 项已独立接受；最终生产后端各 35 项与实际 Open 运输链路也已独立接受。原严格消费者保真门禁失败并保留，当前仅接受最小行高语义和受测布局，不声明完整颜色 / 光栅保真。

正文顶层段落与标题 1–3 新增最小行高，按整数 twip 保存并导出 atLeast，独立于既有倍数行距。完整源继承和逐段身份通过才接受，旧硬拒绝不抹除；模式切换、草稿、可信重开及受控拆并保留。编辑 / 打印仍用自然行框并明确提示不模拟 Word 最小行高。真实 TestDocument 的 13.8 pt 最小行高恢复受测垂直几何，但源与产品的既有超链接蓝色差异及 PDF 标记流差异仍在。

## 支持范围与作者模型

新增 `minimumLineHeightTwips`：只允许 doc 直接子级 paragraph / heading 1–3，内容仅 text / hardBreak，整数 1–31,680 twip，即 0.05–1,584 pt，0.05 pt 递增。这是产品子集上限，不是 OOXML 极限或页面适配保证。与 `lineSpacing` 按存在互斥；null、布尔、字符串、分数、越界及错位置拒绝。仅编辑器 schema 默认 null 在规范化时省略，原始模型 null 不放行。

旧 `lineSpacing` 仍是 1/240 行的 Auto 倍数。276 twip 最小行高等于 13.8 pt，不等于 Auto276，也不等于固定行高。DOCX 在同一 `w:spacing` 中写 `w:line="276" w:lineRule="atLeast"` 并保留段前 / 后；旧模型省略新增字段时仍保持原字节与 profile 默认值。最小行高不支持列表 / 表格内段落、图片段落或 root `splitPageBreakAndParagraphMark=true`；作者模型其他位置独立 pageBreak 兄弟块可保持，但外部源证明是更窄的完整简单正文子集。

## 同源继承证明与旧拒绝

同一不可变包字节先经实际 OPC、部件 / 关系唯一性、Content Types、规范命名空间与属性身份核对，再解析 docDefaults→所选段落样式 basedOn 从根到叶→直接属性。无显式样式时才选实际默认段落样式，不把默认样式插到无关显式样式之前。缺失 / 重复 / 错类型样式、循环或旧 20 级限制超限拒绝。

段前、段后与行距独立继承，行值和规则为一对；只有新 line 而无 lineRule 表示 Auto，不能继承旧 atLeast 规则。只有 before/after 保留继承行对，显式零段间距覆盖旧值；rule 无 line 拒绝。有效继承 atLeast 被有效 Auto 覆盖后不产生最小模式或新警告，但活动链上的损坏 / 不支持 Exact 声明不能因被覆盖而忽略。

未知规则、负 / 零 / 越界最小值、非法单位、自动或行单位段间距、活动 / 未知网格、未支持 contextualSpacing 依赖、修订及原有间距硬拒绝仍有效。明确无效应的 contextualSpacing=false 可通过。完整正文无 tab 字符 / 元素、无活动缩进 / 编号或 tab 消费者时，只允许一般的合法非负有限 left tab stop / 缺省 leader；不是针对原件 709 坐标写特例，损坏或有依赖仍拒绝。

源证明只在一次 audit/import 内生效，核对同源 raw→IR→最终模型的完整段落顺序、文字 / hardBreak、既有标题身份、段间距及 Auto/AtLeast 类型。请求新增能力却身份失败时硬拒绝 UnsupportedSpacing，不静默转换倍数；旧 scanner 仅允许完整证明覆盖的声明，不在扫描后删除汇总拒绝风险。源含列表 / 表格 / 图像、编号等不在这次完整正文证明内。

实际保留最小值才新增风险：

`Minimum paragraph line height preserved; actual line layout and pagination remain consumer-dependent`

原字体、链接、颜色、页面、分页及其他风险不删除，fidelity 仍为 false。普通未证明转换不能借用公共布尔开关获得放行。

## 编辑、预览与拆并

行距模式提供默认 / 倍数 / 最小 / 混合，切换一次事务清除另一模式；“默认行距”只重置两种行模式，段前 / 后保持。撤销 / 重做、草稿恢复、标题转换、合法 owned HTML / 剪贴板元数据保留精确值，非法或不支持结构不能静默掉字段。

Enter 把最小值复制到拆出的两段。合段仅在有效行模式和值相同才允许，缺省按当前 profile 求值；普通文字编辑和整块删除允许。移入列表 / 表格或其他不支持结构须先明确重置，不匹配合段整次拒绝且不污染草稿 / 历史。验证包含实际控制路径与键处理，但不是原生键盘验收。

界面提示原文：`最小行高按原值保存到 DOCX；编辑区和打印采用自然行框，不模拟 Word 最小行高或分页。` 不把 CSS 固定 pt 行高冒充 Word minimum；自然预览不保证所设最小距离，也不保证真实浏览器 / 打印与 DOCX 消费者一致。

## 真原件、布局收益与未通过的严格保真

Apache POI TestDocument.docx，10,861 bytes，SHA-256 `527a906b3cc31d0a72a47290a6c8a53854a89a31178a60fa3cb314d70b57a413`，固定 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`。五段显式 Default 样式：docDefaults 为 Auto276 / after200，活动样式覆盖 atLeast276，最终是 13.8 pt 最小行高加 10 pt 段后。原件字节保持；再分发权未审完，不提交二进制。

旧产品对原件因 UnsupportedSpacing 拒绝。早期仅把样式 atLeast 改成 auto 的独立诊断成功导入，但它不是原件或语义等价修复。新原件导入保留完整五段及粗斜体、颜色、Calibri 字体声明、链接、页面 / 分页属性；固定云端消费者仍以 Carlito 替代 Calibri。

原件与新产品最后链接行基线均 178.25 pt；旧39 Auto276 对照为 184.5 pt。源 / 产品文字及测量 bbox 几何一致；大 32 pt 字体 / 六行控制中 Minimum 与 Auto 明确区分，行距约 39.05 / 44.9 pt，并完成有限裁剪检查。这支持受测布局与 minimum 语义，不保证任意字号、页面、Word/WPS 的布局。

原严格消费者报告 `CAPTURED_UNMET_GATE_STOP_RETAINED` 未通过：源链接字形与下划线为 000080，产品为 0000FF，并有 Default→Standard 的 PDF 标记流名称变化；完整字段 / 整页像素不等。原件 raw InternetLink / theme 声明为 0000FF，固定 LibreOffice 对源链接有特殊样式消费，不能据此任意硬编码改色。旧39 / 新40 对应链接 XML 相同，stress Auto 整包 ZIP 相同；另做真实旧39消费者对照，旧新字形与下划线 RGB 相同，确认颜色差异先于本次最小行高改动。该归因不使原失败变 PASS，不删除色差 / 流差异或原始光栅；本轮明确只接受最小语义和受测布局。

## 实际验证与保留限制

- 前端 807 个不同检查 = 原 774 + 新 33，TypeScript / build 通过；此前 799 通过 / 8 失败的 schema-null 预期校正及原失败保留
- 编辑到冻结 CLI：38 调用、12 状态，完整独立预期模型 / 草稿、trusted model、逐项 XML 与 12 次不覆盖拒绝通过；这是 headless 控件 / 键处理与运输验证，不是真实浏览器 / 原生交互
- 独立源 / 包：978 调用、32 源端点、10 产品和 302 无新增属性旧作者完整 ZIP golden 通过；不等同消费者或 UI 验收
- 核心 72 targeted 执行及两工具链构建通过，格式和 scoped Clippy 通过；strict 原始既有 lint 失败保留。完整 stable / MSRV 各 878 项 = 原 856 + 新 22，342 library + 536 integration，47 阶段含显式零测试 doctest，failed / ignored / filtered 均为 0，测试名称集合相同并已独立接受；不是 1,756 个不同测试。最终生产后端各 35 项 / 实际 Open 链路已独立接受，范围如下
- 原严格消费者门禁 FAIL：源 / 产品完整文字和 bbox 几何精确相同，32 pt 大字 / 多行控制可区分 Minimum 与 Auto（实测行距 39.05 / 44.9 pt），受测未观察到裁剪；超链接字形 / 下划线源 000080、产品 0000FF 及 PDF 标记流名称差异导致严格字段 / 光栅不相等
- 单独旧39对照证实蓝色差异先前已存在，不修复或豁免原严格失败。接受明确收窄为最小行高语义和这些样本的布局；不称完整消费者计划 PASS

UI 失败记录保留：原 799/8 是七处 raw schema expected-object 增加默认 null 的校正，不改变持久模型或 raw-null 拒绝；集成先后暴露 JSON-wire 对照与旧 schema mark 顺序、恢复 NodeType 使用不同 schema，以及 python-docx 把 Auto1.5 标为 ONE_POINT_FIVE 的检查器差异。各自独立旧基线 / 实现证明后修正验证工具，不从失败候选输出构造预期；最终 38 调用的 generator 版本在 807 后单独接受，不冒称它已被此前 807 执行。

无本轮真实浏览器 / 原生 GUI、系统剪贴板或物理打印验收。两工具链完整 Rust 全套各 878 项已独立接受；最终后端与实际 Open 运输结果也已独立接受；验证范围不扩展到原生 GUI。

## 证据定位

- 前端：`tmp/release40-frontend/attempt-8qjwbnil/report.json`，SHA-256 `ad812a8c205891c388341bde172f2ed2e8c767776d012d6ecf7e911de6f84f79`
- 独立源 / 包：`tmp/release40-independent/candidate-plan/candidate-9z_kc_zh/report.json`，SHA-256 `e0c599c779cb363d0a972548db131b0c0aed3a168ac239ce98c8df88ee0c07d8`
- UI 38 调用：`tmp/release40-ui-integration/attempt-a5i_zwgx/report.json`，SHA-256 `6fc95eaed9059cfe0650d674faf8dc9068f90de537d5aa6391ab032904c3f31a`
- 原严格消费者失败：`tmp/release40-independent/candidate-plan/consumer-5p614tv_/report.json`，SHA-256 `ea067c9693d1aea86e050d1866e9213f859d7383e745f4617b9053fc086511ca`
- 旧39颜色归因：`tmp/release40-independent/candidate-plan/old-consumer-attribution-1fx_e5gs/report.json`，SHA-256 `1149f3e39186fd84270433eb519f649d91435749f2d85d7a38ffdc363c1cff43`
- 核心定向：`tmp/release40-core/TARGETED-RESULT-v2.json`，SHA-256 `52b9c10412b9460cf8acf116973be24013838198ddf735f94df35f47346f2571`；strict / scoped 差别见同目录 `CLIPPY-RESULT.json`

## 完整 Rust 与最终后端回归已接受

stable / MSRV 1.88.0 各 878 项（856 + 22），各 342 library + 536 integration，47 阶段包含 46 harness 与显式零测试 doctest；failed / ignored / filtered 均为 0，两工具链完整测试名称相同。这是串行无筛选 Cargo target 覆盖，不是一次字面 cargo test 命令；独立原始覆盖 aggregate 给出接受，原 driver aggregate 阻断记录不改写。

stable：`tmp/release40-regression-plan/executable-v3/stable-batch-cached-final-v1/independent-stable-aggregate-v1.json`，SHA-256 `2b9b065f07b8eccc796d5d845951539a2a36160d30748d8e5aa9e37ded240b14`。MSRV：同 executable-v3 下 `msrv-batch-cached-final-v1/independent-1.88.0-aggregate-v1.json`，SHA-256 `950a5a74e0111701bb584b8ba52b9d271719e09beae2eecfddeb0a5c951536e5`。

最初容量门禁停止与原日志保留。经审查后采用分阶段阈值：编译 / doctest 入口 1.5 GiB、增长上限 768 MiB；仅执行入口 950 MiB、增长上限 240 MiB；共同可用空间底线 600 MiB。派生缓存元数据优化减少重复盘点，不改变测试名称、覆盖、源 / 工具 / 冻结证据保护或拒绝规则；不能描述为降低测试范围后通过。

最终生产后端提取测试 stable / MSRV 各 35 项通过；实际 stable 生产 worker 的 16 个完整 import/Open 源流程和 10 个产品，合计 92 调用通过。10 产品与此前接受包逐字节相同，完整 trusted / authored 模型及不覆盖核对通过；26 个非零结果均为预先指定的拒绝，不是忽略失败。这里没有新增 native GUI 成功、重新 portable 资格验证或严格消费者保真通过的声明；原颜色 / 标签 / 像素门禁失败继续有效。


## 最终后端与原生边界

生产提取测试并非桌面 GUI 测试。stable 35 项报告 `tmp/release40-backend-plan/final-v2/stable-tests-dffy0221/report.json`，SHA-256 `b4ec250778cda3e8cbfa1d74394a6b10db0aa111db5bd408650f1c268f08e38c`；MSRV 35 项 `msrv-tests-4sfwblel/report.json`，SHA-256 `a7e0a5c69c309d1f6bb7ce57c2f74df28a0626d546a64c4bb9ba5ed972001664`。

旧32调用完整绑定报告 `baseline-3yxuarne/report.json`，SHA-256 `34764b0ae50937092e6d5cbff4e96771db9fe995a084f80357b4508b6a37df03`；其原始“待审”标签保留，后续独立审查已接受。实际92调用报告 `candidate-60nnsooq/report.json`，SHA-256 `a5ef6f9c2586c2724fc155a2e3d8dac90c907d6c0a24e4bce799ae00cb1355b5`。完整16源 import/Open 及10产品的运输 / 包 / 模型证明仅豁免有效 u64 elapsed_ms 时间字段，原始响应保留；不等于真实 Tauri 对话框、IPC、WebView 或原生显示。导出通过冻结 CLI。

原生依赖探测 `native-0ww6jmgg/report.json`，SHA-256 `4f1a656a9c01de2baf6006f9b204877a508f1536fbe237790f087737a2a5e6fa`：七项 pkg-config 依赖全部缺失，未运行原生构建 / GUI。以上相对报告均位于 `tmp/release40-backend-plan/final-v2/`；原始文件与失败日志不改写。本轮没有新的 portable bundle / 资格验证声明，也没有再分发真实原件。
