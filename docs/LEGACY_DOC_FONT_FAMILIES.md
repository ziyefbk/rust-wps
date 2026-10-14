# 旧 DOC 的有界 Roman / Swiss 字体类别保留

## 2026-10-14 发布 35 补充：外部 DOCX 的边界已另行扩展

下文保留发布 34 的旧 DOC 实现与当轮证据；其中“通用外部 `import-docx` 不读取 fontTable”的描述是该轮历史状态。发布 35 已新增独立的 [外部 DOCX 字体类别源证明](DOCX_FONT_FAMILIES.md)：实际 OPC 关系、完整保留字体集合、活动样式 / 主题与逐范围模型身份共同通过时，可保留 Roman / Swiss；不是任意字体表导入，也不信任内嵌作者模型。旧 DOC FFN 规则未改：按索引引用的未使用记录与 DOCX 按名字引用的完整 ASCII 折叠类有不同歧义边界。

原 `tests/importing.rs` 的 exported blank.doc 外部重导入仍省略类别，因为实际保留的真正空段为 Arial 22，源表却只有 Times New Roman / roman，完整集合无法证明；该期望不再代表所有外部 DOCX 一律省略。新增路径沿用同一严格模型 / writer / 可信重开，保留浏览器与原生替代限制。发布 35 的精确门禁与仍失败的源颜色 / 光栅见 [REGRESSION.md](REGRESSION.md)。

## 发布 34 的实现与验证记录（2026-10-14）

本增量只保留源明确声明的字体类别，不重命名字体、不安装字体，也不按名字、粗体、PANOSE、已安装替代字体或某个渲染器规则猜测类别。`fontFamily` 字符标记仍表示字面字体名；新增根 `fontFamilyMetadata` 表示字体表的 Roman / Swiss 分类，二者含义不同。

## 源证明与回退

前提仍是 [旧 DOC 字体](LEGACY_DOC_FONTS.md) 的同一份最多 64 MiB 字节快照、完整 Latin 正文 / Normal / CHPX / PAPX / 精确文字与分页身份。没有扩大复杂脚本、样式、表格或通用 DOCX 字体表导入能力。

- 从已验证 FFN 的 family 位取值：1 为 `roman`，2 为 `swiss`。FFN 未定义位继续忽略；非法 family 6 / 7 保留此前的损坏拒绝
- 只收集最终实际输出的非空正文文字，以及已经独立证明并保留的分页段字体。两个有效 Latin 字体槽须字面名和类别都相同；分页符与尾 CR 的两个槽也须类别一致
- 普通段落 CR、真正空段落、未保留度量的裸分页符、未使用字体表记录不提供新类别证据。未使用记录的合法其他类别、同名冲突或大小写别名不能阻止已证明文字的类别保留；原有结构损坏检查仍有效
- 精确相同名字 / 类别的活动重复记录合并。活动同名字体类别冲突，或不同字面名仅 ASCII 大小写有别，无法可靠写成一张文档级字体表，因此整组类别回退
- 合法 family 0 / 3 / 4 / 5（auto / modern / script / decorative）不在本轮模型内；任何实际输出字体采用这些值时，全部新类别元数据回退，并说明原因。已证明的字面字体名、字号、Normal、间距、页面及分页段属性不撤销
- 既有 `validate_font` 已对两个 Latin 槽的不同字面名字回退整份 typography，大小写不同也属于此旧边界；本轮的“类别单独回退”只适用于原 typography 已证明的来源
- 整份 typography 未证明时不增加类别诊断，历史路线保持。源与编辑文字不符或完整候选模型失败时，文档及全部诊断共同回滚

每次候选先完成全部正文绑定和类别收集，再共同附加、完整校验与提交。没有先写一部分可用类别的路径。原 1 MiB 累计属性字节、256 字体、10,000 run / 编辑节点等配额继续有效；类别表最多 256 项，不新增流读取或重开源路径。

## 独立作者模型与包

根 `doc.attrs.fontFamilyMetadata` 可省略；提供时为 1–256 项数组，每项严格只含 `name` 和 `family`。`name` 沿用不超过 31 UTF-16 单元的字面名称检查；`family` 只接受小写 `roman` 或 `swiss`。null、空数组、未知字段、未知类别、非根位置及任何重复 ASCII 大小写折叠名字均拒绝。作者数组不要求源证明，也不是来源认证标志。

未被当前正文使用的合法作者条目仍有效：删字、改字体或清空内容不会悄悄清除文档字体信息，撤销后可恢复原意。作者条目顺序保留在模型中，实际 fontTable 按未改写名字确定性排序。不会进行 Unicode 大小写折叠或名字规范化；非 ASCII 别名的消费者识别差异不在保证范围内。

仅当元数据存在时，writer 增加：

1. `word/fontTable.xml`，每个字面名只写一个 `w:family`
2. 主文档到该部件的内部 `fontTable` 关系
3. 对应 fontTable Content Type Override

新部件及关系 / 内容类型全部纳入既有完整部件集合、SHA-256 和重编码验证。同步篡改哈希仍不能让模型与 XML 不符的包可信重开。没有字体嵌入、字体文件、替代名字、粗体推断或额外 settings；缺省旧 V1 / V2 模型应保持历史整包字节。

可信自有 DOCX 的 `read-docx-model` 保留该元数据；通用外部 `import-docx` 仍不读取 fontTable 类别，即使输入恰好由本应用导出也不等同于可信重开。现有 `tests/importing.rs` 对这条外部路径明确期望省略类别表，其他文字 / 段落 / 页面检查保持。一般外部 DOCX 类别导入需要另做 OPC / 活动字体 / 源绑定证明，不在本轮扩展。

字体类别通过编辑器根状态、草稿、撤销、DOCX 导出和可信模型重开保留。普通 HTML 剪贴板不能替换目标文档根类别表；没有 CSS 通用字体、系统字体安装或渲染器特定选择。编辑器 / 打印 / PDF 导出明确提示尚未据此调整缺失字体替代，不能把 DOCX 消费结果说成浏览器或原生打印验收。

## 独立证据与限制

实现前的隔离实验以发布 33 的未修改产品包为基线，由独立 ZIP 修改只添加类别表和必要 OPC 关系。固定 LibreOffice 中，未改造 Apache POI SampleDoc 的新鲜源 DOC 为 NotoSans-Regular；无表、name-only、非 family 元数据及故意错误 Roman 控制仍是 NotoSerif-Regular。仅正确 Swiss 类别即可恢复两页文字、字形 / 逐字符位置与 72 / 144 dpi 全页光栅精确相等。Arial Black 仍未安装。

SampleDoc 的名字前缀会触发该消费者的 DOC 特例，因此另外独立编写了不存在字体 `JwAbsentFamilyProbe62` 的 Swiss / Roman 二进制对照，完整 CFB 只差一个 family 字节。各自正确类别 DOCX 与源精确相同，反向类别显著改变横向度量及光栅。真实 simple.doc 和已安装 Liberation Serif 控制保留原效果。共 23 个新 PDF，没有复用源渲染；这些 ZIP 干预不是可信产品重开证明。

实验最初的完整元数据控制仅 PDF 内部对象号 19 / 24 对换，使原严格报告保持失败状态；额外分析明确记录该编号差异，嵌入字体流哈希、其余资源字段、字形 / 坐标 / 光栅仍相同。Swiss-only 控制通过原始严格比较，不需要这一编号资格说明。实验报告与产品增量验收应分开记录。

产品最终命令、冻结身份、独立源 / 作者 / 篡改 / 历史包及消费者结果见 [REGRESSION.md](REGRESSION.md)。本轮不是原生 Arial Black 外观、所有字体替代、Microsoft Word / WPS、Windows WebView2 或完整 Office 版式兼容承诺；未扩大对复杂脚本、动态主题、其他 FFN 元数据和通用外部 DOCX fontTable 的支持。

规范锚点：[MS-DOC FFID](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/f0f489b1-0ca5-49d0-9826-5ad811da87c7)、[MS-DOC FFN](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/ff407d64-3478-4b56-9b98-6dbcfc66a4ae)、[OOXML FontFamily](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.fontfamily?view=openxml-3.0.1)。这些字段描述替代信息，不提供可用字体或保证跨平台渲染。
