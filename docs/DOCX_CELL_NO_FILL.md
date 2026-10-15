# DOCX 表格无底色（发布 37）

> 发布 37（2026-10-15）：stable / MSRV 1.88.0 串行全套各 845 项、47 阶段通过并完成独立验收；724 项不同前端检查及 TypeScript / build、两工具链后端各 35 项通过。项目按用户要求在本次推送后暂停，不继续开发新功能。

## 模型与作者语义

简单正文顶层矩形表格的 tableCell / tableHeader 可使用 `cellFillMode: "none"`；唯一合法模式是 none。它与 cellFillRgb 按属性是否存在互斥，非法值、null、错位置及 mark 携带均拒绝。省略两者保持旧作者模型与导出字节行为：表头可沿用旧蓝色规范化，普通正文格通常不写 shd。显式 none 不写 shd；RGB FFFFFF 写白色 shd，不能用白色代替无底色。局部自动文字色是独立属性，底色修改不应隐式清掉它。

编辑器提供缺省 / 无底色 / RGB，保持混合选区、一次撤销、草稿、粘贴保护和 DOCX 往返；724 个不同前端检查及 40 个编辑状态 / 实际 CLI 往返已通过。模块与 DOM 替身验证不能当作真实浏览器、系统剪贴板、输入法或原生按键验收。

## 有界源证明

先排除任何 shd-looking 表格，沿用旧 RGB / 自动色审计。完全没有 header-looking 证据，或全部表头标志均为拥有合法直接归属 / 上下文的合法关闭值时，完整发现后沿用旧行为且不增加风险；不为零候选读取样式。未着色表格中的损坏、包裹、重复或非直接 header-looking 证据使整个新组回退。未完成的发现、包身份 / 解析 / 配额不确定不能冒充零候选。

候选只来自原始直接启用 tblHeader，不能从转换器推测。所有候选必须共同通过规范命名空间、唯一直接属性、支持结构、完整文字和零文字 bookmark / proofErr 路径证明。书签 ID / 名称在候选组内唯一，每对须在本表内闭合；不声称文档全局唯一。隐藏在属性 / tblGrid 内的文字与复杂节点不允许悄悄丢弃。raw → IR → model 精确绑定全部格、段、文字和两个 IR 表头标志，实际 IR 的 row_span / col_span 均须等于 1。

有效无底色须经实际 OPC 主部件、styles 关系、Content Types、docDefaults、默认身份、活动 basedOn 及段落 / 字符依赖共同证明。未使用的非默认有色样式不影响证明；活动或默认底色、未知依赖、歧义默认、非法路径均回退。直接或继承 numPr / numStyleLink / styleLink 不在证明范围。直接 cantSplit 须符合唯一、直接、叶节点、正确命名空间和规范 Boolean 语法；这是保守子集限制，不表示分页控制底色。继承的行标志本身不构成背景。

## 原子性、资源与优先级

全部新候选作为独立组：成功给候选全部单元格附加 none；任一证明失败不附加任何 none。旧 RGB / 自动文字色、行属性、几何、既有风险与硬拒绝保持。源失败只增加 `Some unshaded header-table fills are unproved and remain normalized`；已证明源但 IR / model 绑定或最终克隆验证失败，只增加 `Table no-fill identity could not be bound to converted content; original fill normalization retained`。成功不增加新警告。

保留旧两次模型验证及旧附件顺序，全部旧路径完成后才对克隆附加并验证，成功再提交；失败不把原本可导入的旧模型升级为硬拒绝。旧源高度 / 垂直几何组合拒绝和迟到转换错误仍优先。新的累计工作上限 2,000,000 与内部 64 级样式链防线不扩展旧公共入口：旧 20 级链门禁仍先到达。旧包 / XML / 资源拒绝仍保留；单元内部边界测试不等于公共入口可接受 64 级。

## 导出与可信重开

none 导出完成前检查实际生成 styles / 默认 / 依赖，测试注入缝覆盖生成样式污染，不能只靠事后篡改 ZIP 的负例。无关旧嵌套表格通过完整模型遍历占位，不能因另一张简单表有 none 就改变旧导出行为。

可信重开证明模型与包一致，不证明作者真实性。正文格缺省与 none 在合法规范化包中可等价，合法元数据互换可以接受；表头不匹配、非法模式 / 冲突、影响渲染部件的改动须拒绝。不提供密码学签名或作者真实性保证；现有成员哈希仍用于一致性校验。诊断参考保留陈旧内嵌模型并确实拒绝可信重开，不能当作产品输出。

## 真原件与固定消费者

Apache POI Bug60337 原件 SHA-256 `87f983bdd93b3b5b8469104a5b09ccff41e0f5c144060e13fd4db07bfd2e39da`，13,463 bytes。从发布 36 完整导入响应独立复制、只加 15 个 none 后，与预先封存的完整预期核对；实际 v5 导入 / 导出 / 可信重开符合该预期，旧风险完整保留。原三行五列表及末尾段落保持。

实际源 / 发布 36 基线 / 原始记录保留参考 / 新产品各生成一份新隔离 PDF，共 4 PDF / 4 页 / 8 PNG；72 / 144 dpi 六组唯一整页图均实际目视并独立复核。产品 / 参考完整文字、words、rawdict、含 seqno 的 texttrace、全部 drawings、字体 xref / 流、页面几何、内容流、Poppler 正文 / bbox body 以及 PNG 字节和像素精确一致，两分辨率差异像素均为零，无遮罩或容差。

主 XML 仅移除旧基线 5 个 header shd；PDF 少 10 次填充绘制。产品 / 参考非模型解压成员精确一致，未触碰的非主文档 / 非模型 local ZIP 记录精确一致。主 XML 的 6370 解压字节 / CRC 相同，Python 与 Rust 编码使压缩长度由 699 变 708，已事前允许并完整核对 header / CRC / 累计偏移；没有重写参考凑匹配。

原始失败仍记录：源列宽 1870 twip 规范化为 1800，50 个字符横坐标 / bbox 最多差 14 pt，右边框差 17.5 pt；源 / 产品整页光栅仍失败。基线 / 产品 texttrace 与边框 seqno 原始差值为 -10，不删除序号来宣称严格一致。产品 / 参考整个 PDF 字节和整个 Poppler HTML 字节不相等；CreationDate、PDF trailer IDs、DocChecksum 差异均保留。有限底色改进不等于源完整版式保真。Calibri 的云端字体替代与旧平台限制不变；未新增 Word、WPS、Windows / WebView 或原生打印验收。

## 已通过检查与发布门禁

- stable / MSRV 1.88.0 Rust 串行全套各 845 项通过、各 47 阶段（46 harness + 1 doctest）；845 个不同测试含原 819 + 新增 26，314 library + 531 integration，两工具链重复执行不相加为 1,690 个不同测试。MSRV 零 failed / ignored / filtered，最终 aggregate SHA-256 `a5c55752af0964b11928a9dbe9108693b7e797fcd332b93108c8e67e65691687`，独立验收完成
- 前端 666 + 88 − 30 = 724 个不同检查，TypeScript / build 通过；40 个编辑状态、122 CLI 调用和 40 个独立 XML 检查通过。DOM / 运输替身和真实 CLI 不等于真实浏览器、原生输入法 / 剪贴板 / 打印
- 两工具链生产后端各 35 项通过；独立核对 694 个 worker / CLI 命令。七项原生 pkg-config 依赖缺失，未验收原生构建 / GUI / IPC
- 独立源 / 包 / 消费者验收核对 3,588 个命令及输入 pin；包含 1,894 历史完整响应、302 整包 golden、208 新 audit/import 端点、104 inspect 响应及 111 源 / 作者包。inspect 只豁免已存在 u64 elapsed_ms，原始响应保留
- 真原件 3×5 共 15 格显式 none；仅消除旧基线 5 个蓝色表头填充。固定 LibreOffice 产品 / 独立参考文字、几何、字体 / 内容流与 72 / 144 dpi 光栅精确相同；整个 PDF 字节不相等。源列宽 1870→1800 twip，最多 14 pt 文字 / 17.5 pt 右边框差异与源光栅失败保留
- 原 portable bundle 在同一云端主机、stable / MSRV 固定 CLI 下分别通过 positive 和四个负例（旧 release36、损坏 fixture、损坏 expectation、陈旧 XML/哈希）。不是 fresh-host 执行；去私有路径公开副本现也已在两个冻结 CLI 下完成同样五项资格验证

最终构建收据为 `tmp/docx-cell-no-fill-final-build/attempt-v2/FINAL-RECEIPTS.json`；身份桥 `acceptance-identity-bridge.json` 证明 649 源身份相同且 stable 最终 CLI 与已接受 v5 的 SHA-256 同为 `8484e6b0efbfc98b8fb2dce18c708058e706fc882d6482d4702b46c39617a75d`。2026-10-15 独立只读复核现接受最终构建 / 身份连接：1,096 个文件身份记录无差异，含 649 源与两工具链各 213 个生产链接记录；命令原始输出散列、Cargo artifact map 和冻结 CLI / rlib 均匹配。旧收据的待复核标签原样保留，以新增复核结果补充，不倒写旧证据。这不是全套测试接受。stable 构建复用与 v5 相同的 fresh 产物，MSRV 实际重新构建。

静态门禁须保留范围：最初 fmt --all 返回 1，仅 81 个 vendor 路径且全部与 HEAD 字节相同；rust-wps package fmt 返回 0。严格 all-target Clippy 原始两次均返回 101，各仅一个已知诊断：stable 的 `clippy::chunks_exact_to_as_chunks` / `src/spreadsheet_xls.rs`，MSRV 的 `clippy::uninlined_format_args` / `src/presentation_import.rs`。完整非空主文件归属核对后，只允许相应单一 lint 的重试返回 0；不能写成无 allowance 的严格全通过。

源 / 消费者独立接受：`tmp/docx-cell-no-fill-terminal-review/ACCEPTANCE-v1.json`；前端：`tmp/docx-cell-no-fill-editor-next/completion-v2.json`；后端：`tmp/docx-cell-no-fill-backend-review/TERMINAL_ACCEPTANCE-v1.json`。这些是历史审计定位，不是公开 portable 检查的运行依赖。

旧 portable 资格报告 SHA-256：stable `5967b7446d9a5be6ebb1816fe32492f92223699dfdb71e980542fb7ed5a7b351`；MSRV `151197e0c64f124c6fad284fe4db1f9262bc63fd8c337764b8822f2e8c5532f7`。公开包位于 `tests/fixtures/docx-cell-no-fill/`，含原件、Apache LICENSE / NOTICE、独立完整预期、基线、原始源证明与标准库 runner。需要 Python 3.8+ 和本机已构建 CLI；不附带旧大型私有验证器，不宣称运行 portable 包即可重放消费者渲染或全部历史覆盖。

所有早期尝试、源修正、旧二进制 / rlib、原始失败与诊断保留；不删除旧 REGRESSION 历史。最终 MSRV 全套与独立验收已完成；公开包资格验证结果另在回归记录中给出。

公开 bundle 最终资格验证：stable-8wgbybet 报告 SHA-256 `633eb508f1eca49ffbfdb02a06603bd441f3058bcf94c04171974f66dff7683a`；msrv-qhcru12a 报告 SHA-256 `df5a13f50234c1649418de2a8dbb0aef57df33f35b8170fe88537a4991945603`。两次均直接运行已提升的 `tests/fixtures/docx-cell-no-fill/check.py`，positive 返回 0，旧发布 36 / 损坏原件 / 损坏完整预期 / 陈旧 XML 与哈希四项负例分别返回 1，bundle 与 frozen CLI 前后散列不变。仍为同一云主机执行，不是 fresh-host、渲染或原生 GUI 验收。
