# 当前跨格式回归记录

## 当前：有限段落左右、首行与悬挂缩进（2026-10-13）

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
