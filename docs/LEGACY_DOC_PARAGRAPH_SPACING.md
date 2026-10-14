# 旧二进制 DOC 的编码段落间距

本增量在已有完整 [Latin 字体 / 字号](LEGACY_DOC_FONTS.md) 与 [编码 Normal 身份](LEGACY_DOC_NORMAL_PARAGRAPHS.md) 源证明成功后，逐段保留源文件编码的段前 / 段后 twip 和受支持倍数行距。只复用现有段落属性与 DOCX writer，不改变 vendor、编辑模型、UI 或普通 `convert_ir(..., false)` 路线。

这是编码值的有界恢复，不是源文档物理行框、分页或完整版式证明。

## 默认、覆盖和合法性

同一份最多 64 MiB 的源快照经过原有 FFN / STSH / CLX / CHPX / PAPX 完整证明后，使用格式规定的 `0 / 0 / 240` 初值，再依次应用未继承 Normal 的段落 UPX、该段直接 PAPX。相同分量最后一项优先；覆盖段后不会重置段前或行距。整数原值直接保存，不经过浮点数或 vendor 的行距解释。

- `sprmPDyaBefore`（`0xA413`）与 `sprmPDyaAfter`（`0xA414`）：无符号 16 位整数，合法范围均为 0–31680 twip
- `sprmPDyaLine`（`0x6412`）：四字节 LSPD，前两字节为 `dyaLine`，后两字节为 `fMultLinespace`；flag 只能为 0 或 1
- `dyaLine=0x0000..0x7BC0`、flag=1：`dyaLine/240` 倍行距；现有模型只支持 120–2400，包含不能用有限小数精确表示的 241
- 相同非负范围、flag=0：最小行距，受单倍行距下限约束；只有 `(0,0)` 可等价表示为单倍 240
- `dyaLine=0x8440..0xFFFF`：固定行距，值为 `65536-dyaLine` twip，**不论合法 flag 是 0 还是 1**
- `(0,1)` 是合法的零倍行距，不能误当单倍；`0x7BC1..0x843F`、非法 flag、过大段前 / 段后、截断操作数均视为损坏

每个活动已知声明都验证，不能被后续覆盖或另一项 Unsupported 掩盖。有效的最小 / 固定 / 超范围倍数若最终被合法支持值覆盖，不会阻止该段最后的支持值。活动引用样式及 PRC / 嵌套或错误上下文中的已知间距操作同样不能隐藏损坏，但这些路径不会因此成为新的支持子集。

## 逐段整组应用与原子保护

Normal 三个最终值能完整表示时，添加全部显式 `spaceBefore`、`spaceAfter`、`lineSpacing`，包括零值和源格式默认值；真正空段同样处理。最终行距合法但不能表示时，该段三个间距属性全部沿用以前的规范化结果，不部分应用段前 / 段后，也不撤销已证明的字体 / Normal 身份。

报告分别列出保留 / 规范化段数，并按最终不支持值给出具体原因和段数。整份字体 / Normal 证明未通过时，不附加间距承诺，保留原 Unsupported 响应；新识别的损坏已知间距声明则明确拒绝。

间距与字体 / Normal 修复在同一候选模型事务内执行：先精确核对源段落范围、全文和分页，再更新对应节点，完整模型校验成功后一起提交模型和诊断。迟到文字冲突、分页身份冲突或模型失败不能留下部分修改。非间距属性、根属性、所有现有字符标记、精确空白、真正空段及分页顺序保持；不添加伪空格。

其他操作码仍不在白名单内，包括自动 / 上下文 / 按行计间距、网格、大纲、reset、继承 / 链接样式和列表 / 表格。该改动没有开启普通 legacy IR 的间距转换。

## 两份真实原件与保留边界

真实样本来自 Apache POI 固定 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`，原件未修改、未重新分发，也不宣称已完成单独再分发授权审查：

- [SampleDoc.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/SampleDoc.doc)，SHA-256 `717585c0a88f20862c86220163dd03da87cd743970354c6b29add32725f2e833`：Normal UPX 原字节 `000012641401010014a4c800`，六个普通段落的最终编码值为 `0 / 200 / 276`；七个 PAPX 均无直接间距覆盖，另一个是独立分页段
- [simple.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/simple.doc)，SHA-256 `4876e828e62d490fa188c7c4e47013e2e0dfdf02f5903dfd374bc0d2b2049ca1`：Normal 与直接 PAPX 无间距声明，采用源格式默认 `0 / 0 / 240`

独立二进制解码和独立 LibreOffice 转换的样式 XML 分别建立源预期；不能用新增产品 parser 的结果作为 oracle。旧编辑默认 `0 / 120 / 360` 仅在本证明子集内被替代。

独立分页仍是没有属性的 `pageBreak` atom，writer 输出普通分页段，不能保存该源段的间距或段落标记字体。真正空段可保存间距，但其源基线字体仍未证明。两者有单独明确提示。SampleDoc 第二页的缺失 Arial Black 替代仍可能为源 NotoSans / 输出 NotoSerif；不能据普通段落间距值相等宣称第二页顶部或整页相同。simple 的源 Letter / 左边距 90 pt 与应用默认 A4 / 72 pt 差异仍在。

最终固定 LibreOffice 消费者中，SampleDoc 第一页源文件与新输出的字符属性 / 坐标和完整光栅精确相等；相邻基线间距恢复为 `25.45001220703125 pt`，冻结 Normal-only 输出的 `0.70001220703125 pt` 间距误差降为零。该结果只适用于受测第一页，不能扩大为第二页或任意 DOC 的版式承诺。

**正确编码值仍可能留下物理间距差异。** 独立作者的已安装字体控制 `spacing-installed-margins` 中，源二进制 DOC 的相邻段后 / 段前在该消费者中相加，而现有 DOCX 最小 settings 的相邻间距取最大值；两侧各 12 pt 时，源相对基线为 `0 / 35.54998779296875 / 51.0999755859375 pt`，输出为 `0 / 23.54998779296875 / 39.0999755859375 pt`，仍差 **12 pt**。产品输出与完整独立 DOCX 参考精确相等，并不意味着它与该二进制源的物理布局相等。独立移除参考的 settings 及其两处 OPC 登记后，额外 12 pt 再次出现，单独验证相加 / 取最大值的差别；没有改变 writer 或兼容模式去掩盖这个限制。

源节设置、DOP、文档网格、兼容选项、命名样式动态关系和所有排版机制均未进入本证明。历史字体-only / Normal-only 报告及原始失败记录保持，只有受本次证明覆盖的编码间距预期由此增量取代。

## 验证方法

Rust 控制覆盖默认与边界、逐分量覆盖、重复最后优先、最终支持分类、负固定值的两种合法 flag、零最小 / 零倍区别、活动损坏即使被覆盖也拒绝、原始 STSH / 各 PAPX 不同声明、空段 / 精确空白 / 分页、完整非间距模型保持和模型 / 诊断原子回滚。

独立文件控制从冻结 Normal-only CLI 建立整份模型预期，只添加独立源证明允许的三个间距属性与精确新诊断。DOCX 检查实际 `w:spacing` / `lineRule="auto"`、完整可信重开、不覆盖和篡改拒绝。显式间距沿用已有最小 `settings.xml` 与 OPC 关系，独立 XML 参考也需完整建立这些部件，不能遗漏后将消费者相加 / 取最大值的差异归因于产品。

最终独立检查为 182 输入、207 次完整可信重开、414 次不覆盖、18 个目标部件归因明确的篡改拒绝；最终消费者重新执行同一来源矩阵并新渲染 91 份 PDF，其中 41 对产品输出 / 独立 XML 参考的全文、字体 / 字号、span / 字符、坐标和完整光栅精确相等。前述 12 pt 源布局差异仍保留，不能把参考相等解释为源版式一致。

早期独立参考因 OPC `ns0` 前缀序列化被固定消费者拒绝加载；隔离控制证明必须同时规范化 Content Types 与 document relationships，展开名称后的 XML 树和其他部件（包括 `document.xml`）均不变。只修正参考作者，原失败报告保留。最终执行计数、复现命令、工具链 / CLI / checker / 报告身份及真实源 / 独立消费者结果见 [REGRESSION.md](REGRESSION.md)。没有新的实际浏览器、Windows WebView2、原生打印、Microsoft Word 或 WPS 验收。

## 规范锚点

- [MS-DOC Paragraph Properties](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/484822ee-a9d9-4af4-8423-29fda67a6a58)：间距 SPRM 与格式默认
- [MS-DOC LSPD](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/fcdd2b79-9d47-4e0c-b1fd-705aeab88e41)：符号、合法区间与 flag 语义
- [Formatting order](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/d8b66123-1a3d-4e06-94c5-5ab16e9b6417)、[Last-applied precedence](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/9ac56e29-8488-4b0a-a009-86a26e2f175e)：样式、直接属性与最后优先
