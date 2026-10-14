# 当前跨格式回归记录


## 本轮：正文顶层段落分页属性（2026-10-14）

基于 `9991dd79d5d1ad400ce06c9093868dcf990c2ae9`，新增 `keepNext`、`keepLines` 与 `pageBreakBefore` 的严格可选布尔模型、直接 DOCX 输出、受审计的默认 / 活动段落样式链 / 直接属性继承，以及正文 / 标题编辑控件。显式关闭和缺省分别保存；旧作者无属性模型不升级。源属性保留为固定值，打印仅是 CSS 提示，不能称为自动分页引擎。具体编辑、合段、富文本粘贴与未支持上下文见 [PARAGRAPH_PAGINATION.md](PARAGRAPH_PAGINATION.md)。

### 最终代码门禁

- **Rust 1.99 / 1.88 各 629 项核心测试通过**，原 601 + 28；包含严格三态模型 / 草稿 / 可信重开 / 篡改 / 不覆盖、所有 on/off 拼写、逐字段 false 覆盖、实际样式部件、20 / 21 层链界限、缺省类型与重复 / 缺失 / 循环活动身份、正文顺序绑定以及空文档 / 分栏的旧路径。日志 `tmp/paragraph-pagination-core-{stable,msrv}-final1.log`
- **622 项无浏览器前端检查通过**，原 575 + 47；完整扩展链、锁定 / 混合状态 / 重置、格式转换、Enter 属性归属、精确历史、兼容合段与单段行内富文本粘贴覆盖。parent 在精确冻结后运行全部检查及 prepare-pdf / TypeScript / Vite bundle，均退出 0。**7 个浏览器规格只发现，未执行**
- 前端门禁 `tmp/paragraph-pagination-ui-parent-final-gates.json` SHA `387c9f7714b6ef43e86211cdcf6e35baa2298a5382ffe08b00bc4d673e2b3789`；369 个源码 / 测试 / 配置文件在 `tmp/paragraph-pagination-frontend-freeze.json` SHA `c50f1dd39d28c2288b900f0b545b8b52551d134ff352b92036aab260729df543` 中记录，门禁后逐项不变
- 两端 CLI build、fmt、`git diff --check` 与 scoped all-targets Clippy 通过。原始严格 Clippy 仍各只有原有单项：stable XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`；对应命令仅排除该既有项，无新增豁免
- 核心门禁 `tmp/paragraph-pagination-gates-stable-final1.json` SHA `35f9e0a23bac2296e9a5c071a8c72ff8bf9a91214eece28e9cb970a0a160d89f`；MSRV 对应报告 SHA `df14d7b85c3b5310d112ee74de7b44e02cb0fcec354ee3b72f23f7cd4b8b3cf5`。报告包含精确 argv、工具链 / target、每条日志、源码身份及前后不变检查
- 最终 stable CLI 使用现有 `target/debug/rust-wps`，SHA **`35d4b018e67a032c619938c62765fbeb1eaa6e09126240cbdc8448dcfa75bf47`**；没有另存大型中间二进制。最终 source-proof SHA `0f376678572f62967dee616e871b2ad4761a1e6117a925f328413636f14f07aa`，writer/validation `src/document.rs` SHA `98d1afe8ead414bf53d2cb3db98d25b1435b56b912aa22acf29dd8338dbc315c`
- `CARGO_INCREMENTAL=0`，既有 `../toolchains/{cargo,rustup}`，stable `target` / MSRV `tmp/msrv-target`，单个串行构建、无 RUSTFLAGS / profile 覆盖、无依赖 / 锁文件 / vendor 改动。前端仅本机已安装工具，不重试被拒绝的 registry、浏览器或原生路线

### 真实原件、独立源 / 模型安全与历史契约

- 两份未改造 Apache POI 回归原件来自已固定 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`。[Styles.docx](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/Styles.docx) 10,905 字节，SHA `63f3fe4c3b4fbdc211eca8fa7fd31a799ff4438afbf4ca6e771b3aef107c732e`，Git blob `27f66f48bff955494065233a3980fb65793b2e2f`；[NoHeadFoot.docx](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/NoHeadFoot.docx) 10,380 字节，SHA `dc6a67b1c118658acadc87ba41200f0b90ead05ada95e7ce6fa72a97700430c5`，Git blob `7af2a14583d09c337a7993fb744c33e31e28d687`
- 原件分别有 3 个 / 1 个活动标题样式含 `keepLines=true`，旧导出全部丢失；新导入 / 导出显式保留。它们原有短标题没有已证明的可见重新分页回归，不能把语义修复冒称原件页数修复。原件只用于本地验证，未改写 / 重新保存原件，不作为新增用户生产文档或已审查再分发授权
- 最终独立源 / 安全报告 **23 支持 / 28 拒绝、36 非法模型、27 篡改包**，有效值与直接 XML 逐项独立解析；完整模型可信重开、已有目标不覆盖和全部来源身份通过。报告 `tmp/paragraph-pagination-independent/run-xou7q6ib/report.json` SHA **`42809614e92fc199b9752f94c51e6c6f0639e8108d35a6f4ee60dd7accc75846`**
- 同一报告重放 **302 份不同历史无新属性 DOCX 整包金样**：旧冻结 CLI 与最终 CLI 各自导出均与原金样逐字节相等；每个 CLI 各 604 次完整可信重开、604 次原件 / 输出禁止覆盖。合计源检查与历史检查共 **1,231 次完整可信重开**，不能当作 1,231 份真实来源或新 PDF
- **1,520 条历史 read 命令**完成初次双 CLI 响应对照和最终 CLI 的全部新调用。最终每条原始响应与受保护的已审查临时版本响应相同：相对旧发布为 **587 精确 / 933 有界变化**（505 审计提示、425 固定有效布尔值及提示、3 条原已拒绝来源的严格证明诊断）；该目录内 **零新增导入拒绝**。所有其他模型字段、来源和旧报告不变
- 原始历史对照报告 `tmp/paragraph-pagination-legacy-replay/run-cojf1ls6/report.json` SHA `a1a6ffc5d78166bc846306cb6f01102224e57c9533b4f48777b77170b1892b02` 保留初次分类器的 REVIEW_REQUIRED 状态；独立 XML 复查只增加 `reviewed-acceptance.json`，SHA `794bda68e965cd2bd527efe686e0af5d0b1efe9a6f2b19a4f69e0b260edacf31`。最终新 CLI 1,520 次原始响应复核报告 `tmp/paragraph-pagination-legacy-replay/final-candidate-fb0vh7mk/report.json` SHA **`09e0c280690b477884c65d330c9dec646717858d18b119bddb3cd07da5411bbb`**，明确复用冻结旧响应，不冒称再次执行旧 CLI 或重新渲染旧 PDF

### 新独立消费者与已有颜色限制

全部 21 对源 / 参考控制已完成新隔离 profile 转换：完整逐页文字、换行与横纵坐标都精确相等，最大差 0.0 pt；其中 18 对整页像素严格相等。覆盖三项 true / false、keepNext 连续链、111 行超长 keepLines 段落、文档起点、手动分页相邻、直接 false 覆盖样式 true、样式 false 覆盖默认 true，以及显式样式不叠加无关默认样式。15 项预先指定的分页边界断言另行通过；超长 keepLines 仍可跨页，未将约束写成不可拆分保证。

两份真实原件和未修改的原始继承控制仍有**源 / 输出光栅失败**：Styles 501 像素、NoHeadFoot 第一页 824 像素、继承控制 849 像素。源主题 `accent1=4F81BD`、`themeShade=BF` 的消费色为 `365F91`，既有导入固定色为 `376092`。源 / 当前完整文字、换行、位置和逐 span 其他字体属性相同；当前模型只去除这三项分页属性后严格等于冻结旧 CLI 的模型，旧 / 当前字符 `rPr` 相同。两真实原件另新渲染旧输出，旧 / 当前整页像素与坐标精确相等。原始继承控制的旧分页本就不同，所以没有要求或声称它旧 / 当前完整布局相同。

这个例外只匹配三份精确 SHA 与固定旧 CLI，保留原始 `strictly_equal=false` 和 FAILED 分类，不提高阈值，不忽略任意颜色差异。另生成独立标注的黑色继承派生控制：仅把活动 Heading1 颜色改为固定黑色，保持原始来源不动，独立逐段分页 / 文字 / 类型 / 页面期望不变；其源 / 当前完整像素、文字与坐标严格相等。前端参考与其他控制均不使用颜色例外。

最初 Styles 严格光栅失败报告 `tmp/paragraph-pagination-independent/run-kwkljhcq/report.json` 保留。随后完成 61 份新 PDF（21 对源 / 输出、3 份旧颜色见证、8 对前端参考 / 输出）后，因下一份预留会越过 650 MiB 磁盘边界而安全停止；报告 `tmp/paragraph-pagination-independent/run-1m3zyl7_/report.json` SHA `1a70b826aba1cd729d7777280891beacdeb006315292b0193ae7d8f6f1ad2055` 保留 FAILED 状态。没有降低空间边界或重新宣称其为完整通过；后续只复用已验明身份的完成项并继续缺失消费者，最终计数另列。

实际前端快照来自完整扩展链的 46 份候选操作 / 历史 / 草稿文件；manifest `tmp/paragraph-pagination-editor-final-DiqPBg/manifest.json` SHA `dc1be5d15854f96d60f7d0eeef26603a22a7170feb66b98bd7fe56d4f7581090`。独立根据初始模型和 46 条操作配方计算完整预期，并检查 34 对历史 / 草稿等价关系，不能仅比较快照与自身再输出。

最终主消费者报告 `tmp/paragraph-pagination-independent/resumed-u44hswfl/report.json` SHA **`e65ab364d52c77e1bd51295832d382849d25f40f2391d68f4917755c3d7ae58e`**，状态为 `passed-with-known-source-raster-failures`：**137 份新消费者 PDF / 各自独立 profile**，其中 61 份来自受保护的部分运行、76 份来自新目录恢复；恢复时仅重验前 61 份的文件身份与比较结果，没有重新渲染或重复累计。**67 对中 64 对完整像素 / 文字 / 坐标精确相等，3 对保留上述源颜色失败；全部 46 对前端参考 / 输出均严格相等**。515 份恢复保护文件和消费者可执行文件前后不变。主报告计入另外标明的黑色派生控制，支持来源数因而为 24，不冒称 24 份真实原件。

主 checker `scripts/check-paragraph-pagination-independent.py` SHA **`95046da4104a7f5e605d549974c44f33b6842f513aebdcbc30207e57e26c031c`**。复现调用如下；各报告保留精确 Python 路径、完整 argv、消费者版本 / 二进制哈希和新 profile 路径：

```sh
python scripts/check-paragraph-pagination-independent.py --cli target/debug/rust-wps --expected-cli-sha256 35d4b018e67a032c619938c62765fbeb1eaa6e09126240cbdc8448dcfa75bf47 --historical-cli tmp/list-geometry-final-cli
python scripts/check-paragraph-pagination-independent.py --cli target/debug/rust-wps --expected-cli-sha256 35d4b018e67a032c619938c62765fbeb1eaa6e09126240cbdc8448dcfa75bf47 --render-report tmp/paragraph-pagination-independent/run-xou7q6ib/report.json --editor-models tmp/paragraph-pagination-editor-final-DiqPBg --historical-render-cli tmp/list-geometry-final-cli --render
python scripts/check-paragraph-pagination-independent.py --cli target/debug/rust-wps --expected-cli-sha256 35d4b018e67a032c619938c62765fbeb1eaa6e09126240cbdc8448dcfa75bf47 --resume-render-report tmp/paragraph-pagination-independent/run-1m3zyl7_/report.json
```

恢复前由 parent 重验并删除三份已结束、无后续用途的 MSRV Cargo 测试缓存可执行文件，保留其源码 / 命令 / 日志、全部生产 CLI / probe / rlib / fingerprint 与证据；审计 `tmp/test-cache-cleanup-20261014T011800Z-{before,after}.json`。没有删除失败证据或恢复中间产物。实际查看真实原件、五行 keepLines 的第二页、段前分页后的第二页和前端合段 / 行内粘贴图像，均有真实内容，不是空白图像假通过。

独立 reviewer 另指出普通段落边界控制不足以证明标题缺省的覆盖，因此追加**两个真正 Heading1 的 true / false 边界控制、4 份新 PDF、2 对严格像素 / 全文 / 坐标相等**。两件分别独立构造，统一为明确黑色 / 字体；实际导入节点必须是 `heading level=1`，并保留显式 `keepNext=false`。开启时 HEADING ANCHOR 与 HEADING FOLLOWER 同在第二页；关闭时标题留在第一页（y=748.590764 pt），后段在第二页，证明关闭覆盖应用标题的与下段同页缺省。另有 2 次完整可信重开、4 次源 / 输出不覆盖；全部身份不变。

补充报告 `tmp/heading-pagination-boundary-independent/run-uwhhf99_/report.json` SHA **`a669869e1cff0b64e9103ffae922400c9e7e261f43a9135e200c13e24167d906`**；脚本 `scripts/check-heading-pagination-boundary-independent.py` SHA **`bda362a76bde4b2c9ea73662eed57ed61a0c32095440a12a0469d653bd2d6168`**，严格绑定冻结主 checker，没有改写前一报告。调用：

```sh
python scripts/check-heading-pagination-boundary-independent.py --cli target/debug/rust-wps --expected-cli-sha256 35d4b018e67a032c619938c62765fbeb1eaa6e09126240cbdc8448dcfa75bf47
```

主消费者与补充验收合计 **141 份新 PDF、69 对完整比较，其中 66 对严格像素相等，3 对保留源主题颜色失败**；全部比较的文字 / 换行 / 坐标精确相等。该合计不重复累计恢复文件，也不把原始失败探针 PDF 或旧历史 PDF 算作新的成功验收。

### 生产后端运输与独立审查

- **35 项 Tauri-free 生产后端测试在两工具链各通过**；原 `scripts/pdf-rotation-probe.rs` 未改，分别链接本轮核心。精确 rustc argv、源码 / 依赖 rlib 身份及日志在 `tmp/paragraph-pagination-backend-{stable,msrv}-test-provenance.json`。因磁盘配额，parent 仅在执行结束、哈希复验和无待执行用途确认后删除这两份可重建的测试专用可执行文件；源码、日志、来源、旧冻结生产 CLI 和新生产 probe 保留
- 最终生产 probe `tmp/paragraph-pagination-worker-probe` SHA `0e3e4b7de105245fa9d3bc3e213191873635dace9ed0cfaa2f4e5d0890ca7a92`。通过真实 `worker::run` 子进程核对 **4 份完整导入报告、4 份完整可信重开模型、2 份源声明拒绝**，包括两个原件、keepNext 显式关闭与段前分页控制。报告 `tmp/paragraph-pagination-worker-evidence-7c14a99i/report.json` SHA `93c1b3110a0b20dd1ff7aa150bcca632cbf6ae6357a5cd0be590d8edfce5ff32`；来源 / 输出 / CLI / probe / 独立报告身份均不变。这不是原生 IPC / 系统对话框验收
- 独立 reviewer 最终 CLI 检查 **49 个来源**，其中 16 保留 / 33 拒绝；32 个作者布尔向量模型的重导出、完整重开和不覆盖，以及 32 个段落 / 标题 × 对齐 / 缩进 / true-false 的 XML 顺序控制通过。报告 `tmp/pagination-independent-review/final-y_aoogr2/report.json` SHA `c4251e31e50dfd58848bc05fc313842a4224a69d633aa98d414f801cacfc4468`
- 独立前端 reviewer 完成 **2,916 个有序三态合并组合**（396 允许且精确合并 / 撤销 / 重做，2,516 拒绝，4 走原无属性路径），以及 **1,248 个单段行内粘贴组合**，还核对后续标题 Backspace 的输入规则撤回。报告 `tmp/pagination-independent-review/ui-5R1u0E/report.json`；这些定向矩阵不加进 622 测试计数

### 有界语义和早期失败记录

初次全量核心发现旧外部导入期望缺少三个有效布尔值，已逐项补全期望，没有删除新证据。缺失活动样式 / basedOn 的旧规范化夹具保留为明确的新分页证明拒绝；其独立颜色与上下文正对照另用完整样式证明继续验证。原件和失败日志保留。首次 Clippy 的新增嵌套 if 已修正并完整重跑，没有增加 lint 豁免；新分页属性对应的段落 XML 采用 keep 标志 → spacing / ind → jc 顺序，无属性旧字节路线原样。

最初前端 guard 拒绝全部带属性的合段和开放富文本切片；由于导入段落均有三个显式值，这会影响普通编辑，未把它作为最终行为。最终支持其他段落属性一致、有效分页值一致且后段不含段前分页的相邻同类段落边界合并，逐字段保留两端显式值；单段开放行内切片保留字符标记与目的段落属性。不同属性、部分跨段删除、复杂结构 / 上下文迁移仍拒绝。独立 reviewer 的三态穷举、真实候选事务与历史矩阵另行记录，不累计进 622 单元测试数。

首次独立 checker 对重复活动样式的负向控制错误要求 `audit-docx` 返回 JSON；实际公共解析器已提前拒绝。该失败报告保留，修正的是 checker 的负向断言，不是放宽产品 guard。

真实 WebView2、键鼠 / IME / 系统剪贴板 / 系统对话框 / 原生打印以及 Microsoft Word/WPS 未验收。保留列表 / 表格 / 图片的原审计与规范化边界；没有完整命名样式动态关系或一般 Office 分页 / 版式保真承诺，DOC/DOCX/PDF 阶段仍未完成。


## 本轮：保留编号列表的有界删除（2026-10-14）

基于 `c1221bd6d78a18fd19830d3b6366adb5d752e642`，增加应用注册的 Delete / Backspace 精确整项、整列表、整文档选区删除。真实边界选区移除完整项壳，不合并不同格式的部分项；剩余项完整保留原内容/标记/段落几何和独立编号字体，列表 `start` 保持、剩余项连续重新编号。全删留下默认空正文，根 profile / 页面不变。准确选区规则、程序入口与保守范围见 [有界列表删除](LIST_DELETION.md)。

### 最终门禁与源码身份

- **575 项无浏览器前端检查通过**，原 548 + 27 个新删除模型/完整扩展链检查；最终全量 26.6 秒，日志 `tmp/list-deletion-ui-parent-final-aggregate.log`
- prepare-pdf、TypeScript 和 Vite build 全部退出 0，保留既有 bundle 大小提示。最终门禁只用已安装本机二进制、显式无浏览器配置和 `USERPROFILE=$PWD/tmp/linux-home`；准确 argv / 返回码 / 日志哈希 `tmp/list-deletion-ui-parent-final-gates.json` SHA **`5e5280b5b55baf051f11780336f98ea90be1c0a67b263d7088a93bc8df31e53b`**
- **6 个新浏览器规格仅发现、未执行**，覆盖首/中/末项、整列表、整文档和部分跨项拒绝；日志 `tmp/list-deletion-browser-discovery.log`。实际键盘、选区同步、系统剪贴板、磁盘草稿和原生打印没有新增验收
- 27 新检查含首/中/末/唯一/空项、正反选区、非 1 起始值、全部文档属性、终点外部段落、完整存活节点/草稿/撤销/重做、只读/锁定、完整扩展链、立即输入前后历史隔离，以及跨位置复用同一不可变节点对象和伪造/篡改事务拒绝
- **271 个 Rust 核心 / 后端 / vendor / 依赖配置与 Rust 测试文件逐字节等于基线**。身份清单 `tmp/list-deletion-source-identity.json` SHA **`828aac4887b6a13a742f0e198a3fcace00abaeb52335f8a9cbd3f02d8d4edae0`**。本轮不重新构建或运行 Rust / 后端；上一发布的 Rust 1.99 / 1.88 **各 601 项核心、各 35 项后端**结果明确作为历史未变源码证据复用，不能称为本轮新运行
- 冻结 CLI 沿用 `tmp/list-geometry-final-cli`，SHA **`5c008894209311c84515a6432b6e973a120ad459b30c848ea019c2128808111a`**，没有制作额外大体积二进制副本、修改依赖/vendor 或清理文件

冻结前端产品：`ui/word-list-geometry.ts` SHA **`384ba8daddcebd46037113cf3679ef738162d1081d87fdb2553f0fdb1079fd6f`**；模型诊断 `ui/word-list-geometry-model.ts` SHA **`af0b7ee356587c4fe13f5125dff593cf20755a3c8d8ffa6026e8b682904fd1e8`**。候选的私有源/最终文档身份例外不放宽通用连续性检查；另检查项尾边界，阻止原始删除将外部不同格式段落并入末项。

### 实际前端快照与独立文件消费

```sh
node tests/ui/edit-list-deletion.mjs tmp/list-geometry-first-import.json tmp/list-deletion-editor-original-2
python3 scripts/check-list-deletion-independent.py --cli tmp/list-geometry-final-cli --editor-models tmp/list-deletion-editor-original-2 --render --root tmp/list-deletion-independent
```

快照/报告目录均须使用新路径，脚本拒绝覆盖既有结果。原件是上一轮同一份未改造 LibreOffice `testParaListRightIndent.docx`，SHA **`48209f95d7a2ddfbf0fb3291658b1463e7b8121dac982f26501d8775e5450707`**，不是新增用户文档。真实两项的中间删除控制由应用 Enter 产生第三项；起始 7、相同项复用和混合字符样式单独标为作者模型。

- **12 个实际候选操作、52 份前端快照、54 次完整可信模型重开、57 次已有目标保护、2 份整包字节金样**通过。独立按 before 模型和删除索引计算预期，检查全部存活节点/页面/编号；undo 精确等于 before，redo/draft 精确等于 after
- **12 个不同模型、24 份新 LibreOffice PDF / 24 个隔离新 profile、12 对严格坐标与整页像素对照全部相等**；最大横纵差 **0.0 pt**、改变像素 **0**。其中 11 对非空、1 对空白清空结果。另 40 个相同模型状态只核对完整模型/导出字节并显式链接已有渲染，不累计为新 PDF
- 首项删除后的真实第二项保持编号 x=170.20 pt、正文 x=283.60 pt、18 行正文，编号从 2 变为原 start 1。作者控制保持 start 7、剩余标记 7/8，正文 Times New Roman 13 pt、粗/斜体、下划线、删除线、RGB `205090` 逐项保留；所有标记仍来自真正 numbering，不转文字
- 独立 python-docx/OOXML 参考不调用应用 writer 构造；直接读取实际编号、文字 run、样式、段落和页面。代表性原件存活项、start 7 字符样式和清空页面已实际查看，不靠空白截图通过
- 原件无改动；原件 no-op 导出等于上一轮 `unchanged-original-export.docx`，普通无新属性列表导出等于历史整包金样；没有重跑上一轮 1,520 响应 / 302 包的完整历史目录，也不将旧 325 PDF 称为新渲染

报告 `tmp/list-deletion-independent/run-rfa4t588/report.json` SHA **`5b32df1ed91168c67a2cb9aa6a059a0a2646c709bcccbbf52b2b02949f85aac9`**；最终 checker SHA **`122a0f0ce8e61b63e4043684258f531e0633f60c563b4485ee29f465669d83ea`**；快照 manifest SHA **`352687962d94a3b0c0607e3d7f467ff79d7d8be31bd0fd327bae1d6fed407153`**，生成器 SHA **`de8733337d97cdba02d000394731793b6c505867e31ac97e3378e2c5e52be69b`**。报告逐一核对源、CLI、产品、生成器、输入及历史证据前后身份。

### 独立审查与限制

独立 reviewer 的 **423 个有界检查**通过：256 对合法文字端点穷举（12 允许 / 118 留给普通项内或外部路径 / 126 拒绝）、16 个跨位置复用节点、36 个节点选区历史、96 个原始跨块拟合删除与其他真实源/事务/锁定控制。报告 `tmp/list-delete-review-s6vot0zh/report-third.json` SHA **`812530deca49027ec6a0d3e14af5fe26182ad0d76390f32b8c04b8e972982301`**；这不是 575 的附加单元测试计数。独立 consumer checker/报告另经相同 reviewer 的 hash、模型谱系、XML、别名和图像核对。

早期 reviewer 发现“两个位置复用同一不可变列表对象”会绕过同表判断，以及紧邻输入/删除会合并历史；均修为实际位置证明和前后历史边界，并加入回归。项尾保护最初使用错误映射关联导致合法间距变更被拒绝，已按边界语义修正并复测原 24 项列表检查；测试中的跨 schema 节点比较、无 DOM 的字符串插入和已有自动补尾段历史计数问题亦修正，早期失败日志保留，最终 51 定向 / 575 全量均重新执行。

早期定向命令误用默认 Playwright 配置，启动了 `npx vite preview`，使用本地已安装包；未下载、未重试被拒绝的 registry 或浏览器路线。最终所有门禁改用显式无浏览器配置与本地二进制。

未增加任意跨项合并、提升/嵌套、移动/重排、切换或多列表混合删除。无列表几何数值控件；原始程序删除无私有候选证明仍拒绝。浏览器/原生受限路线没有重试，真实 WebView2、IME、系统剪贴板/对话框、原生打印及 Microsoft Word/WPS 未验收；DOC/DOCX/PDF 阶段仍未完成。


## 本轮：有界十进制列表几何保留（2026-10-14）

基于 `2de7447a57a564203465d57ccf92c69866f53a9d`，新增受审计的直接正文单级 decimal `%1.` 列表：完整非负物理左右/悬挂三元组、独立固定 Latin 编号字体、同一编号实例的一段连续运行。完整正文块/项/文字/顺序与转换结果绑定，源样式关联须解析到同一编号实例；没有修改 vendor 通用优先级。源空项等未证明基线仍拒绝；编辑范围、受控 Enter 和历史/打印限制见 [十进制列表几何](LIST_GEOMETRY.md)。

### 最终门禁与身份

- **Rust 1.99 / 1.88 各 601 项通过**，即原 575 + 26 个新模型/源/草稿/导出/安全检查，不跨工具链累计。日志 `tmp/list-geometry-core-{stable,msrv}-final2.log`
- **35 项 Tauri-free 生产后端检查在两工具链各通过**，原 `scripts/pdf-rotation-probe.rs` 未改，分别链接本轮核心。日志 `tmp/list-geometry-backend-{build,tests}-{stable,msrv}-final2.log`
- **548 项无浏览器前端检查通过**，即原 524 + 24；实际 ProseMirror 候选事务、完整扩展链和注册 Enter、撤销/重做、草稿/打印快照、错误/开放列表粘贴、锁定状态、页面缩小/清除与非法原始属性均覆盖。日志 `tmp/list-geometry-ui-parent-final3-aggregate.log`，准确 argv/状态 `tmp/list-geometry-ui-parent-final3-gates.json` SHA `f7b4ac82788948a86da0cd659a6f184cb377299a8db3b8a83a22e6cf552cf001`
- prepare-pdf、TypeScript、Vite build 通过，保留既有 bundle 大小提示；仅用本机已安装 `node_modules/.bin`，`USERPROFILE=$PWD/tmp/linux-home`，没有 npx 网络获取。**6 个浏览器规格仅发现，未执行**。548 是最终源冻结后的整组结果，先前 546 / 547 轮次已被后续剪贴板/严格属性修复替代，日志保留
- 两端核心/CLI build、fmt、`git diff --check` 与 scoped all-targets Clippy 通过。原始严格 Clippy 仍各仅既有单项：stable XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`，只在对应工具链命令排除旧单项，没有新增豁免。门禁记录 `tmp/list-geometry-gates-final2.json` SHA `01d2fdb8353f74954af206af82e7ae4d9440b3e4eac84946079122a1ae3935c1`
- `CARGO_INCREMENTAL=0`，既有 `../toolchains/{cargo,rustup}`，stable `target` / MSRV `tmp/msrv-target`；无 RUSTFLAGS/profile 覆盖、依赖/锁文件/vendor 变化。资源不足时按 600 MiB 阈值暂停，parent 仅清理逐项重验的旧可重建测试可执行文件，日志、原件、冻结生产 CLI/probe 与源码不动，再继续未完成命令

冻结最终 CLI `tmp/list-geometry-final-cli` SHA **`5c008894209311c84515a6432b6e973a120ad459b30c848ea019c2128808111a`**，与 stable 最后 build 一致。旧基线 `tmp/paragraph-overhang-second-cli` SHA **`dba98e9ba0514c2c5d33ba05d82b4161dce3729ffc921df9c3d0e76ab94210ac`**。本轮 12 个产品源码文件组合指纹 **`ed586f16928576299457c536cb1c7d992e5a726096fa24570dd98360a884cc50`**；完整源身份在 `tmp/list-geometry-product-identity.json`，不将产品文件数、测试数和独立源数混计。

### 未改造原件与独立消费

完整命令由 checker 报告保存，主调用为：

```sh
python3 scripts/check-list-geometry-independent.py --cli tmp/list-geometry-final-cli --render --editor-models tmp/list-geometry-editor-original-1 --root tmp/list-geometry-final-independent
```

- 原件 `testParaListRightIndent.docx` 来自 LibreOffice/core 已验证 revision `6e5f11edfc5f080cf99e1807916df5cedec785b7`；12,774 字节，SHA `48209f95d7a2ddfbf0fb3291658b1463e7b8121dac982f26501d8775e5450707`，Git blob `975b598f476e97e08366b102f5b60b8211e384b1`。未改造/未覆盖；只是一个真实公开回归原件，不是用户生产文件或多份真实文档
- **9 个支持源 / 44 个拒绝源、57 个非法模型、12 个篡改包、13 份真实前端操作快照、22 次完整可信模型重开**。独立代码解析真实 OPC 路径、样式继承、字体、编号实例/层级、正文内容与物理声明；不导入应用 resolver。非法模型不得创建新目标，已有目标与所有源文件保持字节不变
- **38 份全新 LibreOffice PDF / 38 个独立 profile；19/19 对源或独立参考与输出的横纵坐标差严格为 0.0 pt，整页光栅完全相同**。涵盖未改造原件、完整直接覆盖、右侧显式零、起始编号 7、起始 99 的零/一 twip 悬挂，以及实际文字/间距/正文字号/Enter 编辑与所有 undo/redo。独立参考不通过应用 writer 构建
- 原件标记 x=113.50 / 170.20 pt，正文 x=141.85 / 283.60 pt，7 / 18 行正文；27 行可见文字与全部 **757,350** 页像素相等。实际查看原件/输出、字号编辑、Enter 和零悬挂图像；不是空白页面假通过
- 全量报告 `tmp/list-geometry-final-independent/run-41zrpucu/report.json` SHA **`3957dea4710f2b534a21030332f4a63578c0e174a836ef2400802b6b6cacb91b`**。报告内当时 checker SHA `34f935493297ade2bfa5e60e88c69f5314a8eb655b389710444cc2fbfb19c057` 的坐标 gate 仍为 0.1 pt，但所有实际测量都是 0.0，像素 gate 一直为精确相等
- reviewer 要求后，最终 checker 将 pairwise 坐标 gate 收紧至精确 0，SHA **`f71c8838d06b55b27c233528b56e6e9eef526426333d1de4a905d3e3bf67c187`**。对**同一 38 份全新 PDF**全部重新提取坐标、读取原光栅并复核输入/输出/CLI 身份，**19 对全部严格通过**。小型重检报告 `tmp/list-geometry-final-independent/run-41zrpucu/strict-coordinate-recheck-f71c8838d06b.json` SHA **`394f0b00f18350af29a39d8738614d1977b51b9f5d3c251628cc71025ea76c06`**；原报告不修改。这个重检没有新增 PDF，不能再累计为 38 次新渲染
- 最终实际 production `worker::run` 子进程验证原件导入和可信作者包打开与冻结 CLI 完整模型一致，源空项/负值来源拒绝、六个受保护输入/程序不变。报告 `tmp/list-geometry-worker-evidence/report.json` SHA **`654df5825adec371783438809706a8975c0d1858b3ce918943abc4e39a6c7b7e`**；新 probe SHA **`0eebd452bccf2e40fd0e6e9b7ed2f59953b99734d4494a55706e57016c57ac7a`**，链接来源 `tmp/list-geometry-backend-provenance-final.json` SHA **`9bf314d928d02d05a8b9a070ac65a11c04f871fc8a45c22b96314b8b77783180`**。这不等同 Tauri 薄适配器、系统对话框或原生 IPC

### 历史契约、审查和保留限制

- 完整历史目录 **1,520 / 1,520 条 read 响应与上一发布 CLI 精确相同**，包含返回码、stdout 原字节与解析 JSON、stderr；没有该目录内的新增拒绝、支持扩张或诊断变化。新真实列表原件的支持扩张由本轮单独的哈希固定 oracle 验证
- **302 个哈希不同的历史无新属性 DOCX 整包金样**，两 CLI 分别重新导出均与历史文件逐字节相等；每个 CLI 各 **604 次完整可信重开、604 次原件/生成件禁止覆盖**通过
- **1,647 个受保护来源/历史报告/模型/输出等文件**身份不变；另核对 **325 份保留的旧 PDF**身份，不冒称重新渲染。新渲染仍只有本轮 38 份
- 报告 `tmp/list-geometry-legacy-replay/run-go2w29_o/report.json` SHA **`616b8b261b4b52e12a837630a24354db015261b3b0301bc6fbdf0fce64fc5f1d`**，状态 PASS；原报告和当前/基线 CLI 前后 hash 均未改变

独立只读 reviewer 另通过 **58 个源检查**（6 保留 / 47 拒绝 / 5 旧规范化响应与旧 CLI 完全相等）和 **269 个模型决策/导出/可信重开/目标保护检查**。报告 `tmp/list-geometry-review/run-8bjvtabh/report.json`、`tmp/list-geometry-review/models-mf1b2dq1/report.json`；最终结论绑定已暂存树，不把定向检查加到 601/548 计数。

早期独立探针发现真正空源项会出现 **0.05 pt / 1,659 像素差**，该源子集已保守拒绝。初次 Enter 参考误继承源 Calibri 11，改为作者模型明确的 Arial 11 后精确相等；只修独立参考，不放宽阈值。第一次整组 Rust 检查因此中断并保留日志，最终 `final2` 全部重跑。资源边界夹具曾小于预设工作配额，扩大夹具后验证拒绝，没有提高配额。provenance helper 曾把 `-L dependency` 目录当作文件求 hash，发生在后端检查成功之后；修正为显式 extern 文件集合并保留原失败日志，没有重跑/冒称其他阶段失败。

无新属性的旧 writer 字节、旧 normalized list 警告和此前拒绝边界保持。新模式自身没有列表几何数值控件；结构删除/提升/嵌套/合并/移动/切换明确拒绝。作者输出重开走 `read-docx-model`；刻意强制外部 `import-docx` 仍可因不支持的 marker `rPr` / pStyle / override 组合拒绝。

已知 Chromium EPERM、云 localhost 隔离、Linux GLib/WebKit 和离线 Windows 限制没有重试。实际 WebView2、键鼠/输入法/系统剪贴板/对话框、原生打印、Word/WPS 和一般复杂列表仍未验收；不借用历史 vendor/native 计数或声称 DOC/DOCX/PDF 阶段完成。


## 本轮：显式页面内的正文缩进外伸（2026-10-13）

基于 `5f812c2612d1d1de1363b61786c3e435d0163616`，新增仅限已证明显式页面的顶层正文/标题段落负左右缩进及页边距内悬挂。两条左边缘、共同右边缘和各自行宽分别校验；不允许伸出纸张，不钳制数值或自动增加页面。缺省页面、单元格、列表与单图的旧路径维持保守范围；源负值的活动继承/覆盖来源继续可见，编号层级不顺带放行。范围和公式见 [段落页边距外伸](PARAGRAPH_OVERHANG.md)。

### 最终门禁与冻结身份

- Rust **1.99 / 1.88 各 575 项通过**，即原 554 + 21 个新模型/源/草稿/导出/反篡改检查；不跨工具链累计。日志 `tmp/paragraph-overhang-core-{stable,msrv}-final.log`
- **35 项 Tauri-free 实际生产后端检查在两工具链各通过**；原 `scripts/pdf-rotation-probe.rs` 未改，重新链接本轮核心。日志 `tmp/paragraph-overhang-backend-{build,tests}-{stable,msrv}-final.log`
- **524 项无浏览器前端检查通过**，即原 509 + 15 个新检查。实际本机已安装的 `node_modules/.bin/playwright` 完整 argv、`USERPROFILE` 和退出状态在 `tmp/paragraph-overhang-ui-local-all-final-gates.json`，SHA `0c4eec271744c8e8acd24067c723492cefb3dda50e8e609c909f4fd776b13bbf`；日志 `tmp/paragraph-overhang-ui-aggregate-local-all-final.log`
- 新前端检查使用实际 ProseMirror 事务、历史、DOM 解析/序列化与应用控件，覆盖两侧负值、首行/后续行边界、混合正文/单元格原子拒绝、缩小/清除页面、字段重置、列表/表格移动、标题转换、拆分合并、草稿与不可变打印参数。七份实际事务快照经冻结后交给独立文件消费者。不是浏览器/原生输入或布局验收
- TypeScript/Vite build 通过，保留旧 bundle 大小提示；日志 `tmp/paragraph-overhang-ui-build.log`。核心两工具链 build、fmt、严格 Clippy 的限定排除版本通过；原始 Clippy 仍各仅旧 XLS `chunks_exact_to_as_chunks` / 旧 PPT `uninlined_format_args`。完整命令/状态/日志身份在 `tmp/paragraph-overhang-gates.json`，SHA `aceb3328c7497339ab5f4ca52b2cf2af163d66a989d2b240753935058d870ef1`
- 继续使用 `../toolchains/{cargo,rustup}`、`CARGO_INCREMENTAL=0`；stable `target`，1.88 `tmp/msrv-target`，无 RUSTFLAGS / profile 覆盖、依赖/锁文件/vendor 修改或清理。页面纯校验函数提取以避免 UI 循环引用，原页面公开导出与校验规则不变
- **524 个未修改的 tracked 源码/vendor/依赖/旧脚本/测试文件逐字节等于基线**，与上述 524 前端检查恰巧同数，不能混为一组。清单 `tmp/paragraph-overhang-product-identity.json` SHA `34b59a1cd914836a975201307e1a0fe3fb7da718647941dbe0559191d5be0932`；八个本轮产品文件的组合指纹 `cc5e5d147985b51f3853160b0998cb7916e6d693b74a3989f1e1b5f2d89193db`

冻结候选 CLI `tmp/paragraph-overhang-second-cli` SHA **`dba98e9ba0514c2c5d33ba05d82b4161dce3729ffc921df9c3d0e76ab94210ac`**，与最后 stable build 的可执行文件一致。当前发布基线 CLI `tmp/pdf-rotation-chain-final-cli` SHA `e4c874a13af3317d794e650525e0ac04d535aa75318ca1c32386ccc124fe7e88`，两者在独立检查前后不变。

### 真实原件与独立消费

最终完整命令：

```sh
python3 scripts/check-word-paragraph-overhang.py --cli tmp/paragraph-overhang-second-cli --baseline tmp/pdf-rotation-chain-final-cli --editor-models tmp/paragraph-overhang-independent-preflight/editor-models-candidate-second --render
```

实际 Python 路径、LibreOffice launcher/底层程序、版本、每文件独立 profile、字体/渲染命令和输入身份均在报告中。新 checker SHA **`28bc34d48f48575867bcebc7105261fae14dc3933f0080c6a99bf6325ea15f12`**；报告 `tmp/paragraph-overhang-independent/run-3equcs4b/report.json` SHA **`ceea4a2b6afbc3cf330ea5b373c7e4d2f6e4a71d02283e0c4143e90479ff0b6e`**。

- **23 个支持源 / 39 个拒绝源**，含一个未改造 LibreOffice/core `indents.docx`；其 11,553 字节与 SHA `d62a3e3162a737cbe45e21fe544f5dc845e8cdc621bd8a2e3b7e8dcfed52386d` 保持。旧版本只有缩进阻断，新版保留五段文字与有效缩进；上游来源/revision/许可说明和历史 manifest 的旧拒绝预期均记录，不分发原始二进制
- **31 个非法模型 / 8 个可信包篡改**拒绝，包括有符号缩进/XML/页面/标题类型与同步伪造哈希。**5 份编辑结果 + 7 份实际前端快照**经独立 python-docx/XML 检查；**55 次完整可信模型重开**通过；**128 次已有目标拒绝**保留原字节（35 个导出、62 个源文件、31 个非法模型目标），另 31 个不存在目标继续不存在。标记为合成的边界件、独立参考和同源另存件不算更多真实原件
- **62 份全新 PDF / 62 个互不复用的 LibreOffice profile**。**31/31 对横纵坐标差为 0.0 pt，且 31/31 对整页光栅完全相同**；覆盖原件/原件编辑、负左右值、首行与后续行换行、边界、右/居中对齐、继承、标题、实际撤销/重做/粘贴。几何相等与像素相等分开计数；查看过原件、受控多行悬挂等实际图像，不是空白图像通过
- 原件的五个实际起始 x 为 70.95、14.25、42.60、14.25、99.30 pt；相对 twip 段落起点约有该消费者的 +0.10 pt 字形偏移，源/输出仍严格相等。行框边界与字形端点不混为一谈；不能据此声称任意长词或字体字形都不会溢出
- 实际 production `worker::run` 子进程另验证原件导入、已保存作者包打开与越界源拒绝；完整模型与冻结 CLI 精确一致，四个保护输入/程序及派生负向源前后不变。报告 `tmp/paragraph-overhang-worker-evidence/report.json` SHA `41be16113bb58b40757b7ea247dc27aed90e8607e694aada3b67bf0d984802a7`，probe SHA `eefdd0ad833cece0e4a12048a6f17ff93aff1c728037f1d3e166493c8b118900`。它证明实际进程运输，不能替代 Tauri 薄适配器、系统对话框或原生 IPC

### 历史契约与独立审查

新 checker 的 10 个非负 V1/V2 整包字节控制和 14 个精确旧响应对照之外，再独立重放完整历史语料：

- **302 个哈希不同的既有完整 DOCX 金样**分别通过当前发布基线与候选 CLI 重新导出，均逐字节等于历史原文件；每个 CLI 各 **604 次完整可信重开与 604 次原件/生成件禁止覆盖**通过
- **1,520 条既有读操作**：505 audit、771 import、244 read-docx-model。**1,513 条 stdout/stderr/返回码逐字节一致**；其余 **7 条仅为本轮有意支持扩展**：`reject-left-negative`（−1 左）、`reject-right-negative`（−1 右）和真实 `indents.docx` 各 audit+import，`reject-first-outside`（左 100 / hanging 101）仅 import
- 四份变化源的完整 XML、有效缩进、页面物理边界和完整新模型由独立逻辑验证。**无新增拒绝，此历史语料无仅诊断变化**。所有源、302 份保留模型、786 个额外历史模型输入、旧报告以及 **325 份旧 PDF 身份**保持；这些 PDF 只验证保留，不计为新渲染
- 原始严格差异报告保留 `REVIEW_REQUIRED`，不回写成完全相同：`tmp/paragraph-overhang-legacy-replay/run-bi5mm14r/report.json` SHA `33b88cf3831f55a9ff911b5cc62b5add80db88261a065e40339817c168048e22`。独立分类验收 `acceptance-report.json` SHA **`3631fac862fa364aaae60d443c4ef87c53757e72e9976b96a2797f6d9ab61d69`**
- 单独只读 reviewer 通过 **283 项公开 CLI 检查**（37 个源案例与 246 个独立边界/随机模型），另核对全部产品/保护文件身份、历史七项变化、冻结前端快照及消费者报告。最终报告 `tmp/docx-overhang-reviewer/run-final/report.json` SHA `588e93c2987fcfb3d46ab6c8907abf8707369a1a3f2896259d18349254baa29b`；该 harness 不是更多真实文档或浏览器验收

### 失败、修正及未验证范围

首次 `npx` 包装器试图访问 registry，被策略阻断，未执行测试；失败日志 `tmp/paragraph-overhang-ui-aggregate.log` 保留，没有重试被拒绝网络。随后只执行已经安装的本机 Playwright；首轮旧 grep 只选到原 509 项，保留该基线结果，再显式加入新规格完成 524 项。前端测试 DOM 替身最初缺少 CSS `cssText` 写回，补齐替身后才验证真实序列化/解析标记相等，未据此改动产品代码。

独立消费者前两次校准错误把段落起点当作字形起点，过紧的 source-only 端点断言失败；原证据保留。预最终 `run-z0ttyvj4` 的实际前端标题参考还带 python-docx 默认蓝色，而受测模型默认黑色，纵横坐标仍一致、像素不一致；查看图像后给独立参考明确设置模型要求的黑色，并用冻结 checker 全部重新生成上述 62 份最终 PDF。没有放宽源/输出坐标或整页像素比较来隐藏产品漂移。全部旧报告身份与最终计数另汇总在 `tmp/paragraph-overhang-independent-preflight/final-result.json`，SHA `e2f3693c30222eaf298d05202eaf39619eebebe03ca0c02f769bd5fc2fbafce9`。每次 checker 使用新的 `run-*` 目录；允许已有父目录，不重用或覆盖既有证据目录。

已知 Chromium EPERM、云 localhost 隔离、Linux GLib/WebKit 缺失与离线 Windows 路线没有重试。实际 WebView2、鼠标/键盘/输入法、系统对话框、Windows Job 与原生打印均未在本轮验收；没有借用旧 vendor 或原生计数。LibreOffice 单消费者精确比较不是 Word/WPS、一般复杂排版或完整 DOC/DOCX/PDF 阶段完成声明。



## 本轮：PDF 纯旋转修订链重开（2026-10-13）

基于 `a1f35cdcd1e54d7b1b97d736d50e57c26348824c`，保存后的旋转副本可显式重开后继续另存。仅接受从严格基础文件开始、逐步证明为单个叶页面有效方向恰好 +90° 的修订链；最多 16 次，且累计工作配额可能更早阻止追加。不支持一般增量修改，不依赖 Producer / 应用标记，不通过删除安全检查扩大兼容。实现 / 冷重开预算和精确字节 framing 见 [PDF_PAGE_ROTATION.md](PDF_PAGE_ROTATION.md)。桌面 API、源快照、会话 / 画布身份、源会话保持与不覆盖发布路径没有改动。

### 最终门禁与冻结身份

- Rust **1.99 / 1.88 各 554 项通过**，即原 537 + 17 个修订链检查；旋转模块共 43 项，不跨工具链重复累计。日志 `tmp/pdf-rotation-chain-core-{stable,msrv}-final.log`
- 新检查覆盖同页四转、多页交替后重访、继承方向、每次完整源前缀和 ID、七种基础 EOF / 空白结尾、原终端语法与严格 loader 一致性、16 次成功 / 第 17 次拒绝、旧历史及原始字节篡改、额外对象 / 几何 / 引用变化、跳转 / 循环 / 向前 Prev、旋转跳跃、重新序列化与尾部添加、累计配额及先前文件不覆盖
- **42 MiB 原始 stream 的受控文件成功另存，并经全新 `selection` 审计重开**；50 MiB 对照因累计工作配额明确拒绝。另用 301 个 stream 共享带 2 MiB 空白的间接 Length 对象，证明重复物理扫描也计费并提前拒绝。它们是资源边界合成样本，不是真实文件兼容或吞吐量承诺
- **35 项 Tauri-free 实际生产后端检查在两工具链各通过**；`scripts/pdf-rotation-probe.rs` 原样编译 / 运行，日志 `tmp/pdf-rotation-chain-backend-{build,tests}-{stable,msrv}-final.log`。stable 生产 worker / 发布 probe 另经真实修订文件检查，不能等同系统对话框或原生 IPC
- **509 项无浏览器前端检查通过**，前端源码未改。先完成 TypeScript / Vite build，再使用旧精确 argv 运行全量；日志 `tmp/pdf-rotation-chain-ui-{build,aggregate}.log`，命令与结果 `tmp/pdf-rotation-chain-ui-gates.json` SHA **`5bd97ff65dc1ae417a728a8f18e85579936b7f2eaf8502b67b9dbb842d056c65`**。保留既有 bundle 大小提示
- 核心库 / CLI 两工具链 build、`cargo +stable fmt --check`、probe rustfmt、新 Python checker 语法及 `git diff --check` 通过。原始严格 Clippy 仍各只有旧跨格式单项：stable XLS `chunks_exact_to_as_chunks`、1.88 PPT `uninlined_format_args`；只排除各自原单项后通过。日志 `tmp/pdf-rotation-chain-clippy-{stable,msrv}-{raw,scoped}-final.log`
- 缓存仍为 `../toolchains/{cargo,rustup}`，`CARGO_INCREMENTAL=0`；stable 使用 `target`，1.88 使用 `tmp/msrv-target`，前端 `USERPROFILE=$PWD/tmp/linux-home`。无 RUSTFLAGS / profile 覆盖、新 target、依赖 / 锁文件 / vendor 修改；没有清理证据或构建目录
- 除本轮 `src/pdf_rotation.rs` 外，**357 个受保护的 tracked 产品源码 / vendor / 依赖 / 旧脚本逐字节等于基线**。清单 `tmp/pdf-rotation-chain-protected-identity.json` SHA **`50140a3a9fd966bac9ab264bb0a86429167e0299c8b5831e57d264f5a32ac702`**

冻结产品源码 SHA **`c5a2eae5321cc3ca0c25f718e34b76592b282184de6e22a2e7a1f937eed2ea04`**；CLI `tmp/pdf-rotation-chain-final-cli` SHA **`e4c874a13af3317d794e650525e0ac04d535aa75318ca1c32386ccc124fe7e88`**；生产 probe `tmp/pdf-rotation-chain-probe` SHA **`a151ee75596e810a59afc437b944128d1214cd1b665fd54bc42b76ce2d60ba17`**。链接来源及真实命令在 `tmp/pdf-rotation-chain-backend-provenance-stable-final.json`，SHA **`ce14d5e19c45dabb0ec502e9f0096e03879c17c39bec7a72a5e90a650dc13b5a`**。

### 独立原件、逐次保存与图像

后继 checker 单独新增，旧 `scripts/check-pdf-rotation.py` 及首版报告原样保留；首版“所有旧修订拒绝”的预期只适用于历史版本。本轮实际命令：

```sh
python3 scripts/check-pdf-rotation-chains.py --candidate tmp/pdf-rotation-chain-final-cli --probe tmp/pdf-rotation-chain-probe --provenance tmp/pdf-rotation-chain-backend-provenance-stable-final.json --output tmp/pdf-rotation-chain-independent-final
```

新 checker SHA **`d6801ef5b4a5b1e889dd2a680de0ed01c5c2976cd318089db9b5104fab399511`**；最终报告 `tmp/pdf-rotation-chain-independent-final/report.json` SHA **`a404a45d16ff76d10e92f286d31ad5a4dedf215a15d066a206048d881cfb670e`**。固定原件仍是同一份未改造 Mozilla tracemonkey.pdf，1,016,315 字节，SHA **`3662ff519e485810520552bf301d8c3b2b917fd2f83303f4965d7abed367e113`**，不能把多个保存结果计成更多真实文档。

- **3 组路径 / 9 个中间保存文件**：同页 `[1,1,1,1]`、混合 `[1,7,14,1]`、冻结首版输出继续旋转。每个中间文件用 pypdf 独立核对 **996 个对象、377 个 stream 的原始及解码字节**、前驱及原件完整前缀、活动 xref、精确单叶替换、Prev、独立重算 ID、逐页正文 / 页序 / 几何 / 内容及资源引用
- **9 次实际 production publication** 显式打开前一已保存文件、建立新会话、调用生产 worker 与父进程发布；每份输出逐字节等于独立检查的 CLI 文件，源会话和所有前驱文件保持。另有一次原始 worker reopened-chain 请求，输出身份与已检查的第三次旋转一致
- **117/117 次未选页面的前驱 / 输出 Splash 光栅完全相同**；**112 次从未选中页面的原件对照相同**。字节哈希完全相同的 PDF 渲染明确复用，不把复用计成新的渲染。其余每页由新 Poppler 进程生成，配置与命令记录在报告。原 helper 的 Splash 身份记录停在第二层 wrapper；补充 `tmp/pdf-rotation-chain-renderer-provenance-postrun.json`（SHA **`6c4c4d72b8b94a9e78f46a9dcbe20a92aceb1057205b4bffd2d600ecded98e69`**）另记录实际 ELF、字体配置及动态库，明确属于运行后观察，不冒称运行前已冻结。Splash 26.05.0 与 Cairo 25.03.0 的差异单列
- **28/28 次已触及页面的独立累计方向参考对照相同**，覆盖 Splash 与 Cairo；9 份 pypdf 参考由原件独立应用累计方向生成。两个 Poppler 路径不等于两套无关 PDF 引擎。同页四转后的 **14/14 页原件光栅完全相同**；这不是一般性 PDF 重写保真承诺
- **36 个安全 / 拒绝控制通过**：源 / 既有目标 / 硬链接 / 符号链接不覆盖、无效页号和全部选中身份、旧历史 / 新方向 / ID / 几何 / Prev 篡改、取消 / 过期会话、源改变 / worker 身份错误；两份原有未改造不支持原件与六份受控加密 / 注释 / 表单 / 动作 / 签名字典派生件继续拒绝。派生件可能含额外阻断，不把它们当作有效密码学签名或单因子证明
- **45 个受保护原件、历史证据、二进制与派生文件在检查前后不变**。重复调用已存在证据目录在写入前拒绝，报告散列不变；`tmp/pdf-rotation-chain-existing-evidence-rejection.json` 记录这次负向结果。证据约 66 MiB；另查看原件及第 1 页 90 / 180 / 270 / 0° 的实际图像，页面有真实论文内容，没有空白图像假通过
- 独立只读 reviewer 另以公开 CLI 完成 **55 项对抗性检查**，包含独立构造且与冻结旧 writer 字节相等的追加修订、完整链深度、物理 framing / stream 假标记、所有额外修改及不发布保护。`tmp/pdf-rotation-chain-reviewer/report.json` SHA **`818cd50d223fafddb52c9e8cc613e2a2be3be56e7b2b3ad54c4b43c5aa615b1e`**；该受控 harness 不充当更多真实文档

### 原样保留的限制与失败证据

本轮 **9 个直接“前驱光栅精确转置”诊断全部失败并保留**，没有放宽阈值、删图或改写成通过。验收仍采用首版已明确区分的“独立旋转参考 + 未选页精确图像 + 字节 / 对象 / stream 保护”；首版三页失败计数及原报告散列再次核对。pypdf 每页正文不变；Poppler 对旋转后的复制文字可能重排或拼接断词，本轮只确认相同工具下候选和独立累计参考的正文精确一致。

早期测试的 Debug 约束表达式、被 writer 自动重算 Size 的无效负向夹具，以及误将 `+startxref` 的 raw 语法接受当成 strict loader 接受的预期已修正，失败日志保留；没有放松产品 guard。独立审查推动补齐 32 位 xref 加法边界、共享 Length 扫描计费和首次输出的冷重开预算预留。

Chromium EPERM、云 localhost 隔离、Linux GLib/WebKit 缺失与离线 Windows 路线没有重试；Tauri 薄适配器仍只有既有源审查，实际 WebView2 / 系统对话框 / 原生 IPC / Windows Job / 窗口关闭未在本轮验收。没有重跑 vendor 或借用历史原生计数。资源计费不是文件锁、历史真实性认证、进程 CPU / 解码内存配额或通用修订解析；复杂 PDF、OCR / 编辑与整个 DOC/DOCX/PDF 阶段仍未完成。


## 历史：首版 PDF 当前页旋转另存（2026-10-13）

基于 `42482a5e166eb5ed46615d6ee04428c078974b95`，新增严格子集的“当前页顺时针旋转 90°，另存副本”。当前页面必须已经完成渲染，PDF.js 页号 / ref / 有效方向 / 总页数与源 SHA、精确会话 token 交叉验证。只追加选中叶页面的 Rotate 及必要 xref / trailer / 第二文件 ID，完整源前缀保留，源会话不切换，任何已有文件都不覆盖。已有修订、加密、线性化、签名 / 表单 / 注释 / 动作及未证明语法拒绝；保存后的副本可以预览，但首版不能再次旋转另存。完整范围见 [PDF_PAGE_ROTATION.md](PDF_PAGE_ROTATION.md)。

### 最终门禁

- Rust **1.99 / 1.88 各 537 项通过**，即原 511 + 26 个实际核心旋转 / 原始 framing / 候选验证 / 文件安全测试，不跨工具链重复累计。最终日志 `tmp/pdf-rotation-core-{stable,msrv}-final.log`
- 新核心覆盖：单个 classic xref / generation-zero / 精确对象集合与偏移；重复 / 转义键、整数 / 实数 / 嵌套字典混淆、终端注释吞掉 startxref / EOF、跨对象注释、未索引物理对象、藏在 stream / 字符串中的假对象头、重复 Length 别名、错误 / 循环 / 非整数 Length、缺失 endobj / 解析器跳过对象、Info 缺失或类型错误、树 Count / Parent / 环 / 共享孩子、继承几何 / 方向、身份错配、预算、第一 ID 保留 / 第二 ID 改变、旧修订拒绝、确定性输出与拒绝覆盖
- **35 项 Tauri-free 实际生产模块测试在两工具链各通过**，即原 15 + 20 个旋转保存事务检查。稳定版最终日志 `tmp/pdf-rotation-backend-tests-stable-final.log`；MSRV 初次日志 `tmp/pdf-rotation-backend-tests-msrv.log`，冻结 probe 格式后的最终日志 `tmp/pdf-rotation-backend-tests-msrv-final.log`。覆盖对话框取消、worker 前 / 中 / 后及最终发布前的过期会话、源变化、错误响应 / 候选身份 / 路径 / 类型、partial stage 清理、原件 / 已有目标 / 硬链接 / 符号链接 / 最后一刻出现目标的拒绝，以及原有真实子进程超时、阻塞 stdin、非零退出、响应上限
- 保存事务的确定性竞态测试明确使用运输替身，不冒充已解析 PDF；另外的独立 genuine-file probe 真正调用生产 core / worker 子进程与父进程 publication 路径。读取、哈希、同步不跨 session 锁，最终 ownership + persist_noclobber 在短锁内完成
- **509 项无浏览器前端检查通过**，即原 477 + 32 个新增实际函数检查；准确 argv `tmp/pdf-rotation-ui-command.json`，最终日志 `tmp/pdf-rotation-ui-aggregate.log`。新检查覆盖精确 token / page / ref / rotation / count、成功渲染来源、失败翻页保留旧画布、相同页面 / 缩放重叠渲染、重复操作、取消 / 读写失败、旧页面 / 会话 / 总页数下的迟到结果与控制恢复
- 定向 PDF 组 **67 项通过**，包含上述 32 新检查及 35 旧检查，不能与全量累计。日志 `tmp/pdf-rotation-ui-final.log`。通常的保存期间关闭本来由 operation 暂停保护覆盖；本轮修复的是“关闭已在 await，保存随后开始”的竞态，在同一个原生 close handler 的 destroy 前同步复查，测试使用实际 handler 与窗口运输替身
- TypeScript / Vite build、stable / MSRV 核心库与 CLI 构建、production probe 编译、核心格式、Python checker 编译、probe rustfmt 和 `git diff --check` 通过。前端日志 `tmp/pdf-rotation-ui-build.log`，保留既有较大 bundle 提示
- 严格原始 all-targets Clippy 仍分别只有旧 XLS `chunks_exact_to_as_chunks` 和旧 PPT `uninlined_format_args`；各仅排除相应旧单项后通过。日志 `tmp/pdf-rotation-clippy-{stable,msrv}-{raw-final,scoped}.log`（MSRV 原始日志为 `...-msrv-raw.log`）。新增十六进制循环的同类 lint 已在本模块修复，没有新增豁免
- 使用既有 `../toolchains/{cargo,rustup}`、`CARGO_INCREMENTAL=0`、stable `target` 和 MSRV `tmp/msrv-target`；无 RUSTFLAGS / profile 覆盖、无重复 target、无依赖 / 锁文件 / vendor 变更。前端保持 `USERPROFILE=$PWD/tmp/linux-home`

### 未改造原件与独立验收

实际最终命令：

```sh
python3 scripts/check-pdf-rotation.py --candidate tmp/pdf-rotation-final-cli --probe tmp/pdf-rotation-probe-final --provenance tmp/pdf-rotation-backend-provenance-stable-final.json --output tmp/pdf-rotation-independent-reference-final --visual-oracle independent-reference --diagnose-inverse-viewport
```

最终报告 `tmp/pdf-rotation-independent-reference-final/report.json` SHA-256 **`3c5fb35cfdab34b3e382e0f788036d49b2aa300283f59314f70026095fd7681d`**。冻结 CLI **`fae10aa6c08a45f2be89acca8baccc83070da45bb86ef49ef5b5c4db392f5b00`**，最终 production probe **`509d9bd2c2641c8ea3ca5f0375b44b9c7fc01ba9a7fbbc5b5d267311f368b4a8`**，checker **`d847a3bc0dd3f0386c4fe2b64fe1434f3193ae070f06379d0c9f22a790999161`**。最终 stable backend 链接来源在 `tmp/pdf-rotation-backend-provenance-stable-final.json`，SHA **`4fe392642f153dfe50f9f3db4927795cf90464565510a34708ded9cc325eea49`**；汇总构建 / 源 / 依赖身份在 `tmp/pdf-rotation-provenance-final.json`。

- 原件为固定 Mozilla manifest 中的已发表论文 tracemonkey.pdf：1,016,315 字节，SHA **`3662ff519e485810520552bf301d8c3b2b917fd2f83303f4965d7abed367e113`**；14 页、嵌套页面树、共享资源。只是 **1 份未改造论文**，不是三份真实文档或普遍 PDF 兼容声明
- 对第 **1 / 7 / 14 页**分别另存。每份独立 pypdf 核对 **996 个对象、377 个 stream 的原始及解码字节**，仅选择页面的 Rotate 与必要 trailer ID / Prev 变化；所有未选对象、活动偏移、内容 / 资源引用、页面次序和几何相同，源前缀精确保留，pypdf 每页正文相同
- **39/39 个未选择页面的源 / 输出光栅完全相同**。每页通过新启动的 Poppler 进程渲染，程序 / wrapper / 实际可执行文件身份、DPI 和完整命令均记录
- **6/6 个选择页对照完全相同**：三份输出分别与 pypdf 独立生成的同方向参考 PDF，在 Poppler Splash 和 Cairo 两条路径下逐像素相同。两个 Poppler 路径独立于本应用 writer，但不是两套无关 PDF 引擎；三个参考 PDF 明确是受控派生件，不算原件。参考 / 应用输出的 Poppler 正文也精确相同
- **25 项安全 / 拒绝控制通过**：既有目标 / 源拒绝覆盖、无效页号与各身份字段、重新旋转旧修订、真实 production 取消 / 过期 / 发布、源同长度变化，以及两份实际加密派生件（含空密码）、四份受控 annotation / form / action / signature-dictionary 派生件。后者不是有效密码学签名验证
- 另 **2 份未改造负向原件** basicapi.pdf / rotation.pdf 继续按 manifest 验证身份并被拒绝，分别包含不支持结构 / 歧义及线性化；三个原件与保护的二进制 / manifest / checker 前后散列不变
- 对已有证据目录的重跑在写入前明确拒绝，报告散列不变；负向日志 `tmp/pdf-rotation-existing-evidence-rejection.log`

### 没有隐藏的图像与文本差异

初始精确“源光栅直接转置”标准 **失败**：72 DPI 的三个所选页面分别有 **45,253 / 49,432 / 27,282** 个不同像素（每图 484,704 像素）。失败报告 `tmp/pdf-rotation-independent-first/report.json` SHA **`12cbfa2426c5f4962e849b01fcc34e736d9f0ed1b0fada6ded2e8ef5ed9d4a9c`** 及原始图像保留；最终 checker 仍将三项标为失败诊断，不把它们计成通过。

经独立审查后才明确采用上述“独立旋转参考 + 未选页精确像素 + 对象 / stream / 前缀不变”标准。`strict-transpose` 仍是 checker 默认值，替代标准必须显式传 `--visual-oracle independent-reference`，没有修改阈值容差来掩盖差异。

MuPDF 在不重写 PDF 的前提下对输出施加反向视口旋转：关闭抗锯齿时三页均精确相同；默认抗锯齿下第 7 页还有 **22 像素、最大通道差 15** 的残差，其余两页相同。初始独立报告为 `tmp/pdf-rotation-mupdf-inverse-no-aa/report.json` 和 `...-inverse-first/report.json`；最终每一例又记录自身源 / 输出身份及生成诊断脚本。这支持方向相关光栅化解释，但根因仍未证明。关闭 FreeType 的试验出现字体错误与空白输出，被明确废弃。

Poppler 在所选第 1 / 7 页出现文字顺序 / 断词拼接变化，第 7 页 sus- / pend 被拼为 suspend；不把差异归一化成“全文完全相同”。相同工具对独立旋转参考与应用输出的结果精确一致，pypdf 与 stream 数据核对也保持相同。此功能不承诺每个阅读器的复制文字顺序不变。

### 纠正与未验证范围

初始编译的 re-export 路径及一处 Result 嵌套、测试变量遮蔽已修正，失败日志保留。早期物理边界测试曾误用真实对象头而非 stream 中的假头、把实际 1 字节换行当成错误 Length、把有换行终止的合法注释当成跨界注释，修正后补上真正反例；没有弱化生产 guard。独立源审查补齐原始终端注释边界、Info 引用、渲染来源与迟到总页数保护，最终门禁均在这些修复之后。

实际 Tauri 主适配器仍只能源审查；上述 standalone / DOM 检查不证明系统对话框、原生 IPC、WebView2、Windows Job 或实际窗口关闭已经验收。已知 Chromium EPERM、云 localhost 隔离、Linux GLib/WebKit 缺失与离线 Windows 路线没有重试。vendor 与真实桌面测试没有重跑，不借用旧 1,264 项或历史原生计数。外部原地写入、目录恶意置换、源历史侦测、解压 / 渲染资源上限、OCR / 编辑 / 多次旋转与全阶段完成均不在保证范围内。


## 本轮：PDF 预览字节身份与会话选择（2026-10-13）

基于 `9431f8485796cf963cc49f2a935f1c1a6b64ab7d`，本轮将检查时的实际 PDF 缓冲区长度 / SHA-256 与随后交给 PDF.js 的同一返回缓冲区绑定；变化明确要求重新打开。worker 只传固定大小身份，旧 `Inspection` / 严格及预览 JSON 结构不变。新旧选择通过预留 ticket 与精确字符串 token 隔离，失败 / 取消保留已提交文档；演示另存的会话写入同样检查过期，已保存输出明确告知。完整范围见 [PDF_PREVIEW_IDENTITY.md](PDF_PREVIEW_IDENTITY.md)。

### 门禁与覆盖

- Rust **1.99 / 1.88 各 511 项通过**，即原 503 + 8 个核心身份检查，不跨工具链重复累计。原 23 个快照 / panic 测试保留，共 31 个 PDF 源检查。最终日志 `tmp/pdf-session-core-{stable,msrv}-release.log`
- 新核心检查：已知 SHA-256、实际提取缓冲区身份、返回错误 / unwind 身份、已打开句柄的路径替换 / 删除、不同长度及同长度替换、公开 API 拒绝变化 / 显式重开 / A→B→A、读取 / 签名 / 超限 / 缺失错误、准确 64 MiB 交付。原长度增长 / 缩短 / 不超过上限+1读取等检查继续有效
- **15 项 Tauri-free 实际生产模块检查在 Rust 1.99 / 1.88 各通过**：11 项会话 / 协议 / 交付检查，4 项实际 `run_process` Linux 子进程截止、阻塞 stdin、异常退出与响应上限。包含屏障控制的中途选择和 `try_lock` 证明不跨 I/O 持锁、返回 `Vec` 指针不变、同路径重开、取消 / 失败防旧结果复活、ticket 单次提交、超过 JavaScript 安全整数的精确 token 与溢出拒绝。日志 `tmp/pdf-session-tests-release.log`、`tmp/pdf-session-tests-msrv.log`
- **477 项无浏览器前端检查通过**，即原 460 + 17 个新增实际 `main.ts` 函数检查；精确命令 `tmp/pdf-session-ui-command.json`，最终日志 `tmp/pdf-session-ui-aggregate-release.log`。新检查覆盖 token 原样传送、变化不进入 PDF.js、迟到读取 / 加载 / 页面 / 正文、密码取消 / 错误 / 重试、取消后源改变 / 显式重开、旧销毁后的选择与控制恢复
- 新测试特别模拟异步排队的 dialog close 事件：旧 close 到达新密码提示时不能消耗新 listener。同步 DOM 替身不能证明这一点，因此新增可控事件队列；仍不称为浏览器复现或验收
- TypeScript / 前端构建、核心公共库与 CLI 构建、probe 两工具链编译、格式 / Node 语法 / `git diff --check` 通过。前端构建日志 `tmp/pdf-session-ui-build-release.log`。核心严格原始 Clippy 仍分别只有既有 XLS `chunks_exact_to_as_chunks` 与 PPT `uninlined_format_args`；只排除各自原单项后通过。未修改 vendor、Cargo/npm 依赖或锁文件

### 独立原件与协议证据

`scripts/check-pdf-session-identity.mjs` 是显式后继检查器，原 `check-pdf-panic-recovery.mjs` 及其整文件前端冻结保护保持原样。`tmp/pdf-session-independent-first/report.json` 完整通过：

- 6 份固定未改造 Mozilla 原件：独立 Node 哈希 / 长度与公共 wrapper、实际 worker 信封相同；6 份生产函数实际返回字节逐字节等于原件，并直接交给真实 Node PDF.js。Poppler / pypdf 同时独立读取，不将字形顺序差异抹平
- 4 份严格成功报告全部非耗时字段精确等于冻结 CLI；6 份旧预览 / 新预览 / 身份 wrapper / 实际 worker 的原 `Inspection` 精确一致。2 份 CMap panic 仍为严格错误及空正文预览候选，4 次真实 PDF.js 恢复 / 实际前端搜索检查通过，并保留 14 项失败 / 部分 / 取消 / 过期控制
- 24 项协议 / 源控制，包括缺失 / null / 短摘要 / 额外字段 / 长度不符 / 格式伪装 / worker token 不可信、真实 A→B 替换拒绝、B 显式重开、恢复 A、同长度负例、缺失 / 非 PDF 源、加密候选及畸形候选身份不等于 PDF 有效性
- 3 份 DOC/DOCX 的严格 / 预览 / worker 输出对照保持原有路径；六个原件、冻结二进制、历史报告及所有构建来源在前后检查中保持不变
- 已有证据目录拒绝覆盖（EEXIST），用冻结旧 CLI 冒充候选在提取任何样例前拒绝；原报告哈希未变。日志 `tmp/pdf-session-independent-first-existing-output.log` 与 `tmp/pdf-session-independent-reject-baseline/`

固定旧 CLI SHA-256 为 `c92bdffaeeba9fa823d03ed53c141b284645ac70a20ff002c5110e2918274976`；当前 CLI 为 `8d8799d15ab7faf8cf53d496445a9dda00cab89b8753db56338a4178a5069bff`，生产模块 probe 为 `045dd8c0c5fbdb9b0ecdb2ba7bd0b094ab4fc2015280caf652db0818cf0691c4`。构建来源和相关源 / 依赖 / 库 / 二进制身份在 `tmp/pdf-session-provenance.json`；独立报告 SHA-256 为 `fad4a12bc3fee8f56926fb4c445ffa7bbafb3519559cba3e5faab390302081f0`。

### 纠正与环境边界

首轮新增测试声明语法错误及一处 cfg 属性后的多余空行 lint 已修正，失败日志保留。首轮误用全目录格式化导致的 vendor 纯格式改动立即恢复，最终 vendor 与发布版本字节一致；没有删除原件或历史证据。独立源审查发现迁移后一个 `SavedPresentation` 构造缺字段，以及异步 dialog close 事件覆盖不足，均修正并补验。

实际生产 `worker` / 会话模块由 standalone probe 编译执行，薄 Tauri 命令适配器只能源审查，不能据此声称原生编译或 IPC 通过。Chromium EPERM、云端本机服务限制、Linux GLib/WebKit 依赖及离线 Windows 环境仍在；本轮没有重试已拒绝路径、没有实际浏览器 / WebView2 / 对话框 / 页面像素验收。历史浏览器密码与 PDF 工作区样例仅补 token，不将未运行的规格计入通过数。

本轮不提供文件系统原子快照、A→B→A 历史侦测、文件锁、解码资源扩张保护或后续读取时间截止。原 worker 30 秒、32 MiB 响应、64 KiB 请求和 Windows 512 MiB 配额保持，不能把独立 Linux worker 试验等同 Windows Job 实测。


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
