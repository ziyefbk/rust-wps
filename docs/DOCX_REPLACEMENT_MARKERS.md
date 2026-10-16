# DOCX 替换标记边界保护

发布 42（2026-10-16）：双工具链定向 Rust、独立边界、前端与后端回归已通过；接受范围为三类标记的安全拒绝，限制如下。

## 支持边界

本轮只改变原包文字替换 `replace_docx` / CLI `replace` 的已有边界检查，增加 `w:softHyphen`、`w:noBreakHyphen`、`w:sym` 三个字面元素名。空元素与显式开始 / 结束元素均纳入检查，不改上游文字匹配器、导入器、编辑模型、schema、导出器或字体 / 颜色行为。

- 匹配起点 < 标记位置 < 匹配终点时拒绝整个另存请求；不会只保存同一请求中的其他安全匹配
- 标记恰好在匹配的起点或终点，以及完全位于标记一侧的匹配，不因该标记而拒绝；标记本身保留。左侧文字长度改变后，标记的数值偏移可随之改变，这不等于被移除
- 正文及已关联的页眉、页脚、脚注、尾注、注释沿用现有替换范围；任何受检故事部件中的跨界匹配均不能发布部分结果
- 实际 `w:t` 文字中的 U+00AD / U+2011 仍是普通字面文字；转义后的 `<w:softHyphen/>` 字样不是 XML 标记。没有新增符号字形查找、标记删除或跨标记替换能力
- 原有段落、Tab、换行、字段和对象等边界保护不放宽。此处只扩充现有字面 `w:` 元素检查，不声明通用 OOXML 命名空间 / 别名兼容

拒绝提示为：`Search would cross a tab, break, field, embedded object, hyphen or symbol marker; choose a smaller search phrase`。遇到该提示，应缩小查找短语，使匹配不跨越标记；不是让程序猜测应该怎样移动或删除标记。

拒绝时必须保持源文件完整字节不变、不创建目标；已有输出、源路径别名与硬链接不得覆盖。成功的安全替换仍保留未修改 ZIP 部件的 payload，ZIP 压缩及元数据会重新生成，不保证整个输出 ZIP 与源字节相同。

## 未改造原件与旧缺陷

真实来源为 Apache POI 的 [drawing.docx](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/drawing.docx)，固定提交 `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`。本地原件为 95,247 bytes，SHA-256 `5e83462dd7ac3cf0a333fe09cf0837602dbd41775cdcec813a6e288b5d55dff8`，Git blob SHA-1 `23546b7f2cf38843260864e650314997fb0cc680` 与固定来源记录相同；没有改造原件或将其加入本轮提交。

在零起算第 33 段，唯一查询 `Омскводоканалу". Одновременн` 位于字符 `[98,126)`，内部 `softHyphen` 位于 117；对应 UTF-8 字节范围为 `[161,214)`，标记为 196。旧冻结 CLI 成功输出长替换文字后，原标记被遗留在字符位置 156，已经脱离原来的词内位置。字符偏移来自独立 XML token 遍历，不是页面坐标。

独立参考从同一原件执行唯一文字替换并删除恰好被跨越的那个标记；实际旧输出移除该单一标记后，两者规范 XML 树相同。固定云端 LibreOffice 中产品 / 参考均为 20 页，完整文字相同，第 2 页字符几何和光栅不同。此对照记录字符几何与像素差异，没有人工逐图验收；不是修复后的输出验收，也不证明一般 Office 排版保真。修复的正确结果是拒绝危险请求并保留原件。

最初探索用了非唯一查询，产品替换 8 处而参考只修改 1 处；其页数差异不能归因于标记，原失败证据保留并排除。后续唯一查询对照纠正了参考绑定。

## 独立基线与候选验收

独立 Python `zipfile` + 命名空间感知 XML oracle 的旧基线为 85 例：

- 31 个新增标记跨界请求错误发布输出：12 个标记 / XML 形式 / 同或跨 run 组合，3 个安全与危险匹配混合原子性控制，15 个附属故事控制，1 个未改造真实原件
- 50 个安全替换符合独立完整文字、标记位置及属性预期：24 个标记 / XML 形式 / 同或跨 run 的两侧边界控制，15 个附属故事正例，6 个段落边缘标记，3 个 U+00AD / U+2011 / ♠ 字面文字控制，2 个真实原件精确单侧替换；转义标记字样与无关段落隔离另由新增 Rust 测试覆盖
- 4 个既有边界控制继续拒绝；另有 9 个既有目标 / 源别名 / 硬链接不覆盖检查通过；所有源的完整字节、ZIP payload 散列及语义保持

stable 与 MSRV 候选各自通过同一独立 85 例：35 个整次拒绝 = 31 个新增跨界 + 4 个旧边界，50 个安全替换通过，零危险输出；另 9 个不覆盖检查与全部源快照保持通过。真实跨界请求拒绝 / 无输出，真实精确左右边缘替换仍符合标记预期。50 个安全输出的完整 ZIP 部件 → payload 散列映射在旧冻结基线、stable 与 MSRV 之间均相同。两个工具链的执行结果不另算为 170 个不同用例。

只读 `inspect` 的数据除明确记录的顶层 `elapsed_ms` 外与旧基线相同；`import-docx` / `read-docx-model` 的退出值、标准输出及标准错误完全相同。这些命令对照不构成新的布局或导入兼容承诺。

## Rust 与相关回归范围

新增 6 个 integration 测试函数，覆盖六种元素形式、精确边缘与 payload 保留、混合匹配与空替换原子性、Unicode / 实体及字面连字符、局部隔离与转义文字、五类关联故事。`editing` 目标由 4 个变为 10 个测试，循环子例不另计为不同 Rust 测试。

本轮 stable 1.99.0 与 MSRV 1.88.0 已分别通过 342 项 library + 10 项 editing，即各 352 项定向测试，并通过 CLI 构建、格式与 diff 检查。测试名称集合相同，不是 704 个不同测试。本轮未重跑 884 项完整 Rust 套件，不宣称全套通过；此前发布 40 的各 878 项全套与各 35 项后端只作为历史记录。

严格 scoped Clippy 未全绿：stable 在 `spreadsheet_xls.rs` 触发既有 `chunks_exact_to_as_chunks`，MSRV 在 `presentation_import.rs` 触发既有 `uninlined_format_args`；原失败保留，仅针对各自历史 lint 的精确 allowance 复跑通过，不写作“所有严格 lint 通过”。

为控制本轮构建容量，实际使用 `debug=0`、`opt-level=0`、`debug-assertions=true`、`overflow-checks=true`、`codegen-units=16`、`incremental=0`；构建单作业，各执行阶段串行。完整配置另固定 LTO 关闭、panic=unwind、strip=none、rpath=false、split-debuginfo=off，以 locked / offline 运行。该新 profile 保留断言和溢出检查，不作为旧构建 profile 或逐位等价证明，也不把更少的实际测试包装成全套。旧代码 342 项 library 基线也通过；最终 profile、命令、冻结身份与结果见执行回执。

## 前端与生产后端范围

链接当前候选普通 library 的提取生产模块在 stable / MSRV 各通过 35 项后端测试。stable 实际进程 Open worker 通过 5 组序列 / 15 次 Open 往返；每组另含 1 次候选 CLI 跨界原子拒绝、1 次安全单侧替换的独立 XML / ZIP 核对及 1 次已有输出不覆盖。所有原件保持；Open 完整响应只排除记录中的 `/document/inspection/elapsed_ms`。

Open 往返是真实 `worker::run` → 子进程 `--document-worker` 链路；替换使用候选 CLI / 同一核心。Tauri 的 `replace_current` 在另存对话框后通过 `spawn_blocking` 直接调用核心，不能写成“替换经过文档 worker”。这轮未执行原生 GUI、另存对话框或 Tauri IPC。

既有 826 个不同 headless 前端检查各执行一次，无跳过或重试，TypeScript 与 production build 通过；本轮没有新增前端检查。受检候选 CLI 与源码 pins 在执行前后相同，部分检查使用 DOM / 运输替身。没有新增真实浏览器、原生桌面 GUI、Word / WPS 或一般 Office 消费者验收；本轮亦不扩大基础导入对软连字符、符号、字段或复杂布局的兼容范围。

## 证据位置

- 旧 CLI 独立基线与 85 例结果：`tmp/release42-independent/baseline/report.json`
- 真实原件来源、完整独立文字 / 标记偏移：`tmp/release42-independent/genuine-source-baseline.json`
- 唯一查询来源绑定：`tmp/release42-compatibility-investigation/genuine-source-binding.json`
- 校正后的旧输出 / 参考消费：`tmp/release42-compatibility-investigation/unique-match/soft-hyphen-probe.json`、`independent-tree-consumer-check.json`
- 独立验证入口：`tmp/release42-independent/verify_boundaries.py`
- 双工具链独立验收：`tmp/release42-independent/acceptance.json`；详单为同目录 `candidate-stable/report.json` 与 `candidate-msrv/report.json`
- 双工具链定向核心总回执：`tmp/cold-validation-20261016/candidate-v1/combined.json`，SHA-256 `281b2a2f549e3511d72349e18b790bc102fb035f8cb17b32cba927968b916fc3`；同目录 `plan.json` 保存完整 profile，各 `stable/completion.json` / `msrv/completion.json` 保存实际命令、原始失败及 allowance 复跑结果
- 受检 CLI：`tmp/cold-validation-20261016/candidate-v1/stable/candidate-cli`，SHA-256 `d062da54e83352cec4a19212678904c8a53f06d8ae49dce8b42fd698f5018a63`；对应 `msrv/candidate-cli`，SHA-256 `68714e21c87548501c604f6e2dc51ebc4aeab5a8bb2864b2b3c816926d503526`
- 当前 library 后端 35 项：`tmp/release42-backend/stable-tests-6gz8gqos/report.json`、`tmp/release42-backend/1.88.0-tests-9uz24y98/report.json`
- 实际 Open worker：`tmp/release42-backend/stable-worker-lizd9ude/report.json`、`tmp/release42-backend/stable-transport-tqd_veue/report.json`
- 前端 826 项 / TypeScript / build：`tmp/release42-frontend/attempt-ixklvvaj/report.json`

上述临时证据不作为仓库分发的测试依赖。核心回归入口为 `tests/editing.rs`；安全检查在 `src/editing.rs`。历史文档只在顶部增加本轮说明，发布 41 及更早记录保留原字节。
