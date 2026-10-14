# 旧二进制 DOC 的有界字体与字号保留

后续[编码段落间距增量](LEGACY_DOC_PARAGRAPH_SPACING.md)在相同完整证明内恢复受支持间距值。本文保留原字体 / Normal 身份轮的验证与当时失败记录；分页段 / 空段基线字体、节网格、页面和字体替代限制仍有效。

本增量在应用侧对同一份最多 64 MiB 的 DOC 字节快照建立独立源证明，恢复受支持正文文字的**字面 Latin 字体名和有效半点字号**。不修改 `vendor/office_oxide`、编辑模型、DOCX writer 或 UI；并非完整 Word 样式解析器，也不承诺源页面或字体外观一致。

## 支持范围与三种结果

- Word97 / Word2007 有效 FIB 布局；Word2007 必须按 `nFibNew` 判断，不能仅凭基区的 `nFib=0x00C1` 判断版本
- 一块完整正文的压缩 Windows-1252 piece，无 PRM/Prc；明确 English Latin 来源与文字；完整、单调且与原始 CR 段落相符的 CHPX/PAPX 覆盖
- 活动段落为未继承的 Normal，默认字符样式存在且无修改。未使用的合法表格 / 列表样式不阻止这两份真实正文原件
- STSHI 的默认字体槽、Normal 字符 UPX、直接 CHPX 按顺序覆盖；缺省字号使用格式规定的 20 half-points，不能推测为编辑器的 11 pt
- 保留原名字（例如 Times New Roman、Calibri、Arial Black），不写入 LibreOffice / fontconfig 的本机替代名。受支持的手动分页符与既有颜色等标记保持；确切字体变化处拆分文本，不重写其他文字或样式标记
- 仅对非空文字附加格式。真正空段落的基线字体、复杂 / 东亚 / 双向文字、继承样式、显式字符样式、相对字号与 reset / fixup、一般多 piece、表格 / 列表 / 链接等均不在新证明内

结果不是静默的部分证明：

1. **证明成功**：所有适用正文文字添加有效名字 / 字号；报告明确说明局部字体替代、页面与其他格式可能不同
2. **已审计结构有效但超出子集**：缺少 / 零条目字体表（适用索引必须为 0）和隐式缺省字符样式缺失均明确属于此类；FIB 安装语言字体覆盖不能被当作源名字证明。保留原实验性编辑副本路线及原模型，新增具体“字体名和有效字号未保留”原因，不给新保留承诺
3. **活动结构损坏或身份冲突**：字体表 / 活动样式 / FKP / 长度 / 索引有误、解析器直接字号不同、源正文和编辑文字不相等时拒绝编辑副本；不降级为默认字体

未知或不支持的 SPRM 仅完成有界的通用操作数 framing，不是全部 Word 操作码的完整语义验证。已知活动字体 / 样式 / 引用 / reset 包装采用更严格的索引、嵌套和范围检查；保留原实验性风险提示。

应用模块 `src/legacy_typography.rs` 用公开 CFB reader 读取被选中的表流；验证相关流身份、声明大小、截断 / 容器警告和 FIB 分区后才读取字段。STSH/FFN/CLX/FKP 的累计表 / 属性字节最多 1 MiB、字体最多 256、样式最多 4093、源字符最多 1,000,000，run / 最终编辑节点最多 10,000，活动样式链的累计访问最多 10,000，嵌套字符修改最多 8 层。源 / 编辑身份采用精确文本和分页顺序，不 trim、不模糊匹配、不只比较字符数量。拆分采用单调双指针，最后再次校验完整候选模型。

## 两份真实来源与仍然存在的失败

缓存原件来自 Apache POI 固定 revision `ae62bb5116b9aee19ebd5834e3a82066132c9f7f`。它们是公开回归文件，不是用户生产文档；未改写、未提交原件，也没有重新宣称已完成单独再分发授权审查。

| 原件 | SHA-256 | 独立源有效字体 / 字号 |
| --- | --- | --- |
| [simple.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/simple.doc) | `4876e828e62d490fa188c7c4e47013e2e0dfdf02f5903dfd374bc0d2b2049ca1` | Times New Roman / 20 half-points（10 pt），来自 STSHI + 规范缺省 |
| [SampleDoc.doc](https://github.com/apache/poi/blob/ae62bb5116b9aee19ebd5834e3a82066132c9f7f/test-data/document/SampleDoc.doc) | `717585c0a88f20862c86220163dd03da87cd743970354c6b29add32725f2e833` | 第 1 页 Calibri / 22，第 2 页 Arial Black / 32；后者为直接 CHPX |

字体轮验证时，SampleDoc 首段真正是 Normal，旧读取器仍按短首行启发式转成 Heading1。显式字号已恢复 11 pt，标题的粗体 / 间距仍不同，该轮报告保留这一失败。后续 [Normal 段落身份](LEGACY_DOC_NORMAL_PARAGRAPHS.md) 在完全相同的已证明子集内修复编辑副本的编码身份；原始字体-only 报告不重写，只读 / IR 标题猜测仍保持。源间距与页面依然不代表完整首段修复。

固定 LibreOffice 消费时，原始 Arial Black DOC 与最小导出 DOCX 分别选用 NotoSans 和 NotoSerif；字体族元数据没有传到 DOCX，是一个可能原因，尚未独立隔离证明因果。字面 Arial Black 保持，实际替代字体不同的失败保留。simple 的原 Letter 页面 / 90 pt 左边距与应用 A4 / 72 pt 也不同。原布局、颜色校准历史失败和这些字体替代限制均未提高容差隐藏。

## 独立验证方法

`scripts/check-legacy-doc-fonts.py` 使用 Python 标准库直接读取 CFB / FIB / FFN / STSH / CLX / FKP，并独立编码明确标注的作者二进制控制；不通过新增产品 parser 制造字体期望。正向包括规范缺省、Normal 样式字号、直接覆盖、段内多字体 / 字号、Windows-1252 智能标点和正常空段 / 分页。负向覆盖活动字体 / 样式 / 表边界、变量操作数、版本、覆盖缺口、身份与资源上限；未知有效操作单列为有提示的历史 fallback。

实际导出检查四个 `w:rFonts` 槽和 `w:sz`，完整可信重开、现有目标 / 原件不覆盖及源哈希；每次 LibreOffice 消费使用全新进程 / profile / HOME。受控“仅改变字体 / 字号”的参考与产品 DOCX 独立生成，并比较逐页全文、实际字号与字体资源。字面已安装字体及明确标注的匹配替代作者控制要求相同的消费资源与字形推进；真实原件另保留不一致，不能用受控参考替代原始源结果。

最终执行身份、计数、报告和历史跨格式重放见 [REGRESSION.md](REGRESSION.md)。没有新的实际浏览器、Windows WebView2、原生打印、Microsoft Word 或 WPS 验收。

## 规范锚点

- [FIB](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/9aeaa2e7-4a45-468e-ab13-3f6193eb9394)、[FibRgFcLcb97 字段](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/0c9df81f-98d0-454e-ad84-b612cd05b1a4)
- [SttbfFfn](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/18b7d35b-ad29-4723-893b-82aa30c64ced)、[FFN](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/ff407d64-3478-4b56-9b98-6dbcfc66a4ae)
- [STSHI](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/982c0460-4958-4f72-8cec-21b3ac15bd8e)、[StdfBase](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/df0f4654-071d-442f-8563-752d7e0285ef)
- [Pcd](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/498993c9-0a2d-47aa-8ada-fed27616e275)：`fR1` / `fR2` 必须忽略；不能把真实文件的 `0x0030` / `0x0070` 无定义位当作活动字体修改
- [Character SPRMs](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/7022285b-9621-42e9-ad4d-4e02c115ef18)：规范字号缺省、直接字号及字体选择
