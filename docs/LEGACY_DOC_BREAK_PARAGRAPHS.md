# 旧 DOC 独立分页段与段落标记

本增量保留源文件明确要求的“分页后把段落标记留到下一页”语义，以及该独立分页段的 Latin 字体、字号与完整间距。保留原有 `pageBreak` 原子身份，不添加空段、伪文字或坐标补偿，不修改字体替代、vendor、全局 compatibilityMode 或其他格式。

## 模型与新编辑语义

两个可选属性彼此独立，都是显式作者状态：

- 根 `doc.attrs.splitPageBreakAndParagraphMark`：只接受 `true`；省略表示原有默认，`false` / `null` / 其他类型拒绝
- 顶层 `pageBreak.attrs.breakParagraph`：完整精确对象 `{font:{family,halfPoints},spaceBefore,spaceAfter,lineSpacing}`；不得缺字段、多字段或使用 `null`
- 字体为现有 1–31 UTF-16 单元的字面名称，无首尾空白、控制字符；`halfPoints` 为 2–3276 整数；段前 / 后为 0–31680 twip 整数；倍数行距为 120–2400 的 1/240 行整数

因此仅 flag、仅 metrics、二者都有、带 / 不带 metrics 的混合原子以及有 flag 无分页的文档都是合法状态。根 flag 对文档内所有手动分页生效；在这样的文档里新插入的裸分页使用原有 profile 默认字体 / 间距并遵循当前文档的 split 语义。剪贴板保留完整原子 metrics，目标文档的根状态不变，不从单个原子偷偷切换全局兼容语义。

编辑器保留加载 / 文字编辑 / 选择 / 删除 / 撤销重做 / 草稿 / 导出 / 可信重开中的原子数据。删除最后一个分页可留下没有活动分页的根 flag；撤销及后续插入无需猜测旧状态。没有新增数值控件。

## DOC 源导入更严格

作者模型的合法组合不等于源证据已充分。自动导入只在同一最多 64 MiB 源快照中满足以下条件时，一次添加根 flag 和全部独立分页段 metrics：

1. 既有完整 CFB / FIB / FFN / STSH / CLX / CHPX / PAPX 的有限 Latin、未继承 / 未链接内置 Normal、正文 / 段落身份证明通过
2. 有效 FIB 为 `0x112`，声明 DOP 长度恰为 `674` / `690` / `694`，DOP+512 DWORD 的 bit 27 (`0x08000000`) 明确置位；所有既有 DOP 副本与 framing 验证仍执行。真正 `0xC1` / 500-byte Dop97 没有这个字段，不能读取尾外字节或推断消费者默认
3. 每个分页段源内容精确为 `0C 0D`。PAPX 段身份保持完整，break 和紧随的 CR 各自定位到覆盖它们的 CHPX；两者可以跨 run，但最终 Latin 字面字体 / 字号必须相同。不能借用后续正文的字体
4. 默认、Normal、直接 PAPX 按既有规则解析，段前 / 后 / 倍数行距必须完整支持；合法但不支持的最小 / 固定 / 超范围行距不部分保留
5. 精确 PlcfSed 范围结束于已证明的正文 CP，所有节界通过既有校验；每个候选 break 的实际 CP 不得是节结束标记。页面几何是否支持与这项判断分开，不能用纸张 / 网格是否支持来代替 break 身份
6. 所有源块与当前编辑副本顺序、文字、原子形状精确绑定，完整候选模型验证通过。已有未证明根 flag 或 atom attrs 拒绝，后续冲突不能提交任何部分修改或提示

在其余既有保护允许导入时，只要一个源分页不满足条件，全部源分页都保留旧裸原子，根 flag 不添加；正文已证明的字体 / Normal / 间距及独立页面结果不撤销。显式 true 但 metrics / 节界未证明时，增加具体全局回退原因。显式 off、缺 DOP / Dop97 字段或没有分页的文件沿用旧导入结果；完整 typography Unsupported 路线的整份旧响应不变。损坏结构及既有导入拒绝继续保持，不能借回退掩盖；例如独立二进制缺节 / 分节结束控制仍被旧、新 CLI 同样拒绝。

## OOXML 与信任边界

带 metrics 的原子仍写成一个 `w:p`：`w:pPr/w:spacing`、段落标记 `w:pPr/w:rPr`，以及带同样字体 / 字号的 `w:r/w:rPr` 后跟 `w:br w:type="page"`。字体仅写已证明的 `ascii` / `hAnsi` 和 `w:sz`，不扩大脚本证明。无 metrics 时保留原裸分页 XML 字节。

根 true 时只增加 `w:settings/w:compat/w:splitPgBreakAndParaMark`，按原有规则纳入 settings 部件、Content Type、relationship 和完整哈希。已有间距但没有该 flag 时仍写原来的空 settings；没有任何新属性时旧模型与包字节不变。没有全局 compatibilityMode、更换默认字体或 materialize 空段。

自有 DOCX 的完整 typed model 校验、所有部件集合 / 哈希和确定性重编码继续执行，包括条件生成的 settings；仅改模型、仅改设置或字体 / 间距，即使同步伪造哈希也不能获得可信重开。外部 `import-docx` 不等于可信 `read-docx-model`；此增量不扩大普通外部 DOCX 的 split 源导入能力。

## 未改造真实原件与独立实验

[Apache POI SampleDoc.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/SampleDoc.doc)，固定 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`，27,136 bytes，SHA-256 `717585c0a88f20862c86220163dd03da87cd743970354c6b29add32725f2e833`。原件不改写、不随本仓库重新分发。

独立二进制解码定位 DOP table offset 5673、长度 674；DOP+512 字节 `0800fcfd` 的 bit27 为 true；CP72..74 / FC2120..2122 为 `0c0d`。该段及 CR 为 Calibri 22 half-points，间距 `0 / 200 / 276`；后文才变为 Arial Black 32 half-points。源的七个块身份不变。

实现前的独立四格 OOXML 控制在固定 LibreOffice 中得到第二页首基线：

| 根 split flag | 分页段 metrics | y (pt) |
| --- | --- | --- |
| 无 | 无 | 89.10076904296875 |
| 有 | 无 | 114.05078125 |
| 无 | 有 | 89.10076904296875 |
| 有 | 有 | 114.55078125 |

源为 `114.55078125`。二者共同保留后解释了原来 25.45001220703125 pt 的前导位移；第一页完整字符 / 坐标 / 光栅不变。该实验是独立于产品实现的语义对照；最终产品与独立参考的完整比较、实际执行计数及冻结身份以 [REGRESSION.md](REGRESSION.md) 为准。

Arial Black 未安装时，固定消费者仍可能对 DOC 源选择 NotoSans、对最小 DOCX 选择 NotoSerif。这个独立字体替代差异尚未修复，不能宣称 SampleDoc 第二页光栅或整份 Office 文档完全保真。历史缺 DOP 的非零相邻段距控制中，消费者相加 / 取最大值的 12 pt 差异同样未解决。

## 最终独立验证

冻结候选 `a3cb41b089460be59712c7fa70858a8a4ed5afd8e32f796cc44ddd06324ffe83` 的独立检查通过 365 个源输入（306 历史 + 59 新二进制控制）：19 保留、14 整组回退、171 原样成功路线、161 与基线相同的拒绝。另有 21 合法作者组合、63 非法模型、28 篡改拒绝、429 完整可信重开、408 不覆盖与 40 独立 OOXML 对照。所有原件、冻结基线与历史证据保持身份。

14 份新 PDF 均由逐文件隔离固定 LibreOffice 生成：5 组产品 / 独立参考的文字、字体、spans、逐字符坐标和完整光栅相同；4 份独立作者、已安装字体的二进制源新渲染分别与产品相同，覆盖普通、文首、文末、连续分页。真实 SampleDoc 源 PDF 是精确固定身份的既有渲染，明确复用 1 份；其第一页与新产品相同，第二页全部 61 个 y 坐标及首基线 `114.55078125` pt 相同，缺失字体替代差异仍保持。

33 张完整页 PNG 被划入 10 组 SHA-256 完全相同的页面，每组一张全页实际查看，无裁切 / 重叠，预期空白页与源一致。最终源 / 包 / 消费者报告为 `tmp/doc-break-semantics-verify/break-run-bdz2jxlj/report.json`，SHA-256 `65e62f6b9fee9361282d58b536108f5009e5cdc4a09a7565209ed273364a9165`；图像复核记录在同目录 `visual-review.json`。完整门禁、纠正过的控制预期与平台限制见 [REGRESSION.md](REGRESSION.md)。

## 编辑器 / 浏览器打印限制

编辑器仍显示可见分页标记；现有浏览器打印使用零高度的分页原子，不模拟下一页的段落标记行框。页面内标记与不可变打印快照保留数据，并明确提示该限制；原生 PDF 导出前也提示。没有通过任意 CSS 高度、空行或 y 坐标修补声称 DOCX 消费等价。

模型 / Tiptap / DOM 运输测试不等于真实浏览器、Windows WebView2、系统打印、Microsoft Word 或 WPS 验收。这些平台的实际结果未运行前不得写为通过。本轮连接的 Windows 桌面检查在启动即遇到使用额度限制，尚未读取文件或运行测试；随后用户要求不使用本地机器，后续继续仅在云端验证。

## 规范依据

- [MS-DOC Copts](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/93788691-b8f0-44a8-8fee-a8d6491dc0c4)：`fSplitPgBreakAndParaMark` 映射 OOXML 同名行为
- [MS-DOC DOP 版本选择](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/841b5f72-487e-4fe7-8657-ec90d5af8750)、[产品行为 footnote 191](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/54781214-a7e3-4f7a-9bcf-33fd7b965b3c)：有效布局与 Word2007+ 支持范围
- [OOXML splitPgBreakAndParaMark](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.splitpagebreakandparagraphmark?view=openxml-3.0.1)：段末分页后把段落标记移到下一页一行
- [OOXML 段落标记 rPr](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.paragraphmarkrunproperties?view=openxml-3.0.1)、[MS-DOC PlcfSed](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/68a959a6-11e0-4f3e-9a99-76ca8cc4dddc)：标记格式与节界身份
